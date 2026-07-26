//! Main-document body parsing into v1 block nodes.

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::v1::{
    Alignment, AltChunk, AltChunkProperties, AnchorHorizontal, AnchorVertical, AnchoredDrawing,
    BlockNode, BlockSdt, Bookmark, BookmarkEnd, BookmarkId, BookmarkStart, BorderEdge, Break,
    BreakKind, CellVerticalAlignment, CnfStyle, ColorScheme, ColumnDef, Comment, CommentId,
    CommentRangeEnd, CommentRangeStart, CommentReference, DefinitionMap, DocGrid, DocGridType,
    Drawing, DrawingAnchor, EmbeddedKind, EmbeddedObject, EmbeddedPart, Extent, ExternalTarget,
    Field, FormCheckBox, FormCheckBoxSize, FormDropDown, FormFieldData, FormFieldKind,
    FormTextInput, FormTextType, GridColumn, GroupChild, GroupPicture, GroupShape, GroupTextBox,
    GroupTransform, HeaderFooterId, HeaderFooterKind, HeaderFooterRef, HeightRule, HorizontalAlign,
    HorizontalAnchor, HorizontalPosition, Hyperlink, HyperlinkTarget, InlineNode, InlineSdt,
    InternalTarget, LineNumberRestart, LineNumbering, MAX_DESCR_BYTES, MAX_EMU,
    MAX_FIELD_INSTRUCTION_BYTES, MAX_FORM_FIELD_ENTRIES, MAX_FORM_FIELD_STRING_BYTES,
    MAX_MATH_BYTES, MAX_REVISION_DEPTH, MAX_SDT_DEPTH, MAX_TEXTBOX_DEPTH, Math, MediaId, MoveKind,
    MoveRangeEnd, MoveRangeStart, NoBreakHyphen, NoteId, NoteKind, NoteNumberRestart, NotePosition,
    NoteProperties, NoteReference, PageBorderDisplay, PageBorderOffset, PageBorders, PageMargins,
    PageNumbering, PageOrientation, PageSize, PageVerticalAlignment, PaperSource, Paragraph,
    ParagraphProperties, PointEmu, PositionalTab, PositionalTabAlignment, PositionalTabLeader,
    PositionalTabRelativeTo, PropChange, Revision, RevisionKind, RgbColor, Rgba, Run,
    RunProperties, SchemeColor, SdtCheckbox, SdtCheckboxSymbol, SdtControlData, SdtControlKind,
    SdtDataBinding, SdtDate, SdtListItem, SdtLock, SdtProperties, SectionBoundary, SectionColumns,
    SectionId, SectionType, ShapeGeometry, ShapeStroke, SoftHyphen, StyleKind, Symbol, Tab,
    TabAlignment, TabLeader, TabStop, TableAnchor, TableCellProperties, TableFloatPosition,
    TableLayout, TableOverlap, TableProperties, TableRowProperties, TableXAlign, TableYAlign,
    TextBox, TextDirection, VerticalAlign, VerticalAnchor, VerticalMerge, VerticalPosition,
    WordprocessingGroup, WrapMode,
};
use casual_doc_model::{IdGenerator, NodeId};
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::numbering::Numbering;
use crate::properties::{
    apply_paragraph_property, apply_run_property, attribute_value, break_kind, is_true, parse_rgb,
    symbol_glyph,
};
use crate::report::Reporter;
use crate::styles::Styles;
use crate::tables::TableStack;
use crate::vml::{
    VmlColor, VmlDrawing, VmlFill, VmlPosition, VmlRelFrame, VmlShapeKind, VmlStroke,
    parse_vml_pict,
};

/// A run/tab/break/drawing/hyperlink/field segment before ids and normalization.
// `Run` is the largest and most common variant (it holds the full
// `RunProperties`); boxing it would add a heap allocation per run on the hot
// import path, so the size difference is accepted (mirrors the model's
// `InlineNode`).
#[allow(clippy::large_enum_variant)]
enum Segment {
    Run {
        properties: RunProperties,
        text: String,
    },
    Tab,
    Break(BreakKind),
    Drawing {
        media: MediaId,
        extent: Option<Extent>,
    },
    /// A first-class embedded object (chart / SmartArt diagram / OLE object).
    EmbeddedObject {
        kind: EmbeddedKind,
        part: EmbeddedPart,
        extra_parts: Vec<EmbeddedPart>,
        preview: Option<MediaId>,
        extent: Extent,
        prog_id: Option<String>,
    },
    Hyperlink {
        target: HyperlinkTarget,
        tooltip: Option<String>,
        children: Vec<Segment>,
    },
    Field {
        instruction: String,
        children: Vec<Segment>,
        /// Legacy form-field configuration (`w:ffData`), if this field carried one.
        form: Option<FormFieldData>,
    },
    /// A fully-built text box (its inner ids are already allocated).
    TextBox(TextBox),
    /// A fully-built DrawingML group (its inner ids are already allocated).
    Group(WordprocessingGroup),
    /// A reference to a footnote or endnote definition.
    NoteReference {
        kind: NoteKind,
        note: NoteId,
    },
    /// A reference to a comment definition.
    CommentReference {
        comment: CommentId,
    },
    /// The start marker of a comment's anchored range.
    CommentRangeStart {
        comment: CommentId,
    },
    /// The end marker of a comment's anchored range.
    CommentRangeEnd {
        comment: CommentId,
    },
    /// A tracked-change (insertion/deletion) range wrapping inline content.
    Revision {
        kind: RevisionKind,
        author: Option<String>,
        date: Option<String>,
        revision_id: Option<String>,
        children: Vec<Segment>,
    },
    /// The start marker of a bookmark range.
    BookmarkStart {
        bookmark: BookmarkId,
    },
    /// The end marker of a bookmark range.
    BookmarkEnd {
        bookmark: BookmarkId,
    },
    /// The start marker of a tracked-move (source/destination) range.
    MoveRangeStart {
        kind: MoveKind,
        move_id: String,
        name: String,
        author: Option<String>,
        date: Option<String>,
    },
    /// The end marker of a tracked-move (source/destination) range.
    MoveRangeEnd {
        kind: MoveKind,
        move_id: String,
    },
    /// An inline-level content control (`w:sdt`) wrapping inline content.
    Sdt {
        properties: SdtProperties,
        children: Vec<Segment>,
    },
    /// An opaque math object: the retained OMML subtree plus its text fallback.
    Math {
        omml: String,
        text: String,
    },
    /// A symbol glyph (`w:sym`): a font face plus a code point.
    Symbol {
        font: String,
        char: u32,
    },
    /// A non-breaking hyphen glyph (`w:noBreakHyphen`).
    NoBreakHyphen,
    /// A soft (optional) hyphen glyph (`w:softHyphen`).
    SoftHyphen,
    /// An absolute-position tab (`w:ptab`).
    PositionalTab {
        alignment: PositionalTabAlignment,
        relative_to: PositionalTabRelativeTo,
        leader: PositionalTabLeader,
    },
    /// An anchored (floating) drawing: an embedded picture plus its placement.
    AnchoredDrawing {
        media: MediaId,
        extent: Extent,
        anchor: DrawingAnchor,
        descr: Option<String>,
        relative_height: Option<u32>,
    },
}

/// The natural size used for an embedded object whose producer declared none.
const ZERO_EXTENT: Extent = Extent {
    width_emu: 0,
    height_emu: 0,
};

/// Which axis of an anchor a `wp:posOffset`/`wp:align` value belongs to (the
/// enclosing `wp:positionH` or `wp:positionV`).
#[derive(Clone, Copy)]
enum AnchorAxis {
    Horizontal,
    Vertical,
}

/// The `wp:anchor` position/wrap/z-order pointers collected while parsing an open
/// anchored drawing, resolved into an [`AnchoredDrawing`] when the drawing closes.
/// The `pic:pic`'s `a:blip@r:embed` and `wp:extent` flow through the shared
/// `pending_embed`/`pending_extent` fields (an anchored picture is an inline
/// picture with a placement).
#[derive(Default)]
struct PendingAnchor {
    /// `@behindDoc` (z-order behind the text).
    behind_doc: bool,
    /// `@relativeHeight` (the monotonic z-order key within the behind/front band).
    relative_height: Option<u32>,
    /// `wp:positionH@relativeFrom`.
    h_relative: Option<HorizontalAnchor>,
    /// The resolved horizontal offset or alignment (`wp:posOffset`/`wp:align`).
    h_position: Option<HorizontalPosition>,
    /// `wp:positionV@relativeFrom`.
    v_relative: Option<VerticalAnchor>,
    /// The resolved vertical offset or alignment.
    v_position: Option<VerticalPosition>,
    /// The `wp:wrap*` mode.
    wrap: Option<WrapMode>,
    /// The `wp:docPr@descr` alt text (bounded).
    descr: Option<String>,
    /// The axis whose `wp:posOffset`/`wp:align` text is currently being captured.
    capture_axis: Option<AnchorAxis>,
    /// Whether the value being captured is a `wp:posOffset` (`true`) or a
    /// `wp:align` (`false`).
    capture_is_offset: bool,
    /// The text of the value currently being captured (bounded).
    capture_buffer: String,
}

impl PendingAnchor {
    /// Resolves the collected pointers into a [`DrawingAnchor`], applying OOXML
    /// defaults for any component the producer omitted (a missing `positionH`
    /// defaults to a zero offset from the column; a missing `positionV` to a zero
    /// offset from the paragraph; a missing wrap to `wrapNone`).
    fn resolve(&self) -> DrawingAnchor {
        DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: self.h_relative.unwrap_or(HorizontalAnchor::Column),
                position: self.h_position.unwrap_or(HorizontalPosition::Offset(0)),
            },
            vertical: AnchorVertical {
                relative_from: self.v_relative.unwrap_or(VerticalAnchor::Paragraph),
                position: self.v_position.unwrap_or(VerticalPosition::Offset(0)),
            },
            wrap: self.wrap.unwrap_or(WrapMode::None),
            behind_doc: self.behind_doc,
        }
    }
}

/// A DrawingML group (`wpg:wgp` / `wpg:grpSp`) being accumulated while its subtree
/// is parsed. The transform is filled from the group's `grpSpPr>a:xfrm`; children
/// are pushed in document order as each `pic:pic`/`wps:wsp`/nested `wpg:grpSp`
/// closes. The top-of-stack group is the innermost open one.
struct GroupBuilder {
    id: NodeId,
    /// `Some` for the top-level `wpg:wgp` (the anchored object); `None` for a
    /// nested `wpg:grpSp`. Carries the `wp:anchor` + `wp:extent` + `relativeHeight`.
    anchor: Option<(DrawingAnchor, Extent, Option<u32>)>,
    transform: GroupTransform,
    children: Vec<GroupChild>,
}

/// A `pic:pic` or `wps:wsp`/`wps:cxnSp` shape being accumulated inside a group (or
/// a lone/inline DrawingML text box). Its geometry/fill/stroke come from `spPr`;
/// `textbox_blocks` is `Some` once a `w:txbxContent` closed inside it.
struct ShapeBuilder {
    id: NodeId,
    /// `true` for a `pic:pic` (a picture child), `false` for a `wps:wsp`/`wps:cxnSp`.
    is_picture: bool,
    offset: PointEmu,
    extent: Extent,
    geometry: ShapeGeometry,
    fill: Option<Rgba>,
    stroke: Option<ShapeStroke>,
    /// The picture's `a:blip@r:embed`, for a picture child.
    embed: Option<String>,
    /// The alt text (`pic:cNvPr@descr` / `wps:cNvPr@descr`), if declared.
    descr: Option<String>,
    /// The flowed block content of a `w:txbxContent` inside this shape, if any.
    textbox_blocks: Option<Vec<BlockNode>>,
}

/// Which `a:xfrm` an `a:off`/`a:ext`/`a:chOff`/`a:chExt` currently routes to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum XfrmTarget {
    /// Not inside a shape/group transform (values ignored).
    None,
    /// The top group builder's transform (`grpSpPr>a:xfrm`).
    Group,
    /// The open shape builder's transform (`spPr>a:xfrm`).
    Shape,
}

/// Whether an open DrawingML color element paints a shape's fill or its outline.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorDest {
    Fill,
    Stroke,
}

/// A DrawingML color (`a:srgbClr`/`a:schemeClr`/`a:sysClr`) being accumulated,
/// with its luminance/tint/shade/alpha child modifiers, until the element closes.
struct PendingColor {
    dest: ColorDest,
    base: [u8; 4],
    lum_mod: Option<f32>,
    lum_off: Option<f32>,
    tint: Option<f32>,
    shade: Option<f32>,
    alpha: Option<f32>,
    /// The `a:ln@w` outline width (EMU) in scope when `dest == Stroke`.
    stroke_width_emu: i64,
}

/// A main-document relationship an embedded object can reference, resolved from
/// the package (`r:id` -> part). The `r:id` is the lookup key.
#[derive(Clone, Debug)]
pub(crate) struct EmbeddedRel {
    /// Relationship type URI (`.../chart`, `.../diagramData`, `.../oleObject`, …).
    pub relationship_type: String,
    /// Resolved package part name (e.g. `word/charts/chart1.xml`).
    pub part_name: String,
}

/// The `a:graphicData` payload pointers collected while parsing an open
/// `w:drawing`, resolved into an embedded-object node when the drawing closes.
#[derive(Default)]
struct PendingGraphic {
    /// The `a:graphicData@uri` (identifies chart vs diagram vs other).
    uri: Option<String>,
    /// A `c:chart@r:id` (a chart payload).
    chart_rid: Option<String>,
    /// A `dgm:relIds` data-model id (`@r:dm`).
    diagram_dm: Option<String>,
    /// A `dgm:relIds` layout id (`@r:lo`).
    diagram_lo: Option<String>,
    /// A `dgm:relIds` quick-style id (`@r:qs`).
    diagram_qs: Option<String>,
    /// A `dgm:relIds` colors id (`@r:cs`).
    diagram_cs: Option<String>,
}

/// The `w:object` pointers collected while parsing an open OLE object, resolved
/// into an embedded-object node when the object closes.
#[derive(Default)]
struct PendingObject {
    /// The `o:OLEObject@r:id` (the embedding part).
    object_rid: Option<String>,
    /// The `o:OLEObject@ProgID`.
    prog_id: Option<String>,
    /// The `v:imagedata@r:id` preview image (resolves through the media table).
    preview_rid: Option<String>,
    /// The natural size from `w:object@w:dxaOrig`/`@w:dyaOrig` (twips → EMU).
    extent: Option<Extent>,
}

/// The open `w:altChunk`: the resolved chunk part and the properties gathered
/// from its `w:altChunkPr` until the element closes.
struct PendingAltChunk {
    /// The referenced chunk part (`w:altChunk@r:id`, resolved to a part).
    part: EmbeddedPart,
    /// The accumulated `w:altChunkPr` properties.
    properties: AltChunkProperties,
}

/// Which wrapper is currently the innermost open one. A single discriminator
/// stack records the relative open order of the three inline wrappers so a
/// segment routes into whichever nests deepest, regardless of kind — hyperlinks
/// and revisions nest in either order, and revisions nest within themselves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WrapperKind {
    Hyperlink,
    Field,
    Revision,
    Sdt,
}

/// The position an open `w:sdt` occupies, decided from parser state at `<w:sdt>`,
/// so its `</w:sdtContent>`/`</w:sdt>` route by scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SdtScope {
    /// A content control around runs (routes via `wrapper_order`, like a revision).
    Inline,
    /// A content control around blocks (a suspended `ContentFrame`, like a box).
    Block,
    /// A deferred position (row/cell-structural, invalid nesting, or over bound):
    /// reported once, its inner content flows to the current container unchanged.
    Passthrough,
}

/// Which kind of construct a suspended [`ContentFrame`] is building.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FrameKind {
    /// A text box (`w:txbxContent`): emits an inline `TextBox` segment on exit.
    TextBox,
    /// A block content control (`w:sdt`): emits a `BlockNode::Sdt` on exit.
    BlockSdt,
}

/// An inline content control (`w:sdt`) being accumulated, innermost last.
struct SdtAccumulator {
    properties: SdtProperties,
    segments: Vec<Segment>,
}

/// Which table property container (if any) is currently capturing edge children.
/// The edge element names (`top`/`start`/`bottom`/`end`/`left`/`right`) collide
/// between the border containers (`w:tblBorders`/`w:tcBorders`) and the margin
/// containers (`w:tblCellMar`/`w:tcMar`), so this scope disambiguates them; the
/// table-vs-cell level is taken from `tblpr_depth`/`tcpr_depth`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EdgeScope {
    None,
    Borders,
    Margins,
    /// A `w:pBdr` paragraph-border container (edges route to the paragraph).
    ParagraphBorders,
    /// A `w:pgBorders` section page-border container (edges route to the section).
    PageBorders,
}

/// A tracked-change range (`w:ins`/`w:del`) being accumulated.
struct RevisionAccumulator {
    kind: RevisionKind,
    author: Option<String>,
    date: Option<String>,
    revision_id: Option<String>,
    segments: Vec<Segment>,
}

/// Opaque metadata captured from a `w:*PrChange`'s attributes (`w:author`,
/// `w:date`, `w:id`), bounded exactly like `w:ins`/`w:del` revision metadata.
struct PropChangeMeta {
    author: Option<String>,
    date: Option<String>,
    revision_id: Option<String>,
}

impl PropChangeMeta {
    fn from_element(element: &BytesStart<'_>) -> Self {
        Self {
            author: attribute_value(element, b"author")
                .filter(|value| !value.is_empty() && value.len() <= 255),
            date: attribute_value(element, b"date")
                .filter(|value| !value.is_empty() && value.len() <= 64),
            revision_id: attribute_value(element, b"id")
                .filter(|value| !value.is_empty() && value.len() <= 64),
        }
    }

    fn into_change<P>(self, prior: P) -> PropChange<P> {
        PropChange {
            author: self.author,
            date: self.date,
            revision_id: self.revision_id,
            prior: Box::new(prior),
        }
    }
}

/// An open property-change tracked revision (`w:*PrChange`) being captured. The
/// PRIOR snapshot accumulates into the live accumulator (reset to `Default` on
/// open) through the same element routing as the current properties; `saved`
/// holds the just-completed CURRENT properties, restored — with the built
/// `prop_change`/`grid_change` attached — on the matching close.
enum PropChangeCapture {
    /// A run's `w:rPrChange`. `mark` distinguishes a paragraph-mark run's rPr
    /// (`mark_run_properties`) from a normal run's rPr (`run_properties`).
    Run {
        meta: PropChangeMeta,
        saved: RunProperties,
        mark: bool,
    },
    /// A paragraph's `w:pPrChange`.
    Paragraph {
        meta: PropChangeMeta,
        saved: ParagraphProperties,
    },
    /// A table's `w:tblPrChange`.
    Table {
        meta: PropChangeMeta,
        saved: TableProperties,
    },
    /// A row's `w:trPrChange`.
    Row {
        meta: PropChangeMeta,
        saved: TableRowProperties,
    },
    /// A cell's `w:tcPrChange`.
    Cell {
        meta: PropChangeMeta,
        saved: TableCellProperties,
    },
    /// A table grid's `w:tblGridChange`.
    Grid {
        meta: PropChangeMeta,
        saved: Vec<GridColumn>,
    },
}

/// Optional comment metadata captured from a `w:comment`'s attributes (all
/// `None` for footnotes/endnotes, which reuse the same container machinery).
#[derive(Default)]
struct CommentMeta {
    author: Option<String>,
    date: Option<String>,
    initials: Option<String>,
}

/// The content-building state suspended while parsing a text box's own content,
/// restored when the text box closes. A text box (`w:txbxContent`) carries a full
/// block sequence, so parsing it requires a fresh paragraph/run/table context;
/// suspending and restoring the enclosing context keeps the outer paragraph and
/// the enclosing drawing (its image) intact.
struct ContentFrame {
    /// What this frame builds (a text box or a block content control).
    kind: FrameKind,
    /// The allocated id of the emitted node (text box or block sdt).
    node_id: NodeId,
    /// The block sdt's control properties (unused for a text box).
    sdt_properties: SdtProperties,
    /// Text-box nesting depth at this frame (a text-box-only path counter, so
    /// intervening block-sdt frames never inflate the `MAX_TEXTBOX_DEPTH` check).
    depth: u32,
    paragraph_open: bool,
    paragraph_id: Option<NodeId>,
    paragraph_properties: ParagraphProperties,
    ppr_depth: u32,
    numpr_depth: u32,
    pending_num_id: Option<String>,
    pending_ilvl: u8,
    run_open: bool,
    run_properties: RunProperties,
    rpr_depth: u32,
    in_text: bool,
    text_buffer: String,
    drawing_depth: u32,
    blipfill_depth: u32,
    pending_embed: Option<String>,
    pending_extent: Option<Extent>,
    drawing_extra: bool,
    pict_depth: u32,
    pending_graphic: PendingGraphic,
    pending_anchor: Option<PendingAnchor>,
    object_depth: u32,
    pending_object: PendingObject,
    group_stack: Vec<GroupBuilder>,
    pending_shape: Option<ShapeBuilder>,
    pending_group: Option<WordprocessingGroup>,
    /// The open `w:altChunk`, if any: its resolved chunk part plus the properties
    /// (`w:altChunkPr`) accumulated until `</w:altChunk>` commits the block.
    pending_alt_chunk: Option<PendingAltChunk>,
    hyperlink: Option<HyperlinkAccumulator>,
    hyperlink_depth: u32,
    field: Option<FieldAccumulator>,
    field_depth: u32,
    in_instr: bool,
    instr_buffer: String,
    ruby_annotation_depth: u32,
    revisions: Vec<RevisionAccumulator>,
    suppressed_revision_depth: u32,
    wrapper_order: Vec<WrapperKind>,
    tables: TableStack,
    tcpr_depth: u32,
    tblpr_depth: u32,
    trpr_depth: u32,
    pr_change_depth: u32,
    edge_scope: EdgeScope,
    in_tabs: bool,
    mark_rpr_depth: u32,
    /// Whether a paragraph-mark `w:rPr` was seen (so a present-but-empty mark is
    /// preserved as `Some(default)`), and its accumulated run properties.
    mark_rpr_seen: bool,
    mark_run_properties: RunProperties,
    suppressed_tbl_depth: u32,
    sdts: Vec<SdtAccumulator>,
    sdt_scopes: Vec<SdtScope>,
    pending_block_sdt_props: Vec<SdtProperties>,
    sdt_prop_depth: u32,
    segments: Vec<Segment>,
    blocks: Vec<BlockNode>,
}

/// A hyperlink being accumulated while inside a `w:hyperlink`.
struct HyperlinkAccumulator {
    target: HyperlinkTarget,
    tooltip: Option<String>,
    segments: Vec<Segment>,
}

/// A field being accumulated (simple `w:fldSimple` or complex `fldChar`).
struct FieldAccumulator {
    /// The field instruction (`w:instr` or concatenated `w:instrText`).
    instruction: String,
    /// Whether we are past the `separate` boundary (collecting the cached
    /// result). A simple field starts in the result state; a complex field
    /// starts collecting its instruction.
    in_result: bool,
    /// Cached-result segments.
    segments: Vec<Segment>,
    /// Legacy form-field configuration accumulated from a `w:ffData` block (only
    /// a complex `fldChar begin` field can carry one). `None` until `w:ffData`
    /// opens; finalized into `FormFieldData` when the field commits.
    form: Option<FormFieldBuilder>,
}

/// A legacy form field's configuration (`w:ffData`) while it is being parsed.
/// The kind-specific payload is unknown until its `w:textInput` / `w:checkBox` /
/// `w:ddList` container opens, so `kind` is optional here and required only in
/// the finalized [`FormFieldData`].
#[derive(Default)]
struct FormFieldBuilder {
    name: Option<String>,
    enabled: Option<bool>,
    calc_on_exit: Option<bool>,
    help_text: Option<String>,
    status_text: Option<String>,
    entry_macro: Option<String>,
    exit_macro: Option<String>,
    kind: Option<FormFieldKind>,
}

/// Raw section geometry accumulated while inside a `w:sectPr`.
#[derive(Default)]
struct SectionAccumulator {
    page_width: Option<i32>,
    page_height: Option<i32>,
    margin_top: Option<i32>,
    margin_bottom: Option<i32>,
    margin_start: Option<i32>,
    margin_end: Option<i32>,
    margin_header: Option<i32>,
    margin_footer: Option<i32>,
    columns: Option<u16>,
    column_space: Option<i32>,
    column_separator: Option<bool>,
    column_equal_width: Option<bool>,
    column_defs: Vec<ColumnDef>,
    section_type: Option<SectionType>,
    title_page: Option<bool>,
    vertical_alignment: Option<PageVerticalAlignment>,
    page_number_format: Option<String>,
    page_number_start: Option<i32>,
    doc_grid_type: Option<DocGridType>,
    doc_grid_line_pitch: Option<i32>,
    doc_grid_char_space: Option<i32>,
    headers: Vec<HeaderFooterRef>,
    footers: Vec<HeaderFooterRef>,
    orientation: Option<PageOrientation>,
    paper_first: Option<i32>,
    paper_other: Option<i32>,
    page_border_display: Option<PageBorderDisplay>,
    page_border_offset: Option<PageBorderOffset>,
    page_border_top: Option<BorderEdge>,
    page_border_bottom: Option<BorderEdge>,
    page_border_start: Option<BorderEdge>,
    page_border_end: Option<BorderEdge>,
    line_count_by: Option<i32>,
    line_start: Option<i32>,
    line_distance: Option<i32>,
    line_restart: Option<LineNumberRestart>,
    footnote_props: NoteProperties,
    endnote_props: NoteProperties,
    text_direction: Option<TextDirection>,
    bidi: bool,
}

/// Which per-section note-properties container (if any) is open, so its
/// `w:pos`/`w:numFmt`/`w:numStart`/`w:numRestart` children route to the section's
/// footnote or endnote properties.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SectionNoteScope {
    Footnote,
    Endnote,
}

struct BodyParser<'a> {
    ids: &'a mut IdGenerator,
    styles: &'a Styles,
    numbering: &'a Numbering,
    reporter: &'a mut Reporter,
    config: ImportConfig,
    /// Resolution index: image relationship id -> the media table entry.
    media_index: &'a BTreeMap<String, MediaId>,
    /// Resolution index: hyperlink relationship id -> external target URL.
    hyperlink_rels: &'a BTreeMap<String, String>,
    /// Resolution index: relationship id -> an embedded-object part
    /// (chart/diagram/OLE). Empty for parts that carry no such index.
    embedded_index: &'a BTreeMap<String, EmbeddedRel>,
    /// Package part names an embedded-object node references (accumulated across
    /// the whole parse). The caller un-orphans exactly these in the side-table so
    /// the writer emits their relationship once (from the node), not twice.
    embedded_part_names: BTreeSet<String>,
    elements: u64,
    depth: u64,
    text_bytes: usize,
    in_document: bool,
    in_body: bool,
    /// The page background color (`w:background`), captured before the body opens.
    page_background: Option<RgbColor>,
    paragraph_open: bool,
    paragraph_id: Option<NodeId>,
    paragraph_properties: ParagraphProperties,
    ppr_depth: u32,
    numpr_depth: u32,
    pending_num_id: Option<String>,
    pending_ilvl: u8,
    run_open: bool,
    run_properties: RunProperties,
    rpr_depth: u32,
    in_text: bool,
    text_buffer: String,
    drawing_depth: u32,
    blipfill_depth: u32,
    pending_embed: Option<String>,
    pending_extent: Option<Extent>,
    drawing_extra: bool,
    /// Depth of an open `w:pict` (legacy VML picture); `pending_embed` holds its
    /// `v:imagedata@r:id` until the picture closes.
    pict_depth: u32,
    /// Raw-XML re-serializer for the currently-open `w:pict` subtree, mirroring
    /// every event so the closed pict can be re-parsed by
    /// [`parse_vml_pict`](crate::vml::parse_vml_pict) for its positioned VML shapes
    /// (rules, callout boxes, header text boxes, images). `Some` only while a `pict`
    /// is open; loop-level state independent of the text-box frame stack (a VML text
    /// box's `w:txbxContent` closes *inside* the pict, so capture must span it).
    vml_capture: Option<Writer<Vec<u8>>>,
    /// Nesting depth of `w:pict` elements captured into [`Self::vml_capture`], so a
    /// (pathological) nested pict does not finalize the outer capture early.
    vml_capture_depth: u32,
    /// The raw XML of the most-recently-closed `w:pict`, handed to
    /// [`Self::commit_pict`] to map its VML drawings onto the float layer.
    pending_pict_xml: Option<String>,
    /// `a:graphicData` payload pointers for the open drawing (chart/diagram).
    pending_graphic: PendingGraphic,
    /// `wp:anchor` placement pointers for the open drawing; `Some` while inside a
    /// floating anchor, resolved into an [`AnchoredDrawing`] when the drawing
    /// closes.
    pending_anchor: Option<PendingAnchor>,
    /// Depth of an open `w:object` (an OLE object wrapper).
    object_depth: u32,
    /// `w:object` pointers collected until the object closes.
    pending_object: PendingObject,
    /// Open DrawingML groups (`wpg:wgp`/`wpg:grpSp`), innermost last. Non-empty
    /// while a group subtree is being parsed; the finished top-level group lands in
    /// `pending_group`. Saved/restored across a text-box frame so a drawing nested
    /// inside a text box builds a fresh group context.
    group_stack: Vec<GroupBuilder>,
    /// The open `pic:pic`/`wps:wsp` shape being accumulated (its `spPr` geometry/
    /// fill/stroke and, for a text box, its flowed blocks). Saved/restored across a
    /// text-box frame.
    pending_shape: Option<ShapeBuilder>,
    /// The finished top-level group (`wpg:wgp`), pending until the enclosing
    /// `w:drawing` closes and `commit_drawing` emits it. Saved/restored across a
    /// text-box frame.
    pending_group: Option<WordprocessingGroup>,
    /// Which `a:xfrm` the next `a:off`/`a:ext`/`a:chOff`/`a:chExt` routes to.
    xfrm_target: XfrmTarget,
    /// Depth of an open `a:ln` (outline), so a `solidFill` inside it colors the
    /// stroke rather than the fill.
    ln_depth: u32,
    /// Depth of an open shape-style reference (`a:lnRef`/`a:fillRef`/`a:effectRef`/
    /// `a:fontRef` in `wps:style`). A `schemeClr` inside one selects a theme style
    /// index, NOT the shape's actual fill/stroke, so color capture is suppressed.
    style_ref_depth: u32,
    /// The `a:ln@w` outline width (EMU) of the open outline.
    ln_width_emu: i64,
    /// The open DrawingML color element being accumulated (base + modifiers).
    pending_color: Option<PendingColor>,
    /// The resolved 12-slot theme color palette (DrawingML `schemeClr` targets),
    /// in `ColorScheme` field order (dk1, lt1, dk2, lt2, accent1..6, hlink, folHlink).
    palette: [[u8; 4]; 12],
    /// The open `w:altChunk`, if any: its resolved chunk part plus the properties
    /// (`w:altChunkPr`) accumulated until `</w:altChunk>` commits the block.
    pending_alt_chunk: Option<PendingAltChunk>,
    hyperlink: Option<HyperlinkAccumulator>,
    hyperlink_depth: u32,
    /// The open field, if any (simple or complex). Mutually exclusive with an
    /// open hyperlink (a hyperlink and a field never open inside one another).
    field: Option<FieldAccumulator>,
    /// Nesting depth of `<w:fldSimple>` / complex `fldChar` fields, so a
    /// missing/extra delimiter cannot desynchronize field commits.
    field_depth: u32,
    /// Whether we are inside a legacy form field's `w:ffData` block, so its
    /// children route onto the open field's `FormFieldBuilder`.
    in_ffdata: bool,
    /// Whether we are inside a `w:instrText` (its text builds the instruction).
    in_instr: bool,
    /// Buffer for the current `w:instrText` text.
    instr_buffer: String,
    /// Nesting depth of open ruby annotations (`w:rt`), whose text is dropped;
    /// a counter so a nested `w:rt` does not clear it early.
    ruby_annotation_depth: u32,
    /// Open tracked-change (`w:ins`/`w:del`) ranges, innermost last.
    revisions: Vec<RevisionAccumulator>,
    /// Depth of `w:ins`/`w:del` elements that were NOT modeled as run ranges (a
    /// property-context marker, an over-`MAX_REVISION_DEPTH` range, or one outside
    /// a paragraph). A close-side counter — like `field_depth`/`hyperlink_depth` —
    /// so an excluded range's `</w:ins>`/`</w:del>` is balanced and never commits
    /// an enclosing real revision. Excluded ranges are always inner to any open
    /// real revision (valid OOXML never wraps a paragraph in `w:ins`/`w:del`), so
    /// the matching close arrives before the enclosing revision's.
    suppressed_revision_depth: u32,
    /// The relative open order of the hyperlink, field, and revision wrappers, so
    /// a segment routes into the innermost regardless of kind. Its top identifies
    /// which accumulator (`hyperlink`, `field`, or `revisions.last()`) receives
    /// the next segment.
    wrapper_order: Vec<WrapperKind>,
    tables: TableStack,
    tcpr_depth: u32,
    /// Depth of an open `w:tblPr` / `w:trPr` (table / row property container), so
    /// their property children map into the right level.
    tblpr_depth: u32,
    trpr_depth: u32,
    /// Depth of an open reported-and-skipped property-change (`w:sectPrChange` /
    /// `w:numberingChange`), whose nested historical container must not map over
    /// the current values (the modeled changes use `prop_change` instead).
    pr_change_depth: u32,
    /// Open modeled property-change tracked revisions (`w:*PrChange`) being
    /// captured. A stack for robustness against malformed nesting; well-formed
    /// input keeps at most one entry (a `w:*PrChange` is the last child of its
    /// `w:*Pr`). A modeled change never wraps a text box, so it is not saved or
    /// restored across a content frame.
    prop_change: Vec<PropChangeCapture>,
    /// Which table border/margin container is currently open (for edge routing).
    /// A well-formed edge container has no box content, but malformed markup can
    /// nest a `w:txbxContent` inside one; it is saved/restored across a text-box
    /// frame (like the depth counters) so an inner table's edge container cannot
    /// clobber the outer scope and drop the enclosing table's borders.
    edge_scope: EdgeScope,
    /// Which per-section note-properties container (`w:footnotePr`/`w:endnotePr`)
    /// is open, so its child elements route to the section accumulator. A section
    /// never appears inside a text-box frame, so this is not saved/restored.
    section_note_scope: Option<SectionNoteScope>,
    /// Whether a `w:tabs` container is open (its `w:tab` children are tab stops,
    /// not the inline run tab). Never spans a text box.
    in_tabs: bool,
    /// Depth of a paragraph-mark `w:rPr` (opened inside `w:pPr` with no run open):
    /// its children are the pilcrow's run properties, so a `w:shd` there is NOT
    /// paragraph shading. Never spans a text box.
    mark_rpr_depth: u32,
    /// Whether a paragraph-mark `w:rPr` was seen, and its accumulated properties.
    mark_rpr_seen: bool,
    mark_run_properties: RunProperties,
    /// Depth of nested tables refused past `MAX_TABLE_DEPTH`; while non-zero the
    /// table structure is suppressed so it cannot corrupt the enclosing table.
    suppressed_tbl_depth: u32,
    /// Open inline content controls (`w:sdt` around runs), innermost last; each
    /// receives segments via `wrapper_order` exactly like a revision.
    sdts: Vec<SdtAccumulator>,
    /// Scope of every currently-open `w:sdt` (inline/block/passthrough), so its
    /// `</w:sdtContent>`/`</w:sdt>` route by scope.
    sdt_scopes: Vec<SdtScope>,
    /// Properties of open block content controls awaiting their `w:sdtContent`.
    pending_block_sdt_props: Vec<SdtProperties>,
    /// Depth inside a `w:sdtPr`/`w:sdtEndPr` subtree, so its property children
    /// (and any `w:rPr`) are captured or reported, never leaked into run flow.
    sdt_prop_depth: u32,
    /// Content-control nesting depth (a `w:sdt` inside a `w:sdt`); a true path
    /// counter for the `MAX_SDT_DEPTH` guard, NOT suspended in a frame so it
    /// matches the model (a text box does not reset it).
    sdt_depth: u32,
    /// Count of currently-open text-box frames — a text-box-only path counter for
    /// the `MAX_TEXTBOX_DEPTH` bound, so intervening block-sdt frames on `frames`
    /// never inflate it (the two frame kinds share `frames` but not this depth).
    open_textboxes: u32,
    segments: Vec<Segment>,
    blocks: Vec<BlockNode>,
    /// Suspended enclosing contexts, one per open text box or block content control.
    frames: Vec<ContentFrame>,
    /// Depth of an `mc:Choice`/`mc:Fallback` branch being skipped (a non-selected
    /// alternate representation); while non-zero, all events are ignored.
    mc_skip_depth: u32,
    /// Per-`mc:AlternateContent` "a branch was already selected" flags.
    alt_stack: Vec<bool>,
    section: Option<SectionAccumulator>,
    sections: Vec<SectionBoundary>,
    /// Body reference resolution: footnote source `w:id` -> deterministic id.
    footnote_ids: &'a BTreeMap<String, NoteId>,
    /// Body reference resolution: endnote source `w:id` -> deterministic id.
    endnote_ids: &'a BTreeMap<String, NoteId>,
    /// When set (`b"footnote"`/`b"endnote"`), the parser is reading a notes part
    /// and treats each note element as a block container.
    note_container: Option<&'static [u8]>,
    /// The open note's source `w:id` and allocated id (note-container mode).
    current_note: Option<(String, NodeId, CommentMeta)>,
    /// Whether the open note is a skipped separator/continuation note.
    skip_note: bool,
    /// Collected notes/comments: (source `w:id`, allocated id, metadata, blocks).
    notes: Vec<(String, NodeId, CommentMeta, Vec<BlockNode>)>,
    /// When set (`b"hdr"`/`b"ftr"`), the parser reads a header/footer part whose
    /// root element is the single block container.
    hf_root: Option<&'static [u8]>,
    /// Section reference resolution: header relationship id -> definition id.
    header_ids: &'a BTreeMap<String, HeaderFooterId>,
    /// Section reference resolution: footer relationship id -> definition id.
    footer_ids: &'a BTreeMap<String, HeaderFooterId>,
    /// Body reference resolution: comment source `w:id` -> definition id.
    comment_ids: &'a BTreeMap<String, CommentId>,
    /// Document-global bookmark definitions accumulator (threaded across every
    /// part so body + notes + headers + footers + comments land in one table).
    bookmarks: &'a mut DefinitionMap<BookmarkId, Bookmark>,
    /// Source `w:id` string -> allocated `BookmarkId`, for start/end pairing
    /// across paragraphs. Part-scoped (NOT swapped in `ContentFrame`): a bookmark
    /// opened in body flow and closed inside a text box still pairs.
    bookmark_ids: BTreeMap<String, BookmarkId>,
    /// Nesting depth inside an OMML math subtree (`m:oMath`/`m:oMathPara`); 0 when
    /// not capturing. While non-zero, every event is buffered verbatim into
    /// `math_writer` and NOT dispatched to the `w:`-namespace handlers, so a math
    /// run's `m:r`/`m:t` can never be mistaken for a `w:r`/`w:t`.
    math_depth: u32,
    /// The re-serializer that reconstructs the retained OMML subtree while
    /// `math_depth > 0`; taken (and reset) when the subtree's root closes.
    math_writer: Option<Writer<Vec<u8>>>,
    /// The best-effort plain-text fallback accumulated from the captured `m:t`
    /// runs (search/accessibility only; not authoritative).
    math_text: String,
    /// Whether the innermost open captured element is an `m:t` (so its text is
    /// collected into `math_text`).
    math_in_t: bool,
}

/// Resolution tables the body parser consults while mapping constructs.
pub(crate) struct ParseInputs<'a> {
    pub styles: &'a Styles,
    pub numbering: &'a Numbering,
    pub media_index: &'a BTreeMap<String, MediaId>,
    pub hyperlink_rels: &'a BTreeMap<String, String>,
    pub embedded_index: &'a BTreeMap<String, EmbeddedRel>,
    pub footnote_ids: &'a BTreeMap<String, NoteId>,
    pub endnote_ids: &'a BTreeMap<String, NoteId>,
    pub header_ids: &'a BTreeMap<String, HeaderFooterId>,
    pub footer_ids: &'a BTreeMap<String, HeaderFooterId>,
    pub comment_ids: &'a BTreeMap<String, CommentId>,
    /// The document's theme color scheme, against which a floating shape's
    /// `a:schemeClr` fill/outline resolves to a concrete color. `None` when no
    /// theme is available (scheme-colored shape fills then degrade to unresolved).
    pub color_scheme: Option<&'a ColorScheme>,
}

impl<'a> BodyParser<'a> {
    fn build(
        ids: &'a mut IdGenerator,
        reporter: &'a mut Reporter,
        inputs: &ParseInputs<'a>,
        bookmarks: &'a mut DefinitionMap<BookmarkId, Bookmark>,
        note_container: Option<&'static [u8]>,
        config: ImportConfig,
    ) -> Self {
        let palette = inputs.color_scheme.map(resolve_palette).unwrap_or_default();
        BodyParser {
            ids,
            styles: inputs.styles,
            numbering: inputs.numbering,
            reporter,
            config,
            media_index: inputs.media_index,
            hyperlink_rels: inputs.hyperlink_rels,
            embedded_index: inputs.embedded_index,
            embedded_part_names: BTreeSet::new(),
            elements: 0,
            depth: 0,
            text_bytes: 0,
            in_document: false,
            in_body: false,
            page_background: None,
            paragraph_open: false,
            paragraph_id: None,
            paragraph_properties: ParagraphProperties::default(),
            ppr_depth: 0,
            numpr_depth: 0,
            pending_num_id: None,
            pending_ilvl: 0,
            run_open: false,
            run_properties: RunProperties::default(),
            rpr_depth: 0,
            in_text: false,
            text_buffer: String::new(),
            drawing_depth: 0,
            blipfill_depth: 0,
            pending_embed: None,
            pending_extent: None,
            drawing_extra: false,
            pict_depth: 0,
            vml_capture: None,
            vml_capture_depth: 0,
            pending_pict_xml: None,
            pending_graphic: PendingGraphic::default(),
            pending_anchor: None,
            object_depth: 0,
            pending_object: PendingObject::default(),
            group_stack: Vec::new(),
            pending_shape: None,
            pending_group: None,
            xfrm_target: XfrmTarget::None,
            ln_depth: 0,
            ln_width_emu: 0,
            style_ref_depth: 0,
            pending_color: None,
            palette,
            pending_alt_chunk: None,
            hyperlink: None,
            hyperlink_depth: 0,
            field: None,
            field_depth: 0,
            in_ffdata: false,
            in_instr: false,
            instr_buffer: String::new(),
            ruby_annotation_depth: 0,
            revisions: Vec::new(),
            suppressed_revision_depth: 0,
            wrapper_order: Vec::new(),
            tables: TableStack::default(),
            tcpr_depth: 0,
            tblpr_depth: 0,
            trpr_depth: 0,
            pr_change_depth: 0,
            prop_change: Vec::new(),
            edge_scope: EdgeScope::None,
            section_note_scope: None,
            in_tabs: false,
            mark_rpr_depth: 0,
            mark_rpr_seen: false,
            mark_run_properties: RunProperties::default(),
            suppressed_tbl_depth: 0,
            sdts: Vec::new(),
            sdt_scopes: Vec::new(),
            pending_block_sdt_props: Vec::new(),
            sdt_prop_depth: 0,
            sdt_depth: 0,
            open_textboxes: 0,
            segments: Vec::new(),
            blocks: Vec::new(),
            frames: Vec::new(),
            mc_skip_depth: 0,
            alt_stack: Vec::new(),
            section: None,
            sections: Vec::new(),
            footnote_ids: inputs.footnote_ids,
            endnote_ids: inputs.endnote_ids,
            note_container,
            current_note: None,
            skip_note: false,
            notes: Vec::new(),
            hf_root: None,
            header_ids: inputs.header_ids,
            footer_ids: inputs.footer_ids,
            comment_ids: inputs.comment_ids,
            bookmarks,
            bookmark_ids: BTreeMap::new(),
            math_depth: 0,
            math_writer: None,
            math_text: String::new(),
            math_in_t: false,
        }
    }
}

/// The main-document body parse result: ordered block nodes, section boundaries,
/// and the package part names embedded-object nodes reference (so the caller can
/// un-orphan exactly those parts in the side-table).
pub(crate) struct BodyParse {
    pub blocks: Vec<BlockNode>,
    pub sections: Vec<SectionBoundary>,
    pub embedded_part_names: BTreeSet<String>,
    /// The page background color (`w:background@w:color`), if the document sets a
    /// concrete sRGB one. A theme/image background is reported (degraded), not
    /// carried here.
    pub page_background: Option<RgbColor>,
}

/// Parses main-document body bytes into ordered block nodes, allocating ids.
#[allow(clippy::too_many_arguments)]
pub(crate) fn parse<'a>(
    xml: &[u8],
    ids: &'a mut IdGenerator,
    reporter: &'a mut Reporter,
    inputs: ParseInputs<'a>,
    bookmarks: &'a mut DefinitionMap<BookmarkId, Bookmark>,
    config: ImportConfig,
) -> Result<BodyParse, ImportError> {
    let mut parser = BodyParser::build(ids, reporter, &inputs, bookmarks, None, config);
    parser.run(xml)?;
    // Unwind any text box left open by malformed input so the true body root is
    // restored, then finish a paragraph the unwind may have re-opened so its
    // content is committed, not stranded in a suspended frame.
    while !parser.frames.is_empty() {
        parser.exit_frame()?;
    }
    if parser.paragraph_open {
        parser.finish_paragraph()?;
    }
    // Commit any table left open by truncated body markup (its partial content
    // would otherwise be stranded in the `TableStack` at EOF).
    let roots = parser.tables.flush_open(&mut *parser.ids)?;
    parser.blocks.extend(roots);
    Ok(BodyParse {
        blocks: parser.blocks,
        sections: parser.sections,
        embedded_part_names: parser.embedded_part_names,
        page_background: parser.page_background,
    })
}

/// Parses a notes part (`word/footnotes.xml` / `word/endnotes.xml`) into its
/// notes, each keyed by its source `w:id` and allocated id in document order.
/// `container` is `b"footnote"` or `b"endnote"`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn parse_notes(
    xml: &[u8],
    ids: &mut IdGenerator,
    reporter: &mut Reporter,
    styles: &Styles,
    numbering: &Numbering,
    media_index: &BTreeMap<String, MediaId>,
    hyperlink_rels: &BTreeMap<String, String>,
    bookmarks: &mut DefinitionMap<BookmarkId, Bookmark>,
    container: &'static [u8],
    config: ImportConfig,
) -> Result<Vec<(String, NoteId, Vec<BlockNode>)>, ImportError> {
    // A note resolves its own part's media and hyperlink relationships; note
    // references inside a note (rare) carry no index.
    let empty_notes = BTreeMap::new();
    let empty_hf = BTreeMap::new();
    let empty_comment = BTreeMap::new();
    let empty_embedded = BTreeMap::new();
    let inputs = ParseInputs {
        styles,
        numbering,
        media_index,
        hyperlink_rels,
        embedded_index: &empty_embedded,
        footnote_ids: &empty_notes,
        endnote_ids: &empty_notes,
        header_ids: &empty_hf,
        footer_ids: &empty_hf,
        comment_ids: &empty_comment,
        color_scheme: None,
    };
    let mut parser = BodyParser::build(ids, reporter, &inputs, bookmarks, Some(container), config);
    parser.run(xml)?;
    while !parser.frames.is_empty() {
        parser.exit_frame()?;
    }
    // A note left open by malformed input still commits its content.
    parser.close_note()?;
    Ok(parser
        .notes
        .into_iter()
        .map(|(source_id, node_id, _meta, blocks)| (source_id, NoteId::new(node_id), blocks))
        .collect())
}

/// Parses a header/footer part (`word/header1.xml` / `word/footer1.xml`) into its
/// block content. `root` is `b"hdr"` or `b"ftr"`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn parse_header_footer(
    xml: &[u8],
    ids: &mut IdGenerator,
    reporter: &mut Reporter,
    styles: &Styles,
    numbering: &Numbering,
    media_index: &BTreeMap<String, MediaId>,
    hyperlink_rels: &BTreeMap<String, String>,
    bookmarks: &mut DefinitionMap<BookmarkId, Bookmark>,
    root: &'static [u8],
    config: ImportConfig,
) -> Result<Vec<BlockNode>, ImportError> {
    let empty_notes = BTreeMap::new();
    let empty_hf = BTreeMap::new();
    let empty_comment = BTreeMap::new();
    let empty_embedded = BTreeMap::new();
    let inputs = ParseInputs {
        styles,
        numbering,
        media_index,
        hyperlink_rels,
        embedded_index: &empty_embedded,
        footnote_ids: &empty_notes,
        endnote_ids: &empty_notes,
        header_ids: &empty_hf,
        footer_ids: &empty_hf,
        comment_ids: &empty_comment,
        color_scheme: None,
    };
    let mut parser = BodyParser::build(ids, reporter, &inputs, bookmarks, None, config);
    parser.hf_root = Some(root);
    parser.run(xml)?;
    while !parser.frames.is_empty() {
        parser.exit_frame()?;
    }
    if parser.paragraph_open {
        parser.finish_paragraph()?;
    }
    // Commit any table left open by truncated header/footer markup.
    let roots = parser.tables.flush_open(&mut *parser.ids)?;
    parser.blocks.extend(roots);
    Ok(parser.blocks)
}

/// Parses the comments part (`word/comments.xml`) into its comments, each keyed
/// by its source `w:id` with its allocated id, metadata, and block content. The
/// part resolves its own media and hyperlink relationships.
#[allow(clippy::too_many_arguments)]
pub(crate) fn parse_comments(
    xml: &[u8],
    ids: &mut IdGenerator,
    reporter: &mut Reporter,
    styles: &Styles,
    numbering: &Numbering,
    media_index: &BTreeMap<String, MediaId>,
    hyperlink_rels: &BTreeMap<String, String>,
    bookmarks: &mut DefinitionMap<BookmarkId, Bookmark>,
    config: ImportConfig,
) -> Result<Vec<(String, CommentId, Comment)>, ImportError> {
    let empty_notes = BTreeMap::new();
    let empty_hf = BTreeMap::new();
    let empty_comment = BTreeMap::new();
    let empty_embedded = BTreeMap::new();
    let inputs = ParseInputs {
        styles,
        numbering,
        media_index,
        hyperlink_rels,
        embedded_index: &empty_embedded,
        footnote_ids: &empty_notes,
        endnote_ids: &empty_notes,
        header_ids: &empty_hf,
        footer_ids: &empty_hf,
        comment_ids: &empty_comment,
        color_scheme: None,
    };
    let mut parser = BodyParser::build(ids, reporter, &inputs, bookmarks, Some(b"comment"), config);
    parser.run(xml)?;
    while !parser.frames.is_empty() {
        parser.exit_frame()?;
    }
    parser.close_note()?;
    Ok(parser
        .notes
        .into_iter()
        .map(|(source_id, node_id, meta, blocks)| {
            (
                source_id,
                CommentId::new(node_id),
                Comment {
                    blocks,
                    author: meta.author,
                    date: meta.date,
                    initials: meta.initials,
                    // Threading, durable id, and identity are joined from the
                    // companion parts in `build_comments`.
                    ..Comment::default()
                },
            )
        })
        .collect())
}

impl BodyParser<'_> {
    fn next_id(&mut self) -> Result<NodeId, ImportError> {
        self.ids
            .next_id()
            .map_err(|_| ImportError::LimitExceeded { limit: "node_ids" })
    }

    fn run(&mut self, xml: &[u8]) -> Result<(), ImportError> {
        let mut reader = Reader::from_reader(xml);
        let mut buffer = Vec::new();
        loop {
            let event = reader
                .read_event_into(&mut buffer)
                .map_err(|_| ImportError::MalformedXml)?;
            // VML raw-XML capture: mirror every event inside a `w:pict` subtree so
            // the closed pict can be re-parsed by `parse_vml_pict` for its positioned
            // shapes. Teeing runs BEFORE dispatch (and before the math guard) so the
            // captured fragment is faithful; normal dispatch is unaffected, so the
            // inline `v:imagedata` path and the `w:txbxContent` block flow still run.
            self.capture_pict_event(&event);
            // While capturing an OMML subtree, every event is buffered verbatim
            // and NOT dispatched to the `w:`-namespace element handlers — this is
            // the C1 namespace guard, so a math run's `m:r`/`m:t` can never be
            // mistaken for a `w:r`/`w:t` and flatten into the paragraph text.
            if self.math_depth > 0 {
                self.capture_math_event(event)?;
                buffer.clear();
                continue;
            }
            match event {
                Event::Eof => break,
                Event::DocType(_) => return Err(ImportError::MalformedXml),
                Event::Start(element) => {
                    self.depth += 1;
                    if self.depth > self.config.max_depth {
                        return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                    }
                    if is_math_root(element.local_name().as_ref()) && self.math_allowed() {
                        self.begin_math(&element)?;
                    } else {
                        self.on_start(element.local_name().as_ref(), &element)?;
                    }
                }
                Event::Empty(element) => {
                    if is_math_root(element.local_name().as_ref()) && self.math_allowed() {
                        // A degenerate self-closing math root: retain the lone tag.
                        self.emit_empty_math(&element)?;
                    } else {
                        self.on_start(element.local_name().as_ref(), &element)?;
                        self.on_end(element.local_name().as_ref())?;
                    }
                }
                Event::End(element) => {
                    self.on_end(element.local_name().as_ref())?;
                    self.depth = self.depth.saturating_sub(1);
                }
                // A `wp:posOffset`/`wp:align` value: its text is captured into the
                // open anchor accumulator (bounded), not the paragraph flow.
                Event::Text(text) if self.capturing_anchor_axis() => {
                    let raw = text.into_inner();
                    let raw =
                        std::str::from_utf8(raw.as_ref()).map_err(|_| ImportError::MalformedXml)?;
                    if let Some(anchor) = self.pending_anchor.as_mut()
                        && anchor.capture_buffer.len() + raw.len() <= 64
                    {
                        anchor.capture_buffer.push_str(raw);
                    }
                }
                Event::Text(text) if self.in_text || self.in_instr => {
                    let raw = text.into_inner();
                    let raw =
                        std::str::from_utf8(raw.as_ref()).map_err(|_| ImportError::MalformedXml)?;
                    let decoded =
                        quick_xml::escape::unescape(raw).map_err(|_| ImportError::MalformedXml)?;
                    self.push_text(decoded.as_ref())?;
                }
                Event::CData(cdata) if self.in_text || self.in_instr => {
                    let raw = cdata.into_inner();
                    let text =
                        std::str::from_utf8(raw.as_ref()).map_err(|_| ImportError::MalformedXml)?;
                    self.push_text(text)?;
                }
                _ => {}
            }
            buffer.clear();
        }
        Ok(())
    }

    /// Mirrors one XML event into the open `w:pict` raw-XML capture (see
    /// [`Self::vml_capture`]). Opens a fresh capture on the outermost `<w:pict>`
    /// start, appends every event within it, and, on the matching end, hands the
    /// re-serialized fragment to [`Self::pending_pict_xml`] for [`Self::commit_pict`].
    fn capture_pict_event(&mut self, event: &Event<'_>) {
        let pict_start = matches!(event, Event::Start(e) if e.local_name().as_ref() == b"pict");
        let pict_end = matches!(event, Event::End(e) if e.local_name().as_ref() == b"pict");
        if pict_start && self.vml_capture.is_none() {
            self.vml_capture = Some(Writer::new(Vec::new()));
            self.vml_capture_depth = 0;
        }
        if self.vml_capture.is_none() {
            return;
        }
        if let Some(writer) = self.vml_capture.as_mut() {
            // Best-effort: a write failure just yields a shorter fragment, which
            // `parse_vml_pict` handles by returning the shapes parsed so far.
            let _ = writer.write_event(event.borrow());
        }
        if pict_start {
            self.vml_capture_depth += 1;
        } else if pict_end {
            self.vml_capture_depth = self.vml_capture_depth.saturating_sub(1);
            if self.vml_capture_depth == 0
                && let Some(writer) = self.vml_capture.take()
            {
                self.pending_pict_xml = String::from_utf8(writer.into_inner()).ok();
            }
        }
    }

    /// Whether an OMML `m:oMath`/`m:oMathPara` may begin capture here: only in
    /// genuine inline flow (an open paragraph, not inside a property container,
    /// a skipped alternate-content branch, or a property-change subtree). Outside
    /// those, the element falls through to the catch-all reporter unchanged.
    fn math_allowed(&self) -> bool {
        self.paragraph_open
            && self.mc_skip_depth == 0
            && self.pr_change_depth == 0
            && self.sdt_prop_depth == 0
            && self.ppr_depth == 0
            && self.rpr_depth == 0
    }

    /// Begins capturing an OMML subtree: opens a fresh re-serializer and writes
    /// the root's start tag. `self.depth` was already incremented for this Start.
    fn begin_math(&mut self, element: &BytesStart<'_>) -> Result<(), ImportError> {
        let mut writer = Writer::new(Vec::new());
        writer
            .write_event(Event::Start(element.borrow()))
            .map_err(|_| ImportError::MalformedXml)?;
        self.math_writer = Some(writer);
        self.math_text.clear();
        self.math_in_t = false;
        self.math_depth = 1;
        Ok(())
    }

    /// Retains a self-closing math root (`<m:oMath/>`) as an empty-text math node.
    fn emit_empty_math(&mut self, element: &BytesStart<'_>) -> Result<(), ImportError> {
        let mut writer = Writer::new(Vec::new());
        writer
            .write_event(Event::Empty(element.borrow()))
            .map_err(|_| ImportError::MalformedXml)?;
        let omml = String::from_utf8(writer.into_inner()).map_err(|_| ImportError::MalformedXml)?;
        self.push_segment(Segment::Math {
            omml,
            text: String::new(),
        });
        Ok(())
    }

    /// Buffers one event of an in-progress OMML capture verbatim, tracking the
    /// nesting depth and the plain-text fallback. On the root's matching close it
    /// finalizes the retained [`Segment::Math`].
    fn capture_math_event(&mut self, event: Event<'_>) -> Result<(), ImportError> {
        match &event {
            Event::Start(el) => {
                self.depth += 1;
                if self.depth > self.config.max_depth {
                    return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                }
                self.math_depth += 1;
                // An `m:t` (any namespace prefix; local name `t`) carries literal
                // equation text collected into the fallback.
                self.math_in_t = el.local_name().as_ref() == b"t";
            }
            Event::End(_) => {
                self.depth = self.depth.saturating_sub(1);
                self.math_depth -= 1;
                self.math_in_t = false;
            }
            Event::Eof => return Err(ImportError::MalformedXml),
            Event::Text(text) if self.math_in_t => {
                let raw = text.decode().map_err(|_| ImportError::MalformedXml)?;
                let decoded =
                    quick_xml::escape::unescape(&raw).map_err(|_| ImportError::MalformedXml)?;
                self.push_math_text(&decoded)?;
            }
            _ => {}
        }
        let writer = self
            .math_writer
            .as_mut()
            .expect("math writer present while capturing");
        writer
            .write_event(event)
            .map_err(|_| ImportError::MalformedXml)?;
        if writer.get_ref().len() > MAX_MATH_BYTES {
            return Err(ImportError::LimitExceeded {
                limit: "math_bytes",
            });
        }
        if self.math_depth == 0 {
            self.finish_math()?;
        }
        Ok(())
    }

    /// Appends fallback text, counting it against the aggregate text budget.
    fn push_math_text(&mut self, text: &str) -> Result<(), ImportError> {
        self.text_bytes = self.text_bytes.saturating_add(text.len());
        if self.text_bytes > self.config.max_text_bytes {
            return Err(ImportError::LimitExceeded {
                limit: "text_bytes",
            });
        }
        self.math_text.push_str(text);
        Ok(())
    }

    /// Finalizes a completed OMML capture into a retained [`Segment::Math`].
    fn finish_math(&mut self) -> Result<(), ImportError> {
        let writer = self.math_writer.take().expect("math writer present");
        let omml = String::from_utf8(writer.into_inner()).map_err(|_| ImportError::MalformedXml)?;
        let text = std::mem::take(&mut self.math_text);
        self.math_in_t = false;
        self.push_segment(Segment::Math { omml, text });
        Ok(())
    }

    fn push_text(&mut self, text: &str) -> Result<(), ImportError> {
        self.text_bytes = self.text_bytes.saturating_add(text.len());
        if self.text_bytes > self.config.max_text_bytes {
            return Err(ImportError::LimitExceeded {
                limit: "text_bytes",
            });
        }
        // Instruction text (inside `w:instrText`) builds the field code, not a
        // display run; it counts against the same aggregate text bound.
        if self.in_instr {
            self.instr_buffer.push_str(text);
        } else {
            self.text_buffer.push_str(text);
        }
        Ok(())
    }

    fn on_start(&mut self, local: &[u8], element: &BytesStart<'_>) -> Result<(), ImportError> {
        // While skipping a non-selected AlternateContent branch, ignore every
        // element (counting depth so the matching close ends the skip).
        if self.mc_skip_depth > 0 {
            self.mc_skip_depth += 1;
            return Ok(());
        }
        self.elements += 1;
        if self.elements > self.config.max_elements {
            return Err(ImportError::LimitExceeded {
                limit: "xml_elements",
            });
        }
        match local {
            // Property-change tracked revisions carry a nested copy of the PREVIOUS
            // property container (e.g. `w:rPrChange > w:rPr`). We snapshot the
            // just-completed CURRENT properties aside and reset the live
            // accumulator so the nested prior `w:*Pr` accumulates the historical
            // values through the exact same element routing; the snapshot is
            // restored — with its `prop_change` attached — on the matching close.
            b"rPrChange" if self.in_document && (self.rpr_depth > 0 || self.mark_rpr_depth > 0) => {
                self.begin_run_prop_change(element);
            }
            b"pPrChange" if self.in_document && self.ppr_depth > 0 => {
                let saved = std::mem::take(&mut self.paragraph_properties);
                self.prop_change.push(PropChangeCapture::Paragraph {
                    meta: PropChangeMeta::from_element(element),
                    saved,
                });
            }
            b"tblPrChange" if self.in_document && self.tblpr_depth > 0 => {
                if let Some(saved) = self.tables.take_table_properties() {
                    self.prop_change.push(PropChangeCapture::Table {
                        meta: PropChangeMeta::from_element(element),
                        saved,
                    });
                }
            }
            b"trPrChange" if self.in_document && self.trpr_depth > 0 => {
                if let Some(saved) = self.tables.take_row_properties() {
                    self.prop_change.push(PropChangeCapture::Row {
                        meta: PropChangeMeta::from_element(element),
                        saved,
                    });
                }
            }
            b"tcPrChange" if self.in_document && self.tcpr_depth > 0 => {
                if let Some(saved) = self.tables.take_cell_properties() {
                    self.prop_change.push(PropChangeCapture::Cell {
                        meta: PropChangeMeta::from_element(element),
                        saved,
                    });
                }
            }
            // `w:tblGridChange` wraps the prior `w:tblGrid`; its `w:gridCol`
            // children accumulate into the (reset) live grid. It carries only a
            // `w:id` (no author/date).
            b"tblGridChange"
                if self.in_document
                    && self.tables.is_active()
                    && self.suppressed_tbl_depth == 0 =>
            {
                if let Some(saved) = self.tables.take_grid() {
                    self.prop_change.push(PropChangeCapture::Grid {
                        meta: PropChangeMeta::from_element(element),
                        saved,
                    });
                }
            }
            // A section- or numbering-properties change is not yet modeled: report
            // the container and skip its subtree so the historical values are never
            // mapped over the current ones. The counter is incremented here (before
            // the skip catch) so nested changes still balance.
            b"sectPrChange" | b"numberingChange" if self.in_document => {
                self.pr_change_depth += 1;
                self.reporter.report(local);
            }
            // Inside a property-change revision: ignore every element (its
            // historical properties are reported via the container above).
            _ if self.pr_change_depth > 0 => {}
            b"document" => self.in_document = true,
            // `w:background` is a document-level sibling of `w:body` (it precedes
            // it). Capture its concrete sRGB `@w:color`; a theme/image background
            // is not modeled as sRGB, so report it (degraded) rather than lose it.
            b"background" if self.in_document && !self.in_body => {
                self.page_background = attribute_value(element, b"color")
                    .filter(|value| value != "auto")
                    .and_then(|value| parse_rgb(&value));
                let has_theme = attribute_value(element, b"themeColor").is_some();
                if self.page_background.is_none() || has_theme {
                    self.reporter.report(b"background");
                }
            }
            b"body"
                if self.in_document && self.note_container.is_none() && self.hf_root.is_none() =>
            {
                self.in_body = true;
            }
            // Notes-part mode: the `w:footnotes`/`w:endnotes` root enables
            // reporting, and each note element is a block container.
            b"comments" if self.note_container == Some(b"comment") => self.in_document = true,
            b"footnotes" if self.note_container == Some(b"footnote") => self.in_document = true,
            b"endnotes" if self.note_container == Some(b"endnote") => self.in_document = true,
            _ if self.note_container == Some(local) && self.in_document => {
                self.open_note(element)?;
            }
            // A header/footer part's root (`w:hdr`/`w:ftr`) is its single block
            // container: enable document reporting and block parsing.
            _ if self.hf_root == Some(local) => {
                self.in_document = true;
                self.in_body = true;
            }
            // Alternate content: select the first branch, skip (and report) the
            // rest so content is neither duplicated nor lost.
            b"AlternateContent" => self.alt_stack.push(false),
            b"Choice" | b"Fallback" if !self.alt_stack.is_empty() => {
                let selected = *self.alt_stack.last().expect("alt frame");
                if selected {
                    self.mc_skip_depth = 1;
                    self.reporter.report(local);
                } else {
                    *self.alt_stack.last_mut().expect("alt frame") = true;
                }
            }
            // A text box carries block content: parse it in a fresh, suspended
            // context so its inner paragraphs do not corrupt the enclosing one.
            b"txbxContent" => self.enter_textbox()?,
            b"p" if self.in_body
                && !self.run_open
                && self.ppr_depth == 0
                && self.rpr_depth == 0 =>
            {
                self.paragraph_open = true;
                self.paragraph_id = Some(self.next_id()?);
                self.paragraph_properties = ParagraphProperties::default();
                self.mark_rpr_seen = false;
                self.mark_run_properties = RunProperties::default();
                self.numpr_depth = 0;
                self.pending_num_id = None;
                self.pending_ilvl = 0;
                self.segments.clear();
            }
            b"pPr" if self.paragraph_open && !self.run_open => self.ppr_depth += 1,
            b"pStyle" if self.ppr_depth > 0 => {
                match self.resolve_style(element, StyleKind::Paragraph) {
                    Some(style) => self.paragraph_properties.style_ref = Some(style),
                    None => self.reporter.report(local),
                }
            }
            b"numPr" if self.ppr_depth > 0 => self.numpr_depth += 1,
            b"numId" if self.numpr_depth > 0 => {
                self.pending_num_id = attribute_value(element, b"val");
            }
            b"ilvl" if self.numpr_depth > 0 => {
                self.pending_ilvl = attribute_value(element, b"val")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
            }
            b"r" if self.paragraph_open => {
                self.run_open = true;
                self.run_properties = RunProperties::default();
            }
            // A paragraph-mark `w:rPr` (inside `w:pPr`, no run open): its children
            // are the pilcrow's run properties, tracked separately so a `w:shd`
            // there is not captured as paragraph shading.
            b"rPr" if self.ppr_depth > 0 && !self.run_open => {
                self.mark_rpr_depth += 1;
                self.mark_rpr_seen = true;
            }
            b"rPr" if self.run_open => self.rpr_depth += 1,
            b"rStyle" if self.rpr_depth > 0 => {
                match self.resolve_style(element, StyleKind::Character) {
                    Some(style) => self.run_properties.style_ref = Some(style),
                    None => self.reporter.report(local),
                }
            }
            b"t" if self.run_open => {
                self.in_text = true;
                self.text_buffer.clear();
            }
            // Deleted text inside a `w:del` uses `w:delText` instead of `w:t`; it
            // is captured into the run text buffer exactly like `w:t` so deleted
            // content is preserved (the enclosing revision marks it deleted).
            b"delText" if self.run_open => {
                self.in_text = true;
                self.text_buffer.clear();
            }
            // A ruby annotation's runs are the phonetic guide, not base text.
            // A counter (not a flag) so a nested `w:rt` cannot clear it early.
            b"rt" => self.ruby_annotation_depth += 1,
            b"instrText" if self.run_open => {
                self.in_instr = true;
                self.instr_buffer.clear();
            }
            // Footnote/endnote references (inside a run) resolve to a note id.
            b"footnoteReference" if self.run_open => {
                match attribute_value(element, b"id")
                    .and_then(|id| self.footnote_ids.get(&id).copied())
                {
                    Some(note) => self.push_segment(Segment::NoteReference {
                        kind: NoteKind::Footnote,
                        note,
                    }),
                    None => self.reporter.report(b"footnoteReference"),
                }
            }
            b"endnoteReference" if self.run_open => {
                match attribute_value(element, b"id")
                    .and_then(|id| self.endnote_ids.get(&id).copied())
                {
                    Some(note) => self.push_segment(Segment::NoteReference {
                        kind: NoteKind::Endnote,
                        note,
                    }),
                    None => self.reporter.report(b"endnoteReference"),
                }
            }
            // A comment reference (inside a run) resolves to a comment id.
            b"commentReference" if self.run_open => {
                match attribute_value(element, b"id")
                    .and_then(|id| self.comment_ids.get(&id).copied())
                {
                    Some(comment) => self.push_segment(Segment::CommentReference { comment }),
                    None => self.reporter.report(b"commentReference"),
                }
            }
            // A comment range marker (`w:commentRangeStart`/`End`, self-closing →
            // an Empty event, handled here; its `on_end` falls to `_ => {}`).
            // These bracket the commented span; they sit in paragraph flow (like
            // bookmark markers), not inside a run. Each resolves its `w:id` to a
            // modeled comment via the same index the reference uses — an
            // unresolved id (a dropped/oversized comment) is reported+dropped, so
            // the start/end/reference triad drops together and stays balanced.
            b"commentRangeStart" if self.paragraph_open && !self.run_open => {
                match attribute_value(element, b"id")
                    .and_then(|id| self.comment_ids.get(&id).copied())
                {
                    Some(comment) => self.push_segment(Segment::CommentRangeStart { comment }),
                    None => self.reporter.report(b"commentRangeStart"),
                }
            }
            b"commentRangeEnd" if self.paragraph_open && !self.run_open => {
                match attribute_value(element, b"id")
                    .and_then(|id| self.comment_ids.get(&id).copied())
                {
                    Some(comment) => self.push_segment(Segment::CommentRangeEnd { comment }),
                    None => self.reporter.report(b"commentRangeEnd"),
                }
            }
            // A bookmark start marker (`w:bookmarkStart`, self-closing → an Empty
            // event, so handled entirely here; its `on_end` falls to `_ => {}`).
            // Modeled only in paragraph flow, with a bounded non-empty name, and a
            // fresh source `w:id`. A missing/oversized name or a duplicate id is
            // reported+dropped (its `w:id` is NOT registered, so the matching end
            // becomes an orphan that is also reported → balanced, no dangling ref).
            b"bookmarkStart" if self.paragraph_open && !self.run_open => {
                match attribute_value(element, b"name")
                    .filter(|name| !name.is_empty() && name.len() <= 255)
                {
                    Some(name) => {
                        let source = attribute_value(element, b"id").unwrap_or_default();
                        if self.bookmark_ids.contains_key(&source) {
                            // Duplicate `w:id`: keep the first, report the second.
                            self.reporter.report(b"bookmarkStart");
                        } else {
                            let bookmark = BookmarkId::new(self.next_id()?);
                            self.bookmarks.insert(bookmark, Bookmark { name });
                            self.bookmark_ids.insert(source, bookmark);
                            self.push_segment(Segment::BookmarkStart { bookmark });
                            // A column bookmark (`w:colFirst`/`w:colLast`, a
                            // table-cell column range) is modeled by name/range,
                            // but its column span is dropped — report it so the
                            // dropped column attributes surface, never silently.
                            if attribute_value(element, b"colFirst").is_some()
                                || attribute_value(element, b"colLast").is_some()
                            {
                                self.reporter.report(b"bookmarkStart");
                            }
                        }
                    }
                    None => self.reporter.report(b"bookmarkStart"),
                }
            }
            // A bookmark end marker (`w:bookmarkEnd`). Modeled only when its source
            // `w:id` resolves to an open (registered) start; the id is then
            // DE-REGISTERED so exactly one end pairs with one start. A second end
            // reusing that id — or an end with no matching start, a block-level
            // start, or a dropped start — orphans and is reported. (Removing on
            // end also lets a producer legitimately reuse a `w:id` for a later,
            // non-overlapping range: its fresh start registers cleanly.)
            b"bookmarkEnd" if self.paragraph_open && !self.run_open => {
                match attribute_value(element, b"id")
                    .and_then(|source| self.bookmark_ids.remove(&source))
                {
                    Some(bookmark) => self.push_segment(Segment::BookmarkEnd { bookmark }),
                    None => self.reporter.report(b"bookmarkEnd"),
                }
            }
            // A tracked-move range start marker (`w:moveFromRangeStart` /
            // `w:moveToRangeStart`, self-closing → an Empty event, handled here;
            // its `on_end` falls to `_ => {}`). Modeled only in paragraph flow, with
            // a bounded pairing `w:id` and a bounded non-empty `w:name`. The `w:id`
            // is preserved verbatim (opaque, like a revision's grouping key) so the
            // matching end re-pairs on re-import — no id registration is needed. A
            // missing/oversized id or name is reported+dropped. `w:author`/`w:date`
            // mirror the wrapper metadata; `w:colFirst`/`colLast`/
            // `displacedByCustomXml` are ignored.
            b"moveFromRangeStart" | b"moveToRangeStart"
                if self.paragraph_open && !self.run_open =>
            {
                let kind = if local == b"moveFromRangeStart" {
                    MoveKind::From
                } else {
                    MoveKind::To
                };
                match (
                    attribute_value(element, b"id").filter(|id| !id.is_empty() && id.len() <= 64),
                    attribute_value(element, b"name")
                        .filter(|name| !name.is_empty() && name.len() <= 255),
                ) {
                    (Some(move_id), Some(name)) => {
                        self.push_segment(Segment::MoveRangeStart {
                            kind,
                            move_id,
                            name,
                            author: attribute_value(element, b"author")
                                .filter(|value| !value.is_empty() && value.len() <= 255),
                            date: attribute_value(element, b"date")
                                .filter(|value| !value.is_empty() && value.len() <= 64),
                        });
                    }
                    _ => self.reporter.report(local),
                }
            }
            // A tracked-move range end marker (`w:moveFromRangeEnd` /
            // `w:moveToRangeEnd`). Modeled with its bounded pairing `w:id`; an
            // orphan end (no matching start) still round-trips faithfully by its
            // verbatim id. A missing/oversized id is reported+dropped.
            b"moveFromRangeEnd" | b"moveToRangeEnd" if self.paragraph_open && !self.run_open => {
                let kind = if local == b"moveFromRangeEnd" {
                    MoveKind::From
                } else {
                    MoveKind::To
                };
                match attribute_value(element, b"id").filter(|id| !id.is_empty() && id.len() <= 64)
                {
                    Some(move_id) => self.push_segment(Segment::MoveRangeEnd { kind, move_id }),
                    None => self.reporter.report(local),
                }
            }
            // A complex field is delimited by field characters inside runs.
            b"fldChar" if self.run_open => {
                match attribute_value(element, b"fldCharType").as_deref() {
                    Some("begin") => self.begin_field(),
                    Some("separate") => self.separate_field(),
                    Some("end") => self.close_field(),
                    _ => {}
                }
            }
            // A simple field carries its instruction inline and wraps its result.
            b"fldSimple" if self.paragraph_open && !self.run_open => {
                self.open_simple_field(element);
            }
            // A legacy form field's `w:ffData` block sits inside a complex field's
            // `fldChar begin`; it opens a `FormFieldBuilder` on the open field.
            b"ffData" if self.field.is_some() && !self.in_ffdata => {
                self.in_ffdata = true;
                if let Some(field) = self.field.as_mut() {
                    field.form = Some(FormFieldBuilder::default());
                }
            }
            // Every child of `w:ffData` routes onto the builder (its own children —
            // `w:textInput`/`w:checkBox`/`w:ddList` payloads — are handled there).
            _ if self.in_ffdata => self.ffdata_child(local, element),
            b"tab" if self.run_open => self.push_segment(Segment::Tab),
            b"br" if self.run_open => {
                let kind = break_kind(element);
                self.push_segment(Segment::Break(kind));
            }
            // A symbol glyph: `w:sym@w:font` + `@w:char` (a hex code point, often
            // PUA `0xF0xx`). Modeled only when both a bounded, non-empty font name
            // and a parseable hex char are present; anything else is reported so
            // the glyph is not silently dropped without a trace.
            b"sym" if self.run_open => match symbol_glyph(element) {
                Some((font, char)) => self.push_segment(Segment::Symbol { font, char }),
                None => self.reporter.report(b"sym"),
            },
            // Inert typographic glyphs inside a run: a non-breaking hyphen (never a
            // line-break opportunity) and a soft/optional hyphen (a hyphenation
            // point drawn only when the line breaks there). Both are leaves, like a
            // tab or a symbol.
            b"noBreakHyphen" if self.run_open => self.push_segment(Segment::NoBreakHyphen),
            b"softHyphen" if self.run_open => self.push_segment(Segment::SoftHyphen),
            // An absolute-position tab (`w:ptab`): its stop is positioned relative
            // to the margin/indent with an alignment and optional leader. The three
            // attributes are required; a missing/unrecognized one falls back to the
            // schema defaults (left / margin / none) so the tab is never dropped.
            b"ptab" if self.run_open => {
                let alignment = match attribute_value(element, b"alignment").as_deref() {
                    Some("center") => PositionalTabAlignment::Center,
                    Some("right") => PositionalTabAlignment::Right,
                    _ => PositionalTabAlignment::Left,
                };
                let relative_to = match attribute_value(element, b"relativeTo").as_deref() {
                    Some("indent") => PositionalTabRelativeTo::Indent,
                    _ => PositionalTabRelativeTo::Margin,
                };
                let leader = match attribute_value(element, b"leader").as_deref() {
                    Some("dot") => PositionalTabLeader::Dot,
                    Some("hyphen") => PositionalTabLeader::Hyphen,
                    Some("underscore") => PositionalTabLeader::Underscore,
                    Some("middleDot") => PositionalTabLeader::MiddleDot,
                    _ => PositionalTabLeader::None,
                };
                self.push_segment(Segment::PositionalTab {
                    alignment,
                    relative_to,
                    leader,
                });
            }
            b"hyperlink" if self.paragraph_open && !self.run_open && self.field.is_none() => {
                self.hyperlink_depth += 1;
                if self.hyperlink_depth == 1 {
                    match self.resolve_hyperlink_target(element) {
                        Some((target, tooltip)) => {
                            self.hyperlink = Some(HyperlinkAccumulator {
                                target,
                                tooltip,
                                segments: Vec::new(),
                            });
                            self.wrapper_order.push(WrapperKind::Hyperlink);
                        }
                        None => self.reporter.report(b"hyperlink"),
                    }
                } else {
                    // A nested hyperlink is not modeled; its runs flatten into
                    // the outer link and the nesting is reported.
                    self.reporter.report(b"hyperlink");
                }
            }
            // A tracked change (`w:ins`/`w:del`) or a tracked-move run wrapper
            // (`w:moveFrom`/`w:moveTo`, which wrap runs exactly like `w:del`/`w:ins`
            // and share the same nesting rules). Modeled as a run range ONLY when
            // it sits directly in paragraph flow (not inside a run, not inside a
            // property context) and within the nesting bound; every other position
            // — a paragraph-mark revision (`w:pPr>w:rPr>w:ins`), a run-property
            // revision marker (`w:r>w:rPr>w:ins`), an over-`MAX_REVISION_DEPTH`
            // range, or a row/cell revision outside a paragraph — is reported and
            // counted (`suppressed_revision_depth`) so its matching close balances
            // and never commits an enclosing real revision. The arm is
            // UNCONDITIONAL (both branches handle every wrapper name) so start and
            // end stay balanced, exactly like tables' `suppressed_tbl_depth`.
            b"ins" | b"del" | b"moveFrom" | b"moveTo" => {
                if self.paragraph_open
                    && !self.run_open
                    && self.ppr_depth == 0
                    && self.rpr_depth == 0
                    && (self.revisions.len() as u32) < MAX_REVISION_DEPTH
                {
                    self.open_revision(local, element);
                } else {
                    self.reporter.report(local);
                    self.suppressed_revision_depth += 1;
                }
            }
            b"drawing" if self.run_open => {
                self.drawing_depth += 1;
                if self.drawing_depth == 1 {
                    self.pending_embed = None;
                    self.pending_extent = None;
                    self.drawing_extra = false;
                    self.blipfill_depth = 0;
                    self.pending_graphic = PendingGraphic::default();
                    self.pending_anchor = None;
                    self.group_stack.clear();
                    self.pending_shape = None;
                    self.pending_group = None;
                    self.xfrm_target = XfrmTarget::None;
                    self.ln_depth = 0;
                    self.style_ref_depth = 0;
                    self.pending_color = None;
                }
            }
            // The `a:graphicData@uri` distinguishes a chart / diagram / picture /
            // other payload; captured so `commit_drawing` can route it. Only the
            // first (outermost) graphicData in the drawing is recorded.
            b"graphicData" if self.drawing_depth > 0 && self.pending_graphic.uri.is_none() => {
                self.pending_graphic.uri = attribute_value(element, b"uri");
            }
            // A DrawingML chart payload: `c:chart@r:id` points at the chart part.
            b"chart" if self.drawing_depth > 0 && self.pending_graphic.chart_rid.is_none() => {
                self.pending_graphic.chart_rid = attribute_value(element, b"id");
            }
            // A SmartArt diagram payload: `dgm:relIds` names the data/layout/
            // quick-style/colors parts through four relationship ids.
            b"relIds" if self.drawing_depth > 0 => {
                self.pending_graphic.diagram_dm = attribute_value(element, b"dm");
                self.pending_graphic.diagram_lo = attribute_value(element, b"lo");
                self.pending_graphic.diagram_qs = attribute_value(element, b"qs");
                self.pending_graphic.diagram_cs = attribute_value(element, b"cs");
            }
            b"extent" if self.drawing_depth > 0 => {
                if let (Some(cx), Some(cy)) = (attr_i64(element, b"cx"), attr_i64(element, b"cy"))
                    && (0..=MAX_EMU).contains(&cx)
                    && (0..=MAX_EMU).contains(&cy)
                {
                    self.pending_extent = Some(Extent {
                        width_emu: cx,
                        height_emu: cy,
                    });
                }
            }
            b"blipFill" if self.drawing_depth > 0 => self.blipfill_depth += 1,
            b"blip" if self.blipfill_depth > 0 && self.pending_embed.is_none() => {
                self.pending_embed = attribute_value(element, b"embed");
            }
            // A floating anchor (`wp:anchor`): open an anchor accumulator so the
            // drawing's position/wrap/z-order are captured and it commits as an
            // `AnchoredDrawing` rather than collapsing to inline. `@behindDoc`
            // ("1"/"true") sets the behind-text z-order.
            b"anchor" if self.drawing_depth > 0 => {
                // `@behindDoc` defaults to false when absent, so it is an explicit
                // truthy check (not `is_true`, whose absent-is-true toggle semantics
                // suit `<w:b/>`-style flags, not a defaulted attribute).
                let behind_doc = matches!(
                    attribute_value(element, b"behindDoc").as_deref(),
                    Some("1") | Some("true")
                );
                // `@relativeHeight` is the monotonic z-order key (higher paints on
                // top) within the behind/front band.
                let relative_height = attribute_value(element, b"relativeHeight")
                    .and_then(|value| value.parse::<u32>().ok());
                self.pending_anchor = Some(PendingAnchor {
                    behind_doc,
                    relative_height,
                    ..PendingAnchor::default()
                });
            }
            // `wp:positionH`/`wp:positionV`: record which reference the following
            // `wp:posOffset`/`wp:align` is measured from, and mark the axis so its
            // value text is captured.
            b"positionH" if self.pending_anchor.is_some() => {
                let relative = attribute_value(element, b"relativeFrom");
                if let Some(anchor) = self.pending_anchor.as_mut() {
                    anchor.h_relative = horizontal_anchor(relative.as_deref());
                    anchor.capture_axis = Some(AnchorAxis::Horizontal);
                }
            }
            b"positionV" if self.pending_anchor.is_some() => {
                let relative = attribute_value(element, b"relativeFrom");
                if let Some(anchor) = self.pending_anchor.as_mut() {
                    anchor.v_relative = vertical_anchor(relative.as_deref());
                    anchor.capture_axis = Some(AnchorAxis::Vertical);
                }
            }
            // `wp:posOffset` / `wp:align` carry their value as element text; begin
            // capturing it for the current axis.
            b"posOffset" if self.capturing_anchor_axis() => {
                if let Some(anchor) = self.pending_anchor.as_mut() {
                    anchor.capture_is_offset = true;
                    anchor.capture_buffer.clear();
                }
            }
            b"align" if self.capturing_anchor_axis() => {
                if let Some(anchor) = self.pending_anchor.as_mut() {
                    anchor.capture_is_offset = false;
                    anchor.capture_buffer.clear();
                }
            }
            // The wrap mode (`wp:wrap*`, an empty element).
            b"wrapSquare" | b"wrapTight" | b"wrapThrough" | b"wrapTopAndBottom" | b"wrapNone"
                if self.pending_anchor.is_some() =>
            {
                if let Some(anchor) = self.pending_anchor.as_mut() {
                    anchor.wrap = Some(wrap_mode(local));
                }
            }
            // `wp:docPr@descr` is the drawing's alt text: modeled on an anchor
            // (accessibility); on an inline drawing it remains reported (the inline
            // path does not yet carry alt text).
            b"docPr" if self.drawing_depth > 0 => match attribute_value(element, b"descr") {
                Some(descr) if !descr.is_empty() && descr.len() <= MAX_DESCR_BYTES => {
                    if let Some(anchor) = self.pending_anchor.as_mut() {
                        anchor.descr = Some(descr);
                    } else {
                        self.drawing_extra = true;
                    }
                }
                Some(_) => self.drawing_extra = true,
                None => {}
            },
            b"hlinkClick" | b"svgBlip" if self.drawing_depth > 0 => self.drawing_extra = true,
            // A DrawingML group (`wpg:wgp` = the anchored object, `wpg:grpSp` =
            // nested): open a group builder. The top-level group inherits the open
            // anchor + `wp:extent` + `relativeHeight`; a nested group is positioned
            // purely by its own transform.
            b"wgp" | b"grpSp" if self.drawing_depth > 0 => {
                let id = self.next_id()?;
                let anchor = if self.group_stack.is_empty() {
                    self.pending_anchor.as_ref().map(|pending| {
                        (
                            pending.resolve(),
                            self.pending_extent.unwrap_or(ZERO_EXTENT),
                            pending.relative_height,
                        )
                    })
                } else {
                    None
                };
                self.group_stack.push(GroupBuilder {
                    id,
                    anchor,
                    transform: GroupTransform {
                        offset: PointEmu { x_emu: 0, y_emu: 0 },
                        extent: ZERO_EXTENT,
                        child_offset: PointEmu { x_emu: 0, y_emu: 0 },
                        child_extent: ZERO_EXTENT,
                    },
                    children: Vec::new(),
                });
            }
            // The group transform container: its `a:xfrm` off/ext/chOff/chExt route
            // to the top group builder.
            b"grpSpPr" if !self.group_stack.is_empty() => {
                self.xfrm_target = XfrmTarget::Group;
            }
            // A shape (`wps:wsp` / `wps:cxnSp`): open a shape builder. Its geometry/
            // fill/stroke come from `spPr`; a `w:txbxContent` makes it a text box.
            b"wsp" | b"cxnSp" if self.drawing_depth > 0 => {
                let id = self.next_id()?;
                self.pending_shape = Some(ShapeBuilder {
                    id,
                    is_picture: false,
                    offset: PointEmu { x_emu: 0, y_emu: 0 },
                    extent: ZERO_EXTENT,
                    geometry: ShapeGeometry::Rectangle,
                    fill: None,
                    stroke: None,
                    embed: None,
                    descr: None,
                    textbox_blocks: None,
                });
            }
            // A picture INSIDE a group (`pic:pic`): open a picture shape builder so
            // it is sized by its own `a:ext`. A lone/inline picture (no group) keeps
            // the existing `pending_embed`/`pending_extent` path.
            b"pic" if self.drawing_depth > 0 && !self.group_stack.is_empty() => {
                let id = self.next_id()?;
                self.pending_shape = Some(ShapeBuilder {
                    id,
                    is_picture: true,
                    offset: PointEmu { x_emu: 0, y_emu: 0 },
                    extent: ZERO_EXTENT,
                    geometry: ShapeGeometry::Rectangle,
                    fill: None,
                    stroke: None,
                    embed: None,
                    descr: None,
                    textbox_blocks: None,
                });
            }
            // A shape/picture transform container (`wps:spPr` / `pic:spPr`): its
            // `a:xfrm` off/ext route to the open shape builder.
            b"spPr" if self.drawing_depth > 0 => {
                self.xfrm_target = XfrmTarget::Shape;
            }
            // `a:off`/`a:ext`/`a:chOff`/`a:chExt` inside an `a:xfrm`: route to the
            // group transform or the shape geometry per `xfrm_target`. (The
            // extension-list `<a:ext uri=…>` carries no cx/cy and is skipped.)
            b"off" if self.drawing_depth > 0 => {
                if let (Some(x), Some(y)) = (attr_i64(element, b"x"), attr_i64(element, b"y")) {
                    self.set_xfrm_offset(PointEmu { x_emu: x, y_emu: y });
                }
            }
            b"ext" if self.drawing_depth > 0 && self.xfrm_target != XfrmTarget::None => {
                if let (Some(cx), Some(cy)) = (attr_i64(element, b"cx"), attr_i64(element, b"cy"))
                    && (0..=MAX_EMU).contains(&cx)
                    && (0..=MAX_EMU).contains(&cy)
                {
                    self.set_xfrm_extent(Extent {
                        width_emu: cx,
                        height_emu: cy,
                    });
                }
            }
            b"chOff" if self.drawing_depth > 0 && !self.group_stack.is_empty() => {
                if let (Some(x), Some(y)) = (attr_i64(element, b"x"), attr_i64(element, b"y"))
                    && let Some(group) = self.group_stack.last_mut()
                {
                    group.transform.child_offset = PointEmu { x_emu: x, y_emu: y };
                }
            }
            b"chExt" if self.drawing_depth > 0 && !self.group_stack.is_empty() => {
                if let (Some(cx), Some(cy)) = (attr_i64(element, b"cx"), attr_i64(element, b"cy"))
                    && (0..=MAX_EMU).contains(&cx)
                    && (0..=MAX_EMU).contains(&cy)
                    && let Some(group) = self.group_stack.last_mut()
                {
                    group.transform.child_extent = Extent {
                        width_emu: cx,
                        height_emu: cy,
                    };
                }
            }
            // The preset geometry (`a:prstGeom@prst`) of the open shape.
            b"prstGeom" if self.pending_shape.is_some() => {
                if let Some(shape) = self.pending_shape.as_mut() {
                    shape.geometry = match attribute_value(element, b"prst").as_deref() {
                        Some("rect") => ShapeGeometry::Rectangle,
                        Some("roundRect") => ShapeGeometry::RoundRectangle,
                        Some("ellipse") => ShapeGeometry::Ellipse,
                        Some("line" | "straightConnector1") => ShapeGeometry::Line,
                        _ => ShapeGeometry::Other,
                    };
                }
            }
            // An outline (`a:ln`): its `@w` is the stroke width; a `solidFill` inside
            // it colors the stroke rather than the fill.
            b"ln" if self.pending_shape.is_some() => {
                self.ln_depth += 1;
                self.ln_width_emu = attr_i64(element, b"w").filter(|w| *w >= 0).unwrap_or(0);
            }
            // An explicit `a:noFill`: clears the shape fill or (inside `a:ln`) the
            // stroke, so a later default is not assumed.
            b"noFill" if self.pending_shape.is_some() => {
                if let Some(shape) = self.pending_shape.as_mut() {
                    if self.ln_depth > 0 {
                        shape.stroke = None;
                    } else {
                        shape.fill = None;
                    }
                }
            }
            // A DrawingML color (`a:srgbClr`/`a:schemeClr`/`a:sysClr`): open a color
            // accumulator (base + modifiers). Only captured for a shape builder.
            // A shape-style reference container: a `schemeClr` inside it names a
            // theme style index, not the shape's own fill/stroke — suppress capture.
            b"lnRef" | b"fillRef" | b"effectRef" | b"fontRef" if self.pending_shape.is_some() => {
                self.style_ref_depth += 1;
            }
            b"srgbClr" | b"schemeClr" | b"sysClr"
                if self.pending_shape.is_some()
                    && self.pending_color.is_none()
                    && self.style_ref_depth == 0 =>
            {
                let base = match local {
                    b"srgbClr" => attribute_value(element, b"val")
                        .and_then(|hex| parse_rgb(&hex))
                        .map(|rgb| [rgb.r, rgb.g, rgb.b, 255]),
                    b"schemeClr" => attribute_value(element, b"val")
                        .as_deref()
                        .and_then(scheme_slot_index)
                        .map(|index| self.palette[index]),
                    _ => Some([0, 0, 0, 255]),
                };
                if let Some(base) = base {
                    let dest = if self.ln_depth > 0 {
                        ColorDest::Stroke
                    } else {
                        ColorDest::Fill
                    };
                    self.pending_color = Some(PendingColor {
                        dest,
                        base,
                        lum_mod: None,
                        lum_off: None,
                        tint: None,
                        shade: None,
                        alpha: None,
                        stroke_width_emu: self.ln_width_emu,
                    });
                }
            }
            // Color transform modifiers, applied when the color element closes.
            b"lumMod" | b"lumOff" | b"tint" | b"shade" | b"alpha"
                if self.pending_color.is_some() =>
            {
                if let Some(color) = self.pending_color.as_mut()
                    && let Some(factor) = attribute_value(element, b"val")
                        .as_deref()
                        .and_then(parse_percent)
                {
                    match local {
                        b"lumMod" => color.lum_mod = Some(factor),
                        b"lumOff" => color.lum_off = Some(factor),
                        b"tint" => color.tint = Some(factor),
                        b"shade" => color.shade = Some(factor),
                        _ => color.alpha = Some(factor),
                    }
                }
            }
            // A shape's non-visual props (`pic:cNvPr`/`wps:cNvPr`): capture its
            // alt text (`@descr`) for the open shape.
            b"cNvPr" if self.pending_shape.is_some() => {
                if let Some(shape) = self.pending_shape.as_mut()
                    && let Some(descr) = attribute_value(element, b"descr")
                    && !descr.is_empty()
                    && descr.len() <= MAX_DESCR_BYTES
                {
                    shape.descr = Some(descr);
                }
            }
            // A legacy VML picture (`w:pict`) carries its image as
            // `v:imagedata@r:id`; resolve it through the same media table.
            b"pict" if self.run_open => {
                self.pict_depth += 1;
                if self.pict_depth == 1 {
                    self.pending_embed = None;
                }
            }
            b"imagedata" if self.pict_depth > 0 && self.pending_embed.is_none() => {
                self.pending_embed = attribute_value(element, b"id");
            }
            // An OLE object (`w:object`): its `o:OLEObject` names the embedding and
            // an optional `v:shape/v:imagedata` supplies the preview image. Origin
            // sizes (`w:dxaOrig`/`w:dyaOrig`, in twips) give the natural extent.
            b"object" if self.run_open => {
                self.object_depth += 1;
                if self.object_depth == 1 {
                    self.pending_object = PendingObject::default();
                    if let (Some(dxa), Some(dya)) =
                        (attr_i64(element, b"dxaOrig"), attr_i64(element, b"dyaOrig"))
                        && dxa >= 0
                        && dya >= 0
                    {
                        self.pending_object.extent = Some(Extent {
                            width_emu: dxa.saturating_mul(635).min(MAX_EMU),
                            height_emu: dya.saturating_mul(635).min(MAX_EMU),
                        });
                    }
                }
            }
            b"imagedata" if self.object_depth > 0 && self.pending_object.preview_rid.is_none() => {
                self.pending_object.preview_rid = attribute_value(element, b"id");
            }
            b"OLEObject" if self.object_depth > 0 => {
                if self.pending_object.object_rid.is_none() {
                    self.pending_object.object_rid = attribute_value(element, b"id");
                }
                if self.pending_object.prog_id.is_none() {
                    self.pending_object.prog_id = attribute_value(element, b"ProgID");
                }
            }
            // A `w:sectPr` nested in a paragraph's `w:pPr` marks the END of a
            // section: this paragraph is the last of that section. It is a real
            // document section (same body/frame/note/hf guards as the body-level
            // one), captured here and linked to the paragraph on `</w:sectPr>`.
            // The mark/run `rPr` depth guards keep a `sectPr`-like tag inside a
            // run's properties from being mistaken for a section.
            b"sectPr"
                if self.in_body
                    && self.frames.is_empty()
                    && self.note_container.is_none()
                    && self.hf_root.is_none()
                    && self.paragraph_open
                    && self.ppr_depth > 0
                    && self.mark_rpr_depth == 0
                    && self.rpr_depth == 0 =>
            {
                self.section = Some(SectionAccumulator::default());
            }
            // Only a true body-level `w:sectPr` is the final document section; a
            // `sectPr` inside a text box (frames non-empty), a notes part, or a
            // header/footer part is not, so it is reported instead of silently
            // building a phantom/discarded section (which would also burn an id).
            b"sectPr"
                if self.in_body
                    && self.frames.is_empty()
                    && self.note_container.is_none()
                    && self.hf_root.is_none()
                    && !self.paragraph_open
                    && self.ppr_depth == 0 =>
            {
                self.section = Some(SectionAccumulator::default());
            }
            b"pgSz" if self.section.is_some() => {
                // `w:orient` is a hint Word derives from (and keeps consistent
                // with) the width/height; an unknown token is reported, never kept.
                let orientation = match attribute_value(element, b"orient").as_deref() {
                    Some("portrait") => Some(PageOrientation::Portrait),
                    Some("landscape") => Some(PageOrientation::Landscape),
                    None => None,
                    _ => {
                        self.reporter.report(b"pgSz");
                        None
                    }
                };
                if let Some(section) = self.section.as_mut() {
                    section.page_width = attr_i32(element, b"w");
                    section.page_height = attr_i32(element, b"h");
                    section.orientation = orientation;
                }
            }
            b"pgMar" if self.section.is_some() => {
                if let Some(section) = self.section.as_mut() {
                    section.margin_top = attr_i32(element, b"top");
                    section.margin_bottom = attr_i32(element, b"bottom");
                    section.margin_start =
                        attr_i32(element, b"start").or_else(|| attr_i32(element, b"left"));
                    section.margin_end =
                        attr_i32(element, b"end").or_else(|| attr_i32(element, b"right"));
                    // Header/footer band distances from the page edges. Word nests
                    // the header/footer inside the top/bottom margins (see layout's
                    // `PageConfig::content_area`); absent means Word's 720-twip default.
                    section.margin_header = attr_i32(element, b"header");
                    section.margin_footer = attr_i32(element, b"footer");
                }
            }
            b"cols" if self.section.is_some() => {
                let space = attr_i32(element, b"space");
                let separator = attribute_value(element, b"sep")
                    .as_deref()
                    .map(|value| is_true(Some(value)));
                let equal_width = attribute_value(element, b"equalWidth")
                    .as_deref()
                    .map(|value| is_true(Some(value)));
                if let Some(section) = self.section.as_mut() {
                    section.columns =
                        attribute_value(element, b"num").and_then(|value| value.parse().ok());
                    section.column_space = space;
                    section.column_separator = separator;
                    section.column_equal_width = equal_width;
                    // A fresh `w:cols` resets any previously accumulated per-column
                    // geometry (defensive against a malformed doubled element).
                    section.column_defs.clear();
                }
            }
            b"col" if self.section.is_some() => {
                let width = attr_i32(element, b"w");
                let space = attr_i32(element, b"space");
                if let (Some(width), Some(section)) = (width, self.section.as_mut()) {
                    // Bound the count so a hostile `w:cols` cannot accumulate an
                    // unbounded per-column vector (Word caps columns at 45).
                    if section.column_defs.len() < 64 {
                        section.column_defs.push(ColumnDef {
                            width_twips: width.clamp(0, 31_680),
                            space_twips: space.map(|v| v.clamp(0, 31_680)),
                        });
                    }
                }
            }
            b"type" if self.section.is_some() => {
                let section_type = match attribute_value(element, b"val").as_deref() {
                    Some("nextPage") => Some(SectionType::NextPage),
                    Some("continuous") => Some(SectionType::Continuous),
                    Some("evenPage") => Some(SectionType::EvenPage),
                    Some("oddPage") => Some(SectionType::OddPage),
                    Some("nextColumn") => Some(SectionType::NextColumn),
                    _ => None,
                };
                match (section_type, self.section.as_mut()) {
                    (Some(value), Some(section)) => section.section_type = Some(value),
                    (None, _) => self.reporter.report(b"type"),
                    _ => {}
                }
            }
            b"titlePg" if self.section.is_some() => {
                let on = is_true(attribute_value(element, b"val").as_deref());
                if let Some(section) = self.section.as_mut() {
                    section.title_page = Some(on);
                }
            }
            b"vAlign" if self.section.is_some() => {
                let alignment = match attribute_value(element, b"val").as_deref() {
                    Some("top") => Some(PageVerticalAlignment::Top),
                    Some("center") => Some(PageVerticalAlignment::Center),
                    Some("both") => Some(PageVerticalAlignment::Both),
                    Some("bottom") => Some(PageVerticalAlignment::Bottom),
                    _ => None,
                };
                match (alignment, self.section.as_mut()) {
                    (Some(value), Some(section)) => section.vertical_alignment = Some(value),
                    (None, _) => self.reporter.report(b"vAlign"),
                    _ => {}
                }
            }
            b"pgNumType" if self.section.is_some() => {
                let format =
                    attribute_value(element, b"fmt").filter(|v| !v.is_empty() && v.len() <= 32);
                let start = attr_i32(element, b"start");
                if let Some(section) = self.section.as_mut() {
                    section.page_number_format = format;
                    section.page_number_start = start;
                }
            }
            b"docGrid" if self.section.is_some() => {
                let grid_type = match attribute_value(element, b"type").as_deref() {
                    Some("default") => Some(DocGridType::Default),
                    Some("lines") => Some(DocGridType::Lines),
                    Some("linesAndChars") => Some(DocGridType::LinesAndChars),
                    Some("snapToChars") => Some(DocGridType::SnapToChars),
                    _ => None,
                };
                let line_pitch = attr_i32(element, b"linePitch");
                let char_space = attr_i32(element, b"charSpace");
                if let Some(section) = self.section.as_mut() {
                    section.doc_grid_type = grid_type;
                    section.doc_grid_line_pitch = line_pitch;
                    section.doc_grid_char_space = char_space;
                }
            }
            // Printer paper-source bins (`w:paperSrc`).
            b"paperSrc" if self.section.is_some() => {
                let first = attr_i32(element, b"first");
                let other = attr_i32(element, b"other");
                if let Some(section) = self.section.as_mut() {
                    section.paper_first = first;
                    section.paper_other = other;
                }
            }
            // Section page borders (`w:pgBorders`): open an edge-capture scope so
            // its `w:top`/`w:left`/`w:bottom`/`w:right` children route to the
            // section, and record `display`/`offsetFrom`.
            b"pgBorders" if self.section.is_some() => {
                let display = match attribute_value(element, b"display").as_deref() {
                    Some("allPages") => Some(PageBorderDisplay::AllPages),
                    Some("firstPage") => Some(PageBorderDisplay::FirstPage),
                    Some("notFirstPage") => Some(PageBorderDisplay::NotFirstPage),
                    _ => None,
                };
                let offset_from = match attribute_value(element, b"offsetFrom").as_deref() {
                    Some("page") => Some(PageBorderOffset::Page),
                    Some("text") => Some(PageBorderOffset::Text),
                    _ => None,
                };
                self.edge_scope = EdgeScope::PageBorders;
                if let Some(section) = self.section.as_mut() {
                    section.page_border_display = display;
                    section.page_border_offset = offset_from;
                }
            }
            // Section line numbering (`w:lnNumType`).
            b"lnNumType" if self.section.is_some() => {
                let count_by = attr_i32(element, b"countBy");
                let start = attr_i32(element, b"start");
                let distance = attr_i32(element, b"distance");
                let restart = match attribute_value(element, b"restart").as_deref() {
                    Some("newPage") => Some(LineNumberRestart::NewPage),
                    Some("newSection") => Some(LineNumberRestart::NewSection),
                    Some("continuous") => Some(LineNumberRestart::Continuous),
                    _ => None,
                };
                if let Some(section) = self.section.as_mut() {
                    section.line_count_by = count_by;
                    section.line_start = start;
                    section.line_distance = distance;
                    section.line_restart = restart;
                }
            }
            // Per-section footnote/endnote property containers: open a note scope
            // so the shared `w:pos`/`w:numFmt`/`w:numStart`/`w:numRestart` children
            // route to the right side of the section accumulator.
            b"footnotePr" if self.section.is_some() => {
                self.section_note_scope = Some(SectionNoteScope::Footnote);
            }
            b"endnotePr" if self.section.is_some() => {
                self.section_note_scope = Some(SectionNoteScope::Endnote);
            }
            b"pos" if self.section_note_scope.is_some() => {
                let position = match attribute_value(element, b"val").as_deref() {
                    Some("pageBottom") => Some(NotePosition::PageBottom),
                    Some("beneathText") => Some(NotePosition::BeneathText),
                    Some("sectEnd") => Some(NotePosition::SectionEnd),
                    Some("docEnd") => Some(NotePosition::DocumentEnd),
                    _ => None,
                };
                match (position, self.section_note_props()) {
                    (Some(value), Some(props)) => props.position = Some(value),
                    (None, _) => self.reporter.report(b"pos"),
                    _ => {}
                }
            }
            b"numFmt" if self.section_note_scope.is_some() => {
                let format =
                    attribute_value(element, b"val").filter(|v| !v.is_empty() && v.len() <= 32);
                if let Some(props) = self.section_note_props() {
                    props.number_format = format;
                }
            }
            b"numStart" if self.section_note_scope.is_some() => {
                let start = attr_i32(element, b"val");
                if let Some(props) = self.section_note_props() {
                    props.number_start = start;
                }
            }
            b"numRestart" if self.section_note_scope.is_some() => {
                let restart = match attribute_value(element, b"val").as_deref() {
                    Some("continuous") => Some(NoteNumberRestart::Continuous),
                    Some("eachSect") => Some(NoteNumberRestart::EachSection),
                    Some("eachPage") => Some(NoteNumberRestart::EachPage),
                    _ => None,
                };
                match (restart, self.section_note_props()) {
                    (Some(value), Some(props)) => props.number_restart = Some(value),
                    (None, _) => self.reporter.report(b"numRestart"),
                    _ => {}
                }
            }
            // Section text-flow direction (`w:textDirection`), reusing the shared
            // `TextDirection` vocabulary; an unmodeled token is reported.
            b"textDirection" if self.section.is_some() => {
                let direction = match attribute_value(element, b"val").as_deref() {
                    Some("lrTb") => Some(TextDirection::LrTb),
                    Some("tbRl") => Some(TextDirection::TbRl),
                    Some("btLr") => Some(TextDirection::BtLr),
                    _ => None,
                };
                match (direction, self.section.as_mut()) {
                    (Some(value), Some(section)) => section.text_direction = Some(value),
                    (None, _) => self.reporter.report(b"textDirection"),
                    _ => {}
                }
            }
            // Right-to-left section layout (`w:bidi`).
            b"bidi" if self.section.is_some() => {
                let on = is_true(attribute_value(element, b"val").as_deref());
                if let Some(section) = self.section.as_mut() {
                    section.bidi = on;
                }
            }
            b"headerReference" if self.section.is_some() => {
                match attribute_value(element, b"id")
                    .and_then(|id| self.header_ids.get(&id).copied())
                {
                    Some(reference) => {
                        let kind = header_footer_kind(attribute_value(element, b"type").as_deref());
                        if let Some(section) = self.section.as_mut() {
                            section.headers.push(HeaderFooterRef { kind, reference });
                        }
                    }
                    None => self.reporter.report(b"headerReference"),
                }
            }
            b"footerReference" if self.section.is_some() => {
                match attribute_value(element, b"id")
                    .and_then(|id| self.footer_ids.get(&id).copied())
                {
                    Some(reference) => {
                        let kind = header_footer_kind(attribute_value(element, b"type").as_deref());
                        if let Some(section) = self.section.as_mut() {
                            section.footers.push(HeaderFooterRef { kind, reference });
                        }
                    }
                    None => self.reporter.report(b"footerReference"),
                }
            }
            // Tables are the one nested block construct: cell paragraphs (and
            // nested tables) route into the open cell instead of the flat body.
            //
            // Suppression is context-independent and always balanced: every
            // `<w:tbl>` either opens a real table or increments the counter, and
            // every `</w:tbl>` balances it (the end arms below), so a `</w:tbl>`
            // can never desynchronize the table stack or close a real table it
            // does not own — even for malformed input where a `<w:tbl>` appears
            // inside a run or paragraph.
            b"tbl" if self.suppressed_tbl_depth > 0 => {
                // Already inside a refused/suppressed subtree: count every nested
                // table so the matching `</w:tbl>` balances suppression.
                self.suppressed_tbl_depth += 1;
                self.reporter.report(b"tbl");
            }
            b"tbl"
                if self.in_body
                    && !self.run_open
                    && !self.paragraph_open
                    && self.ppr_depth == 0
                    && self.rpr_depth == 0 =>
            {
                if !self.tables.open_table(&mut *self.ids)? {
                    // Nesting past the model bound: suppress the whole subtree so
                    // its rows/cells cannot mutate the enclosing table. Paragraphs
                    // inside still flatten into the enclosing cell (no data loss).
                    self.suppressed_tbl_depth = 1;
                    self.reporter.report(b"tbl");
                }
            }
            // A `<w:tbl>` in a non-block context (malformed: inside a run or
            // paragraph properties) is suppressed so its `</w:tbl>` cannot close a
            // real table; any text inside still flattens into the enclosing cell.
            b"tbl" if self.in_body => {
                self.suppressed_tbl_depth = 1;
                self.reporter.report(b"tbl");
            }
            // An aggregated external content chunk (`w:altChunk`): a block-level
            // sibling of paragraphs/tables referencing an imported sub-document
            // part by `r:id`. Modeled as a first-class `BlockNode::AltChunk` when
            // the reference resolves (the part is un-orphaned in the side-table so
            // its relationship is emitted once, from the node); an unresolved or
            // out-of-context reference is reported and dropped.
            b"altChunk"
                if self.in_body
                    && !self.run_open
                    && !self.paragraph_open
                    && self.pending_alt_chunk.is_none() =>
            {
                match self.resolve_embedded_part_opt(&attribute_value(element, b"id")) {
                    Some(part) => {
                        self.embedded_part_names.insert(part.part_name.clone());
                        self.pending_alt_chunk = Some(PendingAltChunk {
                            part,
                            properties: AltChunkProperties::default(),
                        });
                    }
                    None => self.reporter.report(b"altChunk"),
                }
            }
            // `w:matchSrc` inside the open chunk's `w:altChunkPr`: whether the
            // chunk renders with its own formatting. Present-but-empty means true
            // (`CT_OnOff`).
            b"matchSrc" if self.pending_alt_chunk.is_some() => {
                if let Some(chunk) = self.pending_alt_chunk.as_mut() {
                    chunk.properties.match_source =
                        Some(is_true(attribute_value(element, b"val").as_deref()));
                }
            }
            // Table properties (`w:tblPr`), a direct child of `w:tbl` before any
            // row. Its property children are guarded by `tblpr_depth`.
            b"tblPr" if self.tables.is_active() && self.suppressed_tbl_depth == 0 => {
                self.tblpr_depth += 1;
            }
            b"tr"
                if self.tables.is_active()
                    && self.suppressed_tbl_depth == 0
                    && !self.paragraph_open
                    && !self.run_open =>
            {
                // A row starts: table-property context is closed.
                self.tblpr_depth = 0;
                self.tables.open_row(&mut *self.ids)?;
            }
            // Row properties (`w:trPr`), a child of `w:tr` before any cell.
            b"trPr" if self.tables.is_active() && self.suppressed_tbl_depth == 0 => {
                self.trpr_depth += 1;
            }
            b"tc"
                if self.tables.is_active()
                    && self.suppressed_tbl_depth == 0
                    && !self.paragraph_open
                    && !self.run_open =>
            {
                // A cell starts: table/row-property contexts are closed.
                self.tcpr_depth = 0;
                self.trpr_depth = 0;
                self.tables.open_cell(&mut *self.ids)?;
            }
            b"tblGrid" if self.tables.is_active() && self.suppressed_tbl_depth == 0 => {}
            b"gridCol" if self.tables.is_active() && self.suppressed_tbl_depth == 0 => {
                let width = attr_i32(element, b"w").map(|width| width.clamp(0, 31_680));
                self.tables.add_grid_column(width);
            }
            b"tcPr" if self.tables.is_active() && self.suppressed_tbl_depth == 0 => {
                self.tcpr_depth += 1
            }
            b"gridSpan" if self.tcpr_depth > 0 => {
                if let Some(span) =
                    attribute_value(element, b"val").and_then(|value| value.parse::<u32>().ok())
                    && (1..=16_384).contains(&span)
                {
                    self.tables.set_grid_span(span);
                }
            }
            b"vMerge" if self.tcpr_depth > 0 => {
                let restart = attribute_value(element, b"val")
                    .map(|value| value == "restart")
                    .unwrap_or(false);
                self.tables.set_vertical_merge(if restart {
                    VerticalMerge::Restart
                } else {
                    VerticalMerge::Continue
                });
            }
            b"tcW" if self.tcpr_depth > 0 => {
                // Only `dxa` (twips) widths are modeled; `pct`/`auto` are reported.
                let is_dxa = attribute_value(element, b"type")
                    .map(|kind| kind == "dxa")
                    .unwrap_or(true);
                match attr_i32(element, b"w") {
                    Some(width) if is_dxa => self.tables.set_cell_width(width.clamp(0, 31_680)),
                    _ => self.reporter.report(b"tcW"),
                }
            }
            // ---- table properties (`w:tblPr`) --------------------------------
            b"tblStyle" if self.tblpr_depth > 0 => {
                match self.resolve_style(element, StyleKind::Table) {
                    Some(style) => self.tables.set_table_style(style),
                    None => self.reporter.report(local),
                }
            }
            b"bidiVisual" if self.tblpr_depth > 0 => self
                .tables
                .set_table_bidi_visual(is_true(attribute_value(element, b"val").as_deref())),
            b"jc" if self.tblpr_depth > 0 => match table_alignment(element) {
                Some(alignment) => self.tables.set_table_alignment(alignment),
                None => self.reporter.report(b"jc"),
            },
            b"tblW" if self.tblpr_depth > 0 => {
                let is_dxa = attribute_value(element, b"type")
                    .map(|kind| kind == "dxa")
                    .unwrap_or(true);
                match attr_i32(element, b"w") {
                    Some(width) if is_dxa => self.tables.set_table_width(width.clamp(0, 31_680)),
                    _ => self.reporter.report(b"tblW"),
                }
            }
            b"tblLayout" if self.tblpr_depth > 0 => {
                match attribute_value(element, b"type").as_deref() {
                    Some("fixed") => self.tables.set_table_layout(TableLayout::Fixed),
                    Some("autofit") => self.tables.set_table_layout(TableLayout::Autofit),
                    _ => self.reporter.report(b"tblLayout"),
                }
            }
            b"tblLook" if self.tblpr_depth > 0 => self.apply_table_look(element),
            b"tblInd" if self.tblpr_depth > 0 => {
                let is_dxa = attribute_value(element, b"type")
                    .map(|kind| kind == "dxa")
                    .unwrap_or(true);
                match attr_i32(element, b"w") {
                    Some(v) if is_dxa => self.tables.set_table_indent(v.clamp(-31_680, 31_680)),
                    _ => self.reporter.report(b"tblInd"),
                }
            }
            b"tblCellSpacing" if self.tblpr_depth > 0 => {
                let is_dxa = attribute_value(element, b"type")
                    .map(|kind| kind == "dxa")
                    .unwrap_or(true);
                match attr_i32(element, b"w") {
                    Some(v) if is_dxa => self.tables.set_table_cell_spacing(v.clamp(0, 31_680)),
                    _ => self.reporter.report(b"tblCellSpacing"),
                }
            }
            b"tblOverlap" if self.tblpr_depth > 0 => {
                match attribute_value(element, b"val").as_deref() {
                    Some("never") => self.tables.set_table_overlap(TableOverlap::Never),
                    Some("overlap") => self.tables.set_table_overlap(TableOverlap::Overlap),
                    _ => self.reporter.report(b"tblOverlap"),
                }
            }
            b"tblpPr" if self.tblpr_depth > 0 => {
                self.tables
                    .set_table_float_position(table_float_position(element));
            }
            b"tblCaption" if self.tblpr_depth > 0 => match attribute_value(element, b"val") {
                Some(v) if !v.is_empty() && v.len() <= 255 => self.tables.set_table_caption(v),
                _ => self.reporter.report(b"tblCaption"),
            },
            b"tblDescription" if self.tblpr_depth > 0 => match attribute_value(element, b"val") {
                Some(v) if !v.is_empty() && v.len() <= 255 => self.tables.set_table_description(v),
                _ => self.reporter.report(b"tblDescription"),
            },
            // `ppr_depth == 0` so a paragraph-direct `w:shd` (a `w:p` opened while
            // a `w:tblPr`/`w:tcPr` was left unclosed by malformed markup) is not
            // misrouted to the table/cell — it wins at the paragraph arm below.
            b"shd" if self.tblpr_depth > 0 && self.ppr_depth == 0 => {
                let fill = self.shading_fill(element);
                self.tables.set_table_shading(fill);
            }
            // ---- row properties (`w:trPr`) -----------------------------------
            b"cnfStyle" if self.trpr_depth > 0 => {
                self.tables
                    .set_row_conditional_format(parse_cnf_style(element));
            }
            b"trHeight" if self.trpr_depth > 0 => {
                let value = attribute_value(element, b"val")
                    .and_then(|value| value.parse::<u32>().ok())
                    .map(|value| value.min(31_680));
                let rule = match attribute_value(element, b"hRule").as_deref() {
                    Some("atLeast") => Some(HeightRule::AtLeast),
                    Some("exact") => Some(HeightRule::Exact),
                    Some("auto") => Some(HeightRule::Auto),
                    _ => None,
                };
                self.tables.set_row_height(value, rule);
            }
            b"cantSplit" if self.trpr_depth > 0 => self
                .tables
                .set_row_cant_split(is_true(attribute_value(element, b"val").as_deref())),
            b"tblHeader" if self.trpr_depth > 0 => self
                .tables
                .set_row_header(is_true(attribute_value(element, b"val").as_deref())),
            b"jc" if self.trpr_depth > 0 => match table_alignment(element) {
                Some(alignment) => self.tables.set_row_alignment(alignment),
                None => self.reporter.report(b"jc"),
            },
            b"tblCellSpacing" if self.trpr_depth > 0 => {
                let is_dxa = attribute_value(element, b"type")
                    .map(|kind| kind == "dxa")
                    .unwrap_or(true);
                match attr_i32(element, b"w") {
                    Some(v) if is_dxa => self.tables.set_row_cell_spacing(v.clamp(0, 31_680)),
                    _ => self.reporter.report(b"tblCellSpacing"),
                }
            }
            // ---- cell property long tail (`w:tcPr`) --------------------------
            b"cnfStyle" if self.tcpr_depth > 0 => {
                self.tables
                    .set_cell_conditional_format(parse_cnf_style(element));
            }
            b"shd" if self.tcpr_depth > 0 && self.ppr_depth == 0 => {
                let fill = self.shading_fill(element);
                self.tables.set_cell_shading(fill);
            }
            // Border / margin containers open an edge-capture scope. The
            // table-vs-cell level is `tblpr_depth`/`tcpr_depth`; the scope
            // disambiguates the border/margin edge children that share names.
            b"tblBorders" if self.tblpr_depth > 0 => self.edge_scope = EdgeScope::Borders,
            b"tcBorders" if self.tcpr_depth > 0 => self.edge_scope = EdgeScope::Borders,
            b"tblCellMar" if self.tblpr_depth > 0 => self.edge_scope = EdgeScope::Margins,
            b"tcMar" if self.tcpr_depth > 0 => self.edge_scope = EdgeScope::Margins,
            // Paragraph borders (`w:pBdr`), a direct `w:pPr` child (not the mark's).
            b"pBdr" if self.ppr_depth > 0 && self.rpr_depth == 0 && self.mark_rpr_depth == 0 => {
                self.edge_scope = EdgeScope::ParagraphBorders;
            }
            b"top" | b"start" | b"left" | b"bottom" | b"end" | b"right" | b"insideH"
            | b"insideV" | b"between" | b"bar"
                if self.edge_scope != EdgeScope::None =>
            {
                self.apply_table_edge(local, element);
            }
            // Paragraph shading (`w:shd`), a direct `w:pPr` child — NOT the mark's
            // `w:rPr` shd (a run property, left reported), and not a cell/table shd.
            b"shd" if self.ppr_depth > 0 && self.rpr_depth == 0 && self.mark_rpr_depth == 0 => {
                self.paragraph_properties.shading.fill = self.shading_fill(element);
            }
            // Run border (`w:bdr`): a single edge directly on the run's `w:rPr`
            // (not a `pBdr`-style container). Reuses the shared edge builder.
            b"bdr" if self.rpr_depth > 0 => match self.build_border_edge(element) {
                Some(edge) => self.run_properties.border = Some(edge),
                None => self.reporter.report(b"bdr"),
            },
            // Run shading (`w:shd`): the same fill-only modeling as paragraph/cell.
            b"shd" if self.rpr_depth > 0 => {
                self.run_properties.shading.fill = self.shading_fill(element);
            }
            // A `w:tabs` container: its `w:tab` children are custom tab stops.
            b"tabs" if self.ppr_depth > 0 && self.mark_rpr_depth == 0 => self.in_tabs = true,
            b"tab" if self.in_tabs => self.apply_tab_stop(element),
            b"vAlign" if self.tcpr_depth > 0 => match attribute_value(element, b"val").as_deref() {
                Some("top") => self
                    .tables
                    .set_cell_vertical_alignment(CellVerticalAlignment::Top),
                Some("center") => self
                    .tables
                    .set_cell_vertical_alignment(CellVerticalAlignment::Center),
                Some("bottom") => self
                    .tables
                    .set_cell_vertical_alignment(CellVerticalAlignment::Bottom),
                _ => self.reporter.report(b"vAlign"),
            },
            b"noWrap" if self.tcpr_depth > 0 => self
                .tables
                .set_cell_no_wrap(is_true(attribute_value(element, b"val").as_deref())),
            b"textDirection" if self.tcpr_depth > 0 => {
                match attribute_value(element, b"val").as_deref() {
                    Some("lrTb") => self.tables.set_cell_text_direction(TextDirection::LrTb),
                    Some("tbRl") => self.tables.set_cell_text_direction(TextDirection::TbRl),
                    Some("btLr") => self.tables.set_cell_text_direction(TextDirection::BtLr),
                    _ => self.reporter.report(b"textDirection"),
                }
            }
            b"tcFitText" if self.tcpr_depth > 0 => self
                .tables
                .set_cell_fit_text(is_true(attribute_value(element, b"val").as_deref())),
            b"hideMark" if self.tcpr_depth > 0 => self
                .tables
                .set_cell_hide_mark(is_true(attribute_value(element, b"val").as_deref())),
            // ---- content controls (`w:sdt`) ----------------------------------
            // A content control wraps flow content. Its scope (inline around runs,
            // block around paragraphs/tables, or a deferred passthrough) is decided
            // from the surrounding parser state exactly as `p`/`r`/`tbl` are.
            b"sdt" => {
                let scope = self.decide_sdt_scope();
                self.sdt_scopes.push(scope);
                match scope {
                    SdtScope::Inline => {
                        self.sdt_depth += 1;
                        self.sdts.push(SdtAccumulator {
                            properties: SdtProperties::default(),
                            segments: Vec::new(),
                        });
                        self.wrapper_order.push(WrapperKind::Sdt);
                    }
                    SdtScope::Block => {
                        self.sdt_depth += 1;
                        self.pending_block_sdt_props.push(SdtProperties::default());
                    }
                    // Reported once; its inner rows/cells/paragraphs flow to the
                    // current container unchanged (no data loss).
                    SdtScope::Passthrough => self.reporter.report(b"sdt"),
                }
            }
            // A `w:sdtPr`/`w:sdtEndPr` property subtree: guard its children so a
            // nested `w:rPr` cannot leak into run flow.
            b"sdtPr" | b"sdtEndPr" if !self.sdt_scopes.is_empty() => self.sdt_prop_depth += 1,
            // The block control's content opens a fresh suspended frame; an inline
            // control's content is inert (its segments route via `wrapper_order`).
            b"sdtContent" => {
                if self.sdt_scopes.last() == Some(&SdtScope::Block) {
                    self.enter_sdt_block()?;
                }
            }
            // Recognized `w:sdtPr` type markers set the control kind. The
            // combo/dropdown, date, and checkbox markers additionally open their
            // control-specific `data`, populated from the child markers below.
            b"richText" | b"text" | b"picture" | b"group" | b"repeatingSection" | b"citation"
            | b"bibliography"
                if self.sdt_prop_depth > 0 =>
            {
                let kind = sdt_control_kind(local);
                if let Some(properties) = self.current_sdt_properties() {
                    properties.control_kind = kind;
                }
            }
            b"comboBox" | b"dropDownList" if self.sdt_prop_depth > 0 => {
                let kind = sdt_control_kind(local);
                if let Some(properties) = self.current_sdt_properties() {
                    properties.control_kind = kind;
                    properties.data = Some(SdtControlData::List(Vec::new()));
                }
            }
            b"date" if self.sdt_prop_depth > 0 => {
                let full_date = attribute_value(element, b"fullDate")
                    .filter(|v| !v.is_empty() && v.len() <= 64);
                if let Some(properties) = self.current_sdt_properties() {
                    properties.control_kind = Some(SdtControlKind::Date);
                    properties.data = Some(SdtControlData::Date(SdtDate {
                        full_date,
                        ..SdtDate::default()
                    }));
                }
            }
            b"checkbox" if self.sdt_prop_depth > 0 => {
                if let Some(properties) = self.current_sdt_properties() {
                    properties.control_kind = Some(SdtControlKind::Checkbox);
                    properties.data = Some(SdtControlData::Checkbox(SdtCheckbox::default()));
                }
            }
            // A combo/dropdown choice entry (`w:listItem`): display + value.
            b"listItem" if self.sdt_prop_depth > 0 => {
                let display = attribute_value(element, b"displayText")
                    .filter(|v| !v.is_empty() && v.len() <= 255);
                let value = attribute_value(element, b"value")
                    .filter(|v| v.len() <= 255)
                    .unwrap_or_default();
                if let Some(SdtControlData::List(items)) = self.current_sdt_data()
                    && items.len() < 1024
                {
                    items.push(SdtListItem { display, value });
                }
            }
            // Date-picker detail children (`w:date`'s format/lid/calendar/mapping).
            b"dateFormat" if self.sdt_prop_depth > 0 => {
                let value =
                    attribute_value(element, b"val").filter(|v| !v.is_empty() && v.len() <= 255);
                if let Some(SdtControlData::Date(date)) = self.current_sdt_data() {
                    date.date_format = value;
                }
            }
            b"lid" if self.sdt_prop_depth > 0 => {
                let value =
                    attribute_value(element, b"val").filter(|v| !v.is_empty() && v.len() <= 64);
                if let Some(SdtControlData::Date(date)) = self.current_sdt_data() {
                    date.lid = value;
                }
            }
            b"calendar" if self.sdt_prop_depth > 0 => {
                let value =
                    attribute_value(element, b"val").filter(|v| !v.is_empty() && v.len() <= 64);
                if let Some(SdtControlData::Date(date)) = self.current_sdt_data() {
                    date.calendar = value;
                }
            }
            b"storeMappedDataAs" if self.sdt_prop_depth > 0 => {
                let value =
                    attribute_value(element, b"val").filter(|v| !v.is_empty() && v.len() <= 64);
                if let Some(SdtControlData::Date(date)) = self.current_sdt_data() {
                    date.store_mapped_as = value;
                }
            }
            // Checkbox detail children (`w14:checked`/`w14:checkedState`/`w14:uncheckedState`).
            b"checked" if self.sdt_prop_depth > 0 => {
                let on = is_true(attribute_value(element, b"val").as_deref());
                if let Some(SdtControlData::Checkbox(checkbox)) = self.current_sdt_data() {
                    checkbox.checked = on;
                }
            }
            b"checkedState" if self.sdt_prop_depth > 0 => {
                let symbol = sdt_checkbox_symbol(element);
                if let Some(SdtControlData::Checkbox(checkbox)) = self.current_sdt_data() {
                    checkbox.checked_state = symbol;
                }
            }
            b"uncheckedState" if self.sdt_prop_depth > 0 => {
                let symbol = sdt_checkbox_symbol(element);
                if let Some(SdtControlData::Checkbox(checkbox)) = self.current_sdt_data() {
                    checkbox.unchecked_state = symbol;
                }
            }
            // The customXML data binding (`w:dataBinding`): `w:xpath` is required;
            // without it the binding is meaningless, so it is reported and dropped.
            b"dataBinding" if self.sdt_prop_depth > 0 => {
                let xpath =
                    attribute_value(element, b"xpath").filter(|v| !v.is_empty() && v.len() <= 1024);
                if let Some(xpath) = xpath {
                    let store_item_id = attribute_value(element, b"storeItemID")
                        .filter(|v| !v.is_empty() && v.len() <= 128);
                    let prefix_mappings = attribute_value(element, b"prefixMappings")
                        .filter(|v| !v.is_empty() && v.len() <= 1024);
                    if let Some(properties) = self.current_sdt_properties() {
                        properties.data_binding = Some(SdtDataBinding {
                            xpath,
                            store_item_id,
                            prefix_mappings,
                        });
                    }
                } else {
                    self.reporter.report(b"dataBinding");
                }
            }
            // The edit-lock behaviour (`w:lock@w:val`); an unknown token is reported.
            b"lock" if self.sdt_prop_depth > 0 => {
                match attribute_value(element, b"val").as_deref().map(sdt_lock) {
                    Some(Some(lock)) => {
                        if let Some(properties) = self.current_sdt_properties() {
                            properties.lock = Some(lock);
                        }
                    }
                    _ => self.reporter.report(b"lock"),
                }
            }
            // The placeholder wrapper is a transparent container; its `w:docPart`
            // child carries the building-block name.
            b"placeholder" if self.sdt_prop_depth > 0 => {}
            b"docPart" if self.sdt_prop_depth > 0 => {
                let value = self.sdt_bounded_value(element, b"placeholder", 255);
                if let Some(properties) = self.current_sdt_properties() {
                    properties.placeholder = value;
                }
            }
            b"showingPlcHdr" if self.sdt_prop_depth > 0 => {
                let on = is_true(attribute_value(element, b"val").as_deref());
                if let Some(properties) = self.current_sdt_properties() {
                    properties.showing_placeholder = on;
                }
            }
            b"temporary" if self.sdt_prop_depth > 0 => {
                let on = is_true(attribute_value(element, b"val").as_deref());
                if let Some(properties) = self.current_sdt_properties() {
                    properties.temporary = on;
                }
            }
            // Both building-block gallery forms collapse to one control kind, so the
            // `w:docPartObj` vs `w:docPartList` distinction is reported as lost.
            b"docPartObj" | b"docPartList" if self.sdt_prop_depth > 0 => {
                self.reporter.report(local);
                if let Some(properties) = self.current_sdt_properties() {
                    properties.control_kind = Some(SdtControlKind::BuildingBlockGallery);
                }
            }
            b"alias" if self.sdt_prop_depth > 0 => {
                let value = self.sdt_bounded_value(element, b"alias", 255);
                if let Some(properties) = self.current_sdt_properties() {
                    properties.alias = value;
                }
            }
            b"tag" if self.sdt_prop_depth > 0 => {
                let value = self.sdt_bounded_value(element, b"tag", 255);
                if let Some(properties) = self.current_sdt_properties() {
                    properties.tag = value;
                }
            }
            b"id" if self.sdt_prop_depth > 0 => {
                let value = self.sdt_bounded_value(element, b"id", 64);
                if let Some(properties) = self.current_sdt_properties() {
                    properties.control_id = value;
                }
            }
            // Any other element inside `w:sdtPr`/`w:sdtEndPr` (lock, placeholder,
            // dataBinding, list entries, date/checkbox detail, end-mark `w:rPr`) is
            // the reported long tail. Placed BEFORE the generic rPr/pPr/flow arms
            // so a `w:sdtPr` `w:rPr` can never leak into run/paragraph flow.
            _ if self.sdt_prop_depth > 0 => self.reporter.report(local),
            _ if self.rpr_depth > 0 => {
                if !apply_run_property(&mut self.run_properties, local, element) {
                    self.reporter.report(local);
                }
            }
            // Paragraph-mark `w:rPr` children: the pilcrow's own run formatting,
            // accumulated separately from both the run rPr and the paragraph props.
            _ if self.mark_rpr_depth > 0 => {
                if !apply_run_property(&mut self.mark_run_properties, local, element) {
                    self.reporter.report(local);
                }
            }
            // A `w:pPr` child, but NOT one inside the paragraph mark's `w:rPr`
            // (`mark_rpr_depth`): a mark-rPr run property (e.g. `snapToGrid`, which
            // is valid on both pPr and rPr) must not be misattributed to the
            // paragraph. Mark-rPr children are the reported long tail until the
            // mark run is modeled.
            _ if self.ppr_depth > 0 && self.mark_rpr_depth == 0 => {
                if !apply_paragraph_property(&mut self.paragraph_properties, local, element) {
                    self.reporter.report(local);
                }
            }
            // Known DrawingML scaffolding for an embedded picture is consumed
            // silently; any OTHER element inside a drawing (e.g. a text box)
            // still falls through to the report arm below — no silent loss.
            _ if self.drawing_depth > 0 && is_drawing_scaffolding(local) => {}
            // Known VML shape scaffolding inside a `w:object` is consumed silently
            // (the object round-trips as a first-class reference to its preserved
            // parts, so the presentation shape is not separately modeled).
            _ if self.object_depth > 0 && is_object_scaffolding(local) => {}
            _ if self.in_document => self.reporter.report(local),
            _ => {}
        }
        Ok(())
    }

    fn resolve_style(
        &self,
        element: &BytesStart<'_>,
        expected: StyleKind,
    ) -> Option<casual_doc_model::v1::StyleId> {
        let name = attribute_value(element, b"val")?;
        self.styles.resolve(&name, expected)
    }

    fn on_end(&mut self, local: &[u8]) -> Result<(), ImportError> {
        if self.mc_skip_depth > 0 {
            self.mc_skip_depth -= 1;
            return Ok(());
        }
        match local {
            // Close of a modeled property-change capture: the prior snapshot has
            // accumulated into the live accumulator, so restore the saved current
            // properties with the built change attached. Placed BEFORE the skip
            // catch so it is not swallowed; guarded on the capture stack's top so
            // it only fires for a change we actually opened.
            b"rPrChange" | b"pPrChange" | b"tblPrChange" | b"trPrChange" | b"tcPrChange"
            | b"tblGridChange"
                if self.prop_change_top_matches(local) =>
            {
                self.finish_prop_change();
            }
            // Close of a reported-and-skipped change (`w:sectPrChange` /
            // `w:numberingChange`); balances the on_start increment. Placed BEFORE
            // the skip catch so it is not swallowed.
            b"sectPrChange" | b"numberingChange" if self.pr_change_depth > 0 => {
                self.pr_change_depth = self.pr_change_depth.saturating_sub(1);
            }
            // Inside a reported-and-skipped change: swallow every close so the
            // nested historical property containers never decrement the real depth
            // counters (they never incremented them — their opens were skipped).
            _ if self.pr_change_depth > 0 => {}
            b"document" => self.in_document = false,
            b"body" => self.in_body = false,
            // An `w:altChunk` closes: commit the accumulated chunk as a first-class
            // block, routed into the open table cell if any, else the body root
            // (like a paragraph). An unresolved chunk left `pending_alt_chunk`
            // `None`, so this is inert.
            b"altChunk" => {
                if let Some(chunk) = self.pending_alt_chunk.take() {
                    let id = self.next_id()?;
                    let block = BlockNode::AltChunk(AltChunk {
                        id,
                        part: chunk.part,
                        properties: chunk.properties,
                    });
                    if let Some(returned) = self.tables.push_block(block) {
                        self.blocks.push(returned);
                    }
                }
            }
            b"AlternateContent" => {
                self.alt_stack.pop();
            }
            b"txbxContent" => self.exit_frame()?,
            // A block control's content closes: build and route the `BlockNode::Sdt`.
            // The block frame emptied `sdt_scopes` when it suspended the enclosing
            // context, so its own `</w:sdtContent>` sees a drained scope stack over a
            // `BlockSdt` frame; an inline/passthrough control's `</w:sdtContent>`
            // still sees its `Inline`/`Passthrough` scope on top and is inert.
            b"sdtContent" => {
                if self.sdt_scopes.last().is_none()
                    && self.frames.last().map(|frame| frame.kind) == Some(FrameKind::BlockSdt)
                {
                    self.exit_frame()?;
                }
            }
            b"sdtPr" | b"sdtEndPr" => self.sdt_prop_depth = self.sdt_prop_depth.saturating_sub(1),
            // A content control closes: pop its scope, and (inline) commit its
            // accumulated segments as an `InlineSdt`.
            b"sdt" => match self.sdt_scopes.pop() {
                Some(SdtScope::Inline) => {
                    self.sdt_depth = self.sdt_depth.saturating_sub(1);
                    // A dangling inner wrapper — e.g. an unterminated `fldChar`
                    // field, which is delimited by markers, not an XML element —
                    // may sit ABOVE this control on the wrapper stack when its
                    // `</w:sdt>` fires. Drain those first so the control is the
                    // innermost wrapper before it commits, mirroring the
                    // `finish_paragraph` drain; otherwise `commit_sdt`'s
                    // `pop_wrapper(Sdt)` would fire on the wrong wrapper (a panic
                    // in debug, a silent desync/loss in release).
                    while !self.wrapper_order.is_empty()
                        && !matches!(self.wrapper_order.last(), Some(WrapperKind::Sdt))
                    {
                        let before = self.wrapper_order.len();
                        self.commit_top_wrapper();
                        if self.wrapper_order.len() == before {
                            self.wrapper_order.pop();
                        }
                    }
                    self.commit_sdt();
                }
                Some(SdtScope::Block) => {
                    self.sdt_depth = self.sdt_depth.saturating_sub(1);
                    // Defensive: a block control whose `w:sdtContent` never arrived
                    // leaves its pending properties behind; drop them so nothing
                    // leaks (a frame left open is unwound at container close).
                    self.pending_block_sdt_props.pop();
                }
                Some(SdtScope::Passthrough) | None => {}
            },
            _ if self.note_container == Some(local) => self.close_note()?,
            b"p" if self.paragraph_open => self.finish_paragraph()?,
            b"pPr" => self.ppr_depth = self.ppr_depth.saturating_sub(1),
            b"numPr" => {
                self.numpr_depth = self.numpr_depth.saturating_sub(1);
                if self.numpr_depth == 0 {
                    if let Some(num_id) = self.pending_num_id.take() {
                        match self.numbering.resolve(&num_id, self.pending_ilvl) {
                            Some(reference) => {
                                self.paragraph_properties.numbering = Some(reference);
                            }
                            None => self.reporter.report(b"numPr"),
                        }
                    }
                    self.pending_ilvl = 0;
                }
            }
            b"r" => {
                self.run_open = false;
                self.rpr_depth = 0;
            }
            b"rPr" => {
                // Key the close on `run_open`: a `w:rPr` closing while a run is
                // open is that run's rPr (`rpr_depth`); otherwise it is the
                // paragraph mark's rPr (`mark_rpr_depth`). Keying on the counter
                // alone would let a run rPr nested (malformed) inside an open mark
                // rPr wrongly drain `mark_rpr_depth`.
                if self.run_open {
                    self.rpr_depth = self.rpr_depth.saturating_sub(1);
                } else if self.mark_rpr_depth > 0 {
                    self.mark_rpr_depth -= 1;
                } else {
                    self.rpr_depth = self.rpr_depth.saturating_sub(1);
                }
            }
            b"sectPr" => {
                if let Some(accumulator) = self.section.take() {
                    let id = self.build_section(accumulator)?;
                    // A section closed while a paragraph's `pPr` is still open is a
                    // per-paragraph break: link it to the paragraph. The body-level
                    // section (no paragraph open) is just pushed to `sections`.
                    if self.paragraph_open && self.ppr_depth > 0 {
                        self.paragraph_properties.section_break = Some(id);
                    }
                }
            }
            b"t" | b"delText" if self.in_text => {
                self.in_text = false;
                let text = std::mem::take(&mut self.text_buffer);
                // A ruby annotation (`w:rt`) is a phonetic guide, not base text;
                // its text is dropped (reported on `</w:rt>`) so the base reads in
                // document order instead of being merged with the annotation.
                if !text.is_empty() && self.ruby_annotation_depth == 0 {
                    let properties = self.run_properties.clone();
                    self.push_segment(Segment::Run { properties, text });
                }
            }
            b"rt" if self.ruby_annotation_depth > 0 => {
                self.ruby_annotation_depth -= 1;
                // The annotation text is dropped; report that the ruby guide is
                // not modeled (its base text is preserved in document order).
                self.reporter.report(b"rt");
            }
            b"instrText" if self.in_instr => {
                self.in_instr = false;
                let text = std::mem::take(&mut self.instr_buffer);
                match self.field.as_mut() {
                    // Append to the open field's instruction while collecting it.
                    Some(field) if !field.in_result => field.instruction.push_str(&text),
                    // Instruction text with no field collecting it (orphaned, or
                    // after `separate`) is not modeled; report it (never silent).
                    _ if !text.is_empty() => self.reporter.report(b"instrText"),
                    _ => {}
                }
            }
            b"fldSimple" if self.field_depth > 0 => self.close_field(),
            // The form field's `w:ffData` block closes; its builder stays on the
            // open field and is finalized when the field commits.
            b"ffData" if self.in_ffdata => self.in_ffdata = false,
            b"blipFill" => self.blipfill_depth = self.blipfill_depth.saturating_sub(1),
            // A `wp:posOffset`/`wp:align` closes: parse the captured value into the
            // current axis's position.
            b"posOffset" | b"align" if self.capturing_anchor_axis() => self.finish_anchor_value(),
            // A `wp:positionH`/`wp:positionV` closes: clear the capture axis.
            b"positionH" | b"positionV" if self.pending_anchor.is_some() => {
                if let Some(anchor) = self.pending_anchor.as_mut() {
                    anchor.capture_axis = None;
                }
            }
            // A DrawingML color element closes: fold its modifiers and assign the
            // resulting color to the open shape's fill or stroke.
            b"srgbClr" | b"schemeClr" | b"sysClr" if self.pending_color.is_some() => {
                self.commit_color();
            }
            // An outline closes: leave the shape's captured stroke, drop the depth.
            b"ln" if self.ln_depth > 0 => {
                self.ln_depth = self.ln_depth.saturating_sub(1);
            }
            b"lnRef" | b"fillRef" | b"effectRef" | b"fontRef" if self.style_ref_depth > 0 => {
                self.style_ref_depth = self.style_ref_depth.saturating_sub(1);
            }
            // A transform container closes: stop routing off/ext.
            b"spPr" | b"grpSpPr" if self.drawing_depth > 0 => {
                self.xfrm_target = XfrmTarget::None;
            }
            // A picture child closes: finalize it into the open group.
            b"pic"
                if self
                    .pending_shape
                    .as_ref()
                    .is_some_and(|shape| shape.is_picture) =>
            {
                self.commit_shape();
            }
            // A shape/text box closes: finalize it into a group child, or (outside a
            // group) into a floating/inline text box segment.
            b"wsp" | b"cxnSp" if self.pending_shape.is_some() => {
                self.commit_shape();
            }
            // A group closes: stash the top-level group (`wpg:wgp`) for the enclosing
            // drawing, or fold a nested group (`wpg:grpSp`) into its parent.
            b"wgp" | b"grpSp" if !self.group_stack.is_empty() => {
                self.commit_group();
            }
            b"drawing" if self.drawing_depth > 0 => {
                self.drawing_depth -= 1;
                if self.drawing_depth == 0 {
                    self.commit_drawing();
                }
            }
            b"pict" if self.pict_depth > 0 => {
                self.pict_depth -= 1;
                if self.pict_depth == 0 {
                    self.commit_pict()?;
                }
            }
            b"object" if self.object_depth > 0 => {
                self.object_depth -= 1;
                if self.object_depth == 0 {
                    self.commit_object();
                }
            }
            b"hyperlink" if self.hyperlink_depth > 0 => {
                if self.hyperlink_depth == 1 {
                    self.commit_hyperlink();
                }
                self.hyperlink_depth = self.hyperlink_depth.saturating_sub(1);
            }
            // An excluded (reported, not modeled) `w:ins`/`w:del` closes: balance
            // the suppression counter — never commit an enclosing real revision.
            // Checked BEFORE the commit arm because an excluded range is always
            // inner to any open real revision, so its close arrives first.
            b"ins" | b"del" | b"moveFrom" | b"moveTo" if self.suppressed_revision_depth > 0 => {
                self.suppressed_revision_depth -= 1;
            }
            // A real tracked-change/move range closes: commit it into the enclosing
            // wrapper (an outer revision/hyperlink) or the paragraph.
            b"ins" | b"del" | b"moveFrom" | b"moveTo" if self.top_wrapper_is_revision() => {
                self.commit_revision()
            }
            b"tcPr" => self.tcpr_depth = self.tcpr_depth.saturating_sub(1),
            b"tblPr" => self.tblpr_depth = self.tblpr_depth.saturating_sub(1),
            b"trPr" => self.trpr_depth = self.trpr_depth.saturating_sub(1),
            b"tblBorders" | b"tcBorders" | b"tblCellMar" | b"tcMar" | b"pBdr" | b"pgBorders" => {
                self.edge_scope = EdgeScope::None;
            }
            // A per-section note-properties container closes: leave the note scope
            // so trailing siblings do not route into it.
            b"footnotePr" | b"endnotePr" => self.section_note_scope = None,
            b"tabs" => self.in_tabs = false,
            // A refused subtree's own `</w:tbl>` closes suppression, never a real
            // table on the stack; its `</w:tc>`/`</w:tr>` are inert.
            b"tbl" if self.suppressed_tbl_depth > 0 => {
                self.suppressed_tbl_depth -= 1;
            }
            b"tc" if self.tables.is_active() && self.suppressed_tbl_depth == 0 => {
                self.tcpr_depth = 0;
                self.tables.close_cell(&mut *self.ids)?;
            }
            b"tr" if self.tables.is_active() && self.suppressed_tbl_depth == 0 => {
                if !self.tables.close_row() {
                    self.reporter.report(b"tr");
                }
            }
            b"tbl" if self.tables.is_active() => match self.tables.close_table() {
                Some(table) => {
                    if let Some(returned) = self.tables.push_block(BlockNode::Table(table)) {
                        self.blocks.push(returned);
                    }
                }
                None => self.reporter.report(b"tbl"),
            },
            _ => {}
        }
        Ok(())
    }

    /// Whether an open anchor is currently capturing a `wp:posOffset`/`wp:align`
    /// value for one of its axes.
    fn capturing_anchor_axis(&self) -> bool {
        self.pending_anchor
            .as_ref()
            .is_some_and(|anchor| anchor.capture_axis.is_some())
    }

    /// Parses the captured `wp:posOffset`/`wp:align` text into the current axis's
    /// position on the open anchor. An unparseable value leaves the position unset
    /// (resolved to a zero offset later), never panicking.
    fn finish_anchor_value(&mut self) {
        let Some(anchor) = self.pending_anchor.as_mut() else {
            return;
        };
        let Some(axis) = anchor.capture_axis else {
            return;
        };
        let is_offset = anchor.capture_is_offset;
        let buffer = std::mem::take(&mut anchor.capture_buffer);
        let value = buffer.trim();
        match (axis, is_offset) {
            (AnchorAxis::Horizontal, true) => {
                if let Ok(emu) = value.parse::<i64>() {
                    anchor.h_position =
                        Some(HorizontalPosition::Offset(emu.clamp(-MAX_EMU, MAX_EMU)));
                }
            }
            (AnchorAxis::Vertical, true) => {
                if let Ok(emu) = value.parse::<i64>() {
                    anchor.v_position =
                        Some(VerticalPosition::Offset(emu.clamp(-MAX_EMU, MAX_EMU)));
                }
            }
            (AnchorAxis::Horizontal, false) => {
                if let Some(align) = horizontal_align(value) {
                    anchor.h_position = Some(HorizontalPosition::Align(align));
                }
            }
            (AnchorAxis::Vertical, false) => {
                if let Some(align) = vertical_align(value) {
                    anchor.v_position = Some(VerticalPosition::Align(align));
                }
            }
        }
    }

    /// Commits the top-level drawing that just closed. A `c:chart`/`dgm:relIds`
    /// payload becomes a first-class `EmbeddedObject` referencing its preserved
    /// part(s); a resolved `a:blip@r:embed` becomes a `Drawing` (inline) or an
    /// `AnchoredDrawing` (floating `wp:anchor`); an unresolved/dangling reference
    /// is reported and dropped. A resolved drawing carrying unmodeled detail is
    /// also reported.
    /// Routes an `a:off`/`a:ext` (within an `a:xfrm`) to the group transform or the
    /// open shape, per [`Self::xfrm_target`].
    fn set_xfrm_offset(&mut self, point: PointEmu) {
        match self.xfrm_target {
            XfrmTarget::Group => {
                if let Some(group) = self.group_stack.last_mut() {
                    group.transform.offset = point;
                }
            }
            XfrmTarget::Shape => {
                if let Some(shape) = self.pending_shape.as_mut() {
                    shape.offset = point;
                }
            }
            XfrmTarget::None => {}
        }
    }

    fn set_xfrm_extent(&mut self, extent: Extent) {
        match self.xfrm_target {
            XfrmTarget::Group => {
                if let Some(group) = self.group_stack.last_mut() {
                    group.transform.extent = extent;
                }
            }
            XfrmTarget::Shape => {
                if let Some(shape) = self.pending_shape.as_mut() {
                    shape.extent = extent;
                }
            }
            XfrmTarget::None => {}
        }
    }

    /// Folds the open [`PendingColor`] and assigns it to the open shape's fill or
    /// stroke.
    fn commit_color(&mut self) {
        let Some(color) = self.pending_color.take() else {
            return;
        };
        let Some(shape) = self.pending_shape.as_mut() else {
            return;
        };
        let rgba = fold_color(&color);
        match color.dest {
            ColorDest::Fill => shape.fill = Some(rgba),
            ColorDest::Stroke => {
                shape.stroke = Some(ShapeStroke {
                    color: rgba,
                    width_emu: color.stroke_width_emu,
                });
            }
        }
    }

    /// Finalizes the open shape (`pic:pic`/`wps:wsp`) at its close. Inside a group
    /// it becomes a [`GroupChild`]; outside one, a text box (floating when an
    /// anchor is open, else inline). A bare anchored shape with no text is not
    /// first-class on its own, so it is reported and dropped.
    fn commit_shape(&mut self) {
        let Some(mut shape) = self.pending_shape.take() else {
            return;
        };
        if shape.is_picture {
            // The picture's `a:blip@r:embed` flowed through the shared
            // `pending_embed`; take it so the next sibling picture captures its own.
            shape.embed = self.pending_embed.take();
        }
        if !self.group_stack.is_empty() {
            if let Some(child) = self.shape_to_group_child(shape)
                && let Some(group) = self.group_stack.last_mut()
            {
                group.children.push(child);
            }
            return;
        }
        // Not in a group: a lone/inline DrawingML text box, or a bare shape.
        let Some(blocks) = shape.textbox_blocks.take() else {
            // A standalone anchored/inline shape (rectangle/line) with no text is
            // not modeled on its own; report so nothing is silently lost.
            self.reporter.report(b"wsp");
            return;
        };
        if blocks.is_empty() {
            self.reporter.report(b"txbxContent");
            return;
        }
        match self.pending_anchor.take() {
            Some(pending) => {
                // A floating text box: carry its anchor + extent + fill/border.
                let extent = self
                    .pending_extent
                    .take()
                    .or((shape.extent != ZERO_EXTENT).then_some(shape.extent))
                    .unwrap_or(ZERO_EXTENT);
                self.push_segment(Segment::TextBox(TextBox {
                    id: shape.id,
                    anchor: Some(pending.resolve()),
                    relative_height: pending.relative_height,
                    extent: Some(extent),
                    fill: shape.fill,
                    border: shape.stroke,
                    blocks,
                }));
            }
            None => {
                // Preserve an inline shape's authored appearance and size. Do not
                // take `pending_extent`: the same drawing can contain a sibling
                // picture whose commit path still needs the shared `wp:extent`.
                let extent = self
                    .pending_extent
                    .or((shape.extent != ZERO_EXTENT).then_some(shape.extent));
                self.push_segment(Segment::TextBox(TextBox {
                    id: shape.id,
                    anchor: None,
                    relative_height: None,
                    extent,
                    fill: shape.fill,
                    border: shape.stroke,
                    blocks,
                }));
            }
        }
    }

    /// Converts a finished shape into a [`GroupChild`]. A picture with an
    /// unresolved media reference, or a text box with no blocks, is reported and
    /// dropped (`None`).
    fn shape_to_group_child(&mut self, mut shape: ShapeBuilder) -> Option<GroupChild> {
        if shape.is_picture {
            let embed = shape.embed.as_deref()?;
            let media = *self.media_index.get(embed)?;
            return Some(GroupChild::Picture(GroupPicture {
                id: shape.id,
                media,
                offset: shape.offset,
                extent: shape.extent,
                descr: shape.descr,
            }));
        }
        if let Some(blocks) = shape.textbox_blocks.take() {
            if blocks.is_empty() {
                self.reporter.report(b"txbxContent");
                return None;
            }
            return Some(GroupChild::TextBox(GroupTextBox {
                id: shape.id,
                offset: shape.offset,
                extent: shape.extent,
                blocks,
                fill: shape.fill,
                border: shape.stroke,
            }));
        }
        Some(GroupChild::Shape(GroupShape {
            id: shape.id,
            offset: shape.offset,
            extent: shape.extent,
            geometry: shape.geometry,
            fill: shape.fill,
            stroke: shape.stroke,
        }))
    }

    /// Finalizes the top open group at its close: a top-level `wpg:wgp` (with an
    /// anchor) is stashed in `pending_group` for the enclosing `w:drawing`; a
    /// nested `wpg:grpSp` is folded into its parent as a [`GroupChild::Group`]. An
    /// empty group (no children) is reported and dropped.
    fn commit_group(&mut self) {
        let Some(builder) = self.group_stack.pop() else {
            return;
        };
        if builder.children.is_empty() {
            self.reporter.report(b"wgp");
            return;
        }
        match builder.anchor {
            Some((anchor, extent, relative_height)) => {
                self.pending_group = Some(WordprocessingGroup {
                    id: builder.id,
                    anchor: Some(anchor),
                    relative_height,
                    extent,
                    transform: builder.transform,
                    children: builder.children,
                });
            }
            None => {
                let nested = WordprocessingGroup {
                    id: builder.id,
                    anchor: None,
                    relative_height: None,
                    extent: builder.transform.extent,
                    transform: builder.transform,
                    children: builder.children,
                };
                if let Some(parent) = self.group_stack.last_mut() {
                    parent.children.push(GroupChild::Group(nested));
                } else {
                    self.reporter.report(b"grpSp");
                }
            }
        }
    }

    fn commit_drawing(&mut self) {
        // A DrawingML group takes precedence: emit the whole positioned group
        // rather than collapsing to a single (stretched) picture.
        if let Some(group) = self.pending_group.take() {
            self.push_segment(Segment::Group(group));
            return;
        }
        let extent = self.pending_extent.take();
        let extra = self.drawing_extra;
        let graphic = std::mem::take(&mut self.pending_graphic);
        // A chart payload (`a:graphicData` -> `c:chart`).
        if let Some(rid) = &graphic.chart_rid
            && let Some(part) = self.resolve_embedded_part(rid)
        {
            self.embedded_part_names.insert(part.part_name.clone());
            self.push_segment(Segment::EmbeddedObject {
                kind: EmbeddedKind::Chart,
                part,
                extra_parts: Vec::new(),
                preview: None,
                extent: extent.unwrap_or(ZERO_EXTENT),
                prog_id: None,
            });
            return;
        }
        // A SmartArt diagram payload (`a:graphicData` -> `dgm:relIds`): the data
        // model is primary, layout/quick-style/colors are extra parts.
        if let Some(part) = self.resolve_embedded_part_opt(&graphic.diagram_dm) {
            self.embedded_part_names.insert(part.part_name.clone());
            let mut extra_parts = Vec::new();
            for rid in [
                &graphic.diagram_lo,
                &graphic.diagram_qs,
                &graphic.diagram_cs,
            ] {
                if let Some(extra_part) = self.resolve_embedded_part_opt(rid) {
                    self.embedded_part_names
                        .insert(extra_part.part_name.clone());
                    extra_parts.push(extra_part);
                }
            }
            self.push_segment(Segment::EmbeddedObject {
                kind: EmbeddedKind::Diagram,
                part,
                extra_parts,
                preview: None,
                extent: extent.unwrap_or(ZERO_EXTENT),
                prog_id: None,
            });
            return;
        }
        // Otherwise the embedded-picture path. An open anchor accumulator routes a
        // resolvable picture to an `AnchoredDrawing` (placed at its position); an
        // inline picture stays a `Drawing`.
        let anchor = self.pending_anchor.take();
        match self.pending_embed.take() {
            Some(embed) => match self.media_index.get(&embed) {
                Some(media) => {
                    if let Some(anchor) = anchor {
                        self.push_segment(Segment::AnchoredDrawing {
                            media: *media,
                            extent: extent.unwrap_or(ZERO_EXTENT),
                            anchor: anchor.resolve(),
                            descr: anchor.descr,
                            relative_height: anchor.relative_height,
                        });
                        // Any remaining unmodeled detail (e.g. a click-link) is
                        // still surfaced so the anchored drawing is never silently
                        // under-modeled.
                        if extra {
                            self.reporter.report(b"drawing");
                        }
                        return;
                    }
                    if extra {
                        self.reporter.report(b"drawing");
                    }
                    self.push_segment(Segment::Drawing {
                        media: *media,
                        extent,
                    });
                }
                None => self.reporter.report(b"drawing"),
            },
            None => self.reporter.report(b"drawing"),
        }
    }

    /// Commits a `w:object` OLE embedding that just closed into a first-class
    /// `EmbeddedObject` (kind `OleObject`) referencing the embedding part, with
    /// the optional preview image and `ProgID`. An unresolved embedding is
    /// reported and dropped.
    fn commit_object(&mut self) {
        let object = std::mem::take(&mut self.pending_object);
        let Some(part) = self.resolve_embedded_part_opt(&object.object_rid) else {
            self.reporter.report(b"object");
            return;
        };
        self.embedded_part_names.insert(part.part_name.clone());
        let preview = object
            .preview_rid
            .as_ref()
            .and_then(|rid| self.media_index.get(rid).copied());
        let prog_id = object
            .prog_id
            .filter(|value| !value.is_empty() && value.len() <= 255);
        self.push_segment(Segment::EmbeddedObject {
            kind: EmbeddedKind::OleObject,
            part,
            extra_parts: Vec::new(),
            preview,
            extent: object.extent.unwrap_or(ZERO_EXTENT),
            prog_id,
        });
    }

    /// Resolves a relationship id to an embedded-object part reference through the
    /// embedded index, bounding each field to the model's domain (an out-of-domain
    /// or unresolved id yields `None`, so the caller reports/drops).
    fn resolve_embedded_part(&self, relationship_id: &str) -> Option<EmbeddedPart> {
        let rel = self.embedded_index.get(relationship_id)?;
        if relationship_id.is_empty()
            || relationship_id.len() > 255
            || rel.relationship_type.is_empty()
            || rel.relationship_type.len() > 2048
            || rel.part_name.is_empty()
            || rel.part_name.len() > 1024
        {
            return None;
        }
        Some(EmbeddedPart {
            relationship_id: relationship_id.to_owned(),
            relationship_type: rel.relationship_type.clone(),
            part_name: rel.part_name.clone(),
        })
    }

    /// [`resolve_embedded_part`] for an optional id.
    fn resolve_embedded_part_opt(&self, relationship_id: &Option<String>) -> Option<EmbeddedPart> {
        self.resolve_embedded_part(relationship_id.as_deref()?)
    }

    /// Commits a legacy VML picture (`w:pict`) that just closed, mapping its
    /// positioned VML shapes onto the float layer.
    ///
    /// The pict's raw XML (captured by [`Self::capture_pict_event`]) is re-parsed by
    /// [`parse_vml_pict`] into a flat list of [`VmlDrawing`]s, each already carrying
    /// an absolute-twip box (group coordinate systems flattened). Each drawing maps
    /// onto the EXISTING float model: a `v:rect`/`v:roundrect`/`v:oval` becomes a
    /// standalone anchored [`GroupShape`] (a group-of-one), a `v:line` a line float,
    /// a `v:imagedata` a positioned [`AnchoredDrawing`], and a `v:textbox` a floating
    /// [`TextBox`] whose flowed blocks were redirected here by [`Self::exit_frame`].
    /// A bare inline `v:imagedata` with no absolute position keeps the legacy inline
    /// [`Segment::Drawing`] behaviour. A generic `v:shape` path is deferred (its
    /// bounding box is not filled — see [`Self::vml_shape_segment`]).
    fn commit_pict(&mut self) -> Result<(), ImportError> {
        let raw = self.pending_pict_xml.take();
        let drawings = raw.as_deref().map(parse_vml_pict).unwrap_or_default();
        let mut emitted = false;
        for drawing in &drawings {
            if let Some(segment) = self.vml_segment(drawing)? {
                self.push_segment(segment);
                emitted = true;
            }
        }
        if emitted {
            // The VML path consumed the picture; drop any stale inline embed so it is
            // not re-emitted as a duplicate inline image.
            self.pending_embed = None;
        } else {
            // No mappable positioned VML: preserve the legacy inline `v:imagedata`
            // behaviour (a bare inline image resolves through the media table).
            match self.pending_embed.take() {
                Some(id) => match self.media_index.get(&id) {
                    Some(media) => self.push_segment(Segment::Drawing {
                        media: *media,
                        extent: None,
                    }),
                    None => self.reporter.report(b"pict"),
                },
                // Report an image-less, shape-less pict only when it yielded nothing
                // at all (an unmodeled/empty fragment) so nothing is silently lost;
                // a deferred shape has already reported itself.
                None if drawings.is_empty() => self.reporter.report(b"pict"),
                None => {}
            }
        }
        Ok(())
    }

    /// Maps one parsed [`VmlDrawing`] onto a float-layer [`Segment`]. Returns `None`
    /// when the drawing is not mappable (an unresolved image rid, a deferred generic
    /// path shape) or is a `v:textbox` — a text box's flowed content was already
    /// emitted as an inline `TextBox` segment when its `w:txbxContent` closed (see
    /// [`Self::exit_frame`]), so it must not also be painted as a float.
    fn vml_segment(&mut self, drawing: &VmlDrawing) -> Result<Option<Segment>, ImportError> {
        // A positioned image (`v:imagedata@r:id`): a float, unless it is a genuinely
        // inline image (no absolute VML box), which keeps the legacy inline mapping.
        if let Some(rid) = &drawing.image_rid {
            let Some(&media) = self.media_index.get(rid) else {
                self.reporter.report(b"pict");
                return Ok(None);
            };
            if vml_is_floating(&drawing.position) {
                let (anchor, relative_height) = vml_anchor(&drawing.position);
                return Ok(Some(Segment::AnchoredDrawing {
                    media,
                    extent: vml_extent(&drawing.position),
                    anchor,
                    descr: None,
                    relative_height,
                }));
            }
            // Inline VML image: unchanged (the model does not capture CSS sizing).
            return Ok(Some(Segment::Drawing {
                media,
                extent: None,
            }));
        }
        // A VML text box (`v:textbox`): its flowed blocks were already emitted as an
        // inline `TextBox` segment, in document order, when `w:txbxContent` closed (see
        // `exit_frame`). Skip it here so the box is not also painted as a float — VML
        // text boxes render inline, because their absolute VML box positions overlap
        // each other and the body text on real documents. The box fill/border are
        // intentionally not carried onto a float.
        if drawing.textbox.is_some() {
            return Ok(None);
        }
        // A geometric shape (rule / callout box / line).
        self.vml_shape_segment(drawing)
    }

    /// Maps a geometric VML shape (`v:rect`/`v:roundrect`/`v:oval`/`v:line`/generic
    /// `v:shape`) onto a standalone anchored float, wrapped as a group-of-one so it
    /// reuses the float layer's group placement and z-order.
    fn vml_shape_segment(&mut self, drawing: &VmlDrawing) -> Result<Option<Segment>, ImportError> {
        // A generic `v:shape` path is approximated by the OUTLINE of its bounding
        // box, not its solid fill: the SDS callout frames are hairline path outlines
        // (a filled "ring" tracing a thin border), so filling the box would blacken
        // the whole callout, while a stroked box reproduces the frame LibreOffice
        // shows. Honoring the exact `path` is a follow-up (complex path geometry).
        let (geometry, fill, stroke) = match &drawing.kind {
            VmlShapeKind::Rect => (
                ShapeGeometry::Rectangle,
                vml_fill(&drawing.fill),
                vml_stroke(&drawing.stroke),
            ),
            VmlShapeKind::RoundRect { .. } => (
                ShapeGeometry::RoundRectangle,
                vml_fill(&drawing.fill),
                vml_stroke(&drawing.stroke),
            ),
            VmlShapeKind::Oval => (
                ShapeGeometry::Ellipse,
                vml_fill(&drawing.fill),
                vml_stroke(&drawing.stroke),
            ),
            VmlShapeKind::Line { from, to } => {
                return Ok(Some(self.vml_line_segment(drawing, *from, *to)?));
            }
            VmlShapeKind::Shape { .. } => (
                ShapeGeometry::Other,
                None,
                Some(vml_path_outline(&drawing.fill, &drawing.stroke)),
            ),
        };
        let extent = vml_extent(&drawing.position);
        let (anchor, relative_height) = vml_anchor(&drawing.position);
        let child = GroupChild::Shape(GroupShape {
            id: self.next_id()?,
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent,
            geometry,
            fill,
            stroke,
        });
        Ok(Some(Segment::Group(self.vml_group_of_one(
            anchor,
            relative_height,
            extent,
            child,
        )?)))
    }

    /// Maps a `v:line` onto a line float: the segment's bounding box becomes the
    /// float box (so the float layer draws its top-left→bottom-right diagonal, which
    /// is the segment for a horizontal/vertical rule or a main-diagonal line; an
    /// anti-diagonal line is a documented deferral).
    fn vml_line_segment(
        &mut self,
        drawing: &VmlDrawing,
        from: Option<(i64, i64)>,
        to: Option<(i64, i64)>,
    ) -> Result<Segment, ImportError> {
        let (fx, fy) = from.unwrap_or((0, 0));
        let (tx, ty) = to.unwrap_or((0, 0));
        let (left, top) = (fx.min(tx), fy.min(ty));
        let extent = Extent {
            width_emu: twip_emu_len((fx - tx).abs()),
            height_emu: twip_emu_len((fy - ty).abs()),
        };
        let (anchor, relative_height) = vml_anchor_at(&drawing.position, left, top);
        let child = GroupChild::Shape(GroupShape {
            id: self.next_id()?,
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent,
            geometry: ShapeGeometry::Line,
            fill: vml_fill(&drawing.fill),
            stroke: vml_stroke(&drawing.stroke),
        });
        Ok(Segment::Group(self.vml_group_of_one(
            anchor,
            relative_height,
            extent,
            child,
        )?))
    }

    /// Wraps a single float-layer child in an anchored group-of-one: an identity
    /// transform whose child space equals the box, so the child paints at the
    /// group's resolved origin sized to its own extent.
    fn vml_group_of_one(
        &mut self,
        anchor: DrawingAnchor,
        relative_height: Option<u32>,
        extent: Extent,
        child: GroupChild,
    ) -> Result<WordprocessingGroup, ImportError> {
        Ok(WordprocessingGroup {
            id: self.next_id()?,
            anchor: Some(anchor),
            relative_height,
            extent,
            transform: GroupTransform {
                offset: PointEmu { x_emu: 0, y_emu: 0 },
                extent,
                child_offset: PointEmu { x_emu: 0, y_emu: 0 },
                child_extent: extent,
            },
            children: vec![child],
        })
    }

    /// Parses a `w:shd`'s background fill: an explicit sRGB `@w:fill` becomes an
    /// `RgbColor`; `auto`/theme fills yield `None`. A real pattern (`@w:val` other
    /// than `clear`/`nil`) or a non-`auto` pattern color (`@w:color`) is also
    /// reported (degraded) so no visible shading is silently lost.
    fn shading_fill(&mut self, element: &BytesStart<'_>) -> Option<RgbColor> {
        let pattern_modeled = matches!(
            attribute_value(element, b"val").as_deref(),
            None | Some("clear") | Some("nil")
        );
        let pattern_color_default = matches!(
            attribute_value(element, b"color").as_deref(),
            None | Some("auto")
        );
        // A theme fill/color (`w:themeFill`/`w:themeColor`) carries a visible
        // background we do not model as sRGB; report it so it is not silently
        // lost (Word routinely emits `themeFill` without a duplicate `w:fill`).
        let has_theme = attribute_value(element, b"themeFill").is_some()
            || attribute_value(element, b"themeColor").is_some();
        if !pattern_modeled || !pattern_color_default || has_theme {
            self.reporter.report(b"shd");
        }
        attribute_value(element, b"fill")
            .filter(|value| value != "auto")
            .and_then(|value| parse_rgb(&value))
    }

    /// Routes a border/margin edge child (`top`/`start`/…) to the table or cell
    /// (per `tcpr_depth`) under the open [`EdgeScope`]. A border edge with no
    /// valid style, or a non-`dxa` margin, is reported (the container name) so no
    /// visible geometry is silently lost.
    fn apply_table_edge(&mut self, local: &[u8], element: &BytesStart<'_>) {
        let cell = self.tcpr_depth > 0;
        match self.edge_scope {
            EdgeScope::Borders => match self.build_border_edge(element) {
                Some(edge) if cell => self.tables.set_cell_border(local, edge),
                Some(edge) => self.tables.set_table_border(local, edge),
                None => self
                    .reporter
                    .report(if cell { b"tcBorders" } else { b"tblBorders" }),
            },
            EdgeScope::Margins => {
                // Margins have no inside edges.
                if matches!(local, b"insideH" | b"insideV") {
                    return;
                }
                let is_dxa = attribute_value(element, b"type")
                    .map(|kind| kind == "dxa")
                    .unwrap_or(true);
                match attr_i32(element, b"w") {
                    Some(width) if is_dxa => {
                        let width = width.clamp(0, 31_680);
                        if cell {
                            self.tables.set_cell_margin(local, width);
                        } else {
                            self.tables.set_table_margin(local, width);
                        }
                    }
                    _ => self
                        .reporter
                        .report(if cell { b"tcMar" } else { b"tblCellMar" }),
                }
            }
            EdgeScope::ParagraphBorders => match self.build_border_edge(element) {
                Some(edge) => {
                    let borders = &mut self.paragraph_properties.borders;
                    match local {
                        b"top" => borders.top = Some(edge),
                        b"bottom" => borders.bottom = Some(edge),
                        b"start" | b"left" => borders.start = Some(edge),
                        b"end" | b"right" => borders.end = Some(edge),
                        b"between" => borders.between = Some(edge),
                        b"bar" => borders.bar = Some(edge),
                        _ => {}
                    }
                }
                None => self.reporter.report(b"pBdr"),
            },
            EdgeScope::PageBorders => match self.build_border_edge(element) {
                Some(edge) => {
                    if let Some(section) = self.section.as_mut() {
                        match local {
                            b"top" => section.page_border_top = Some(edge),
                            b"bottom" => section.page_border_bottom = Some(edge),
                            b"start" | b"left" => section.page_border_start = Some(edge),
                            b"end" | b"right" => section.page_border_end = Some(edge),
                            _ => {}
                        }
                    }
                }
                None => self.reporter.report(b"pgBorders"),
            },
            EdgeScope::None => {}
        }
    }

    /// The open section's footnote or endnote property accumulator, selected by
    /// the current [`SectionNoteScope`]. `None` when no section or note scope is
    /// open (the caller then drops the value).
    fn section_note_props(&mut self) -> Option<&mut NoteProperties> {
        let scope = self.section_note_scope?;
        let section = self.section.as_mut()?;
        Some(match scope {
            SectionNoteScope::Footnote => &mut section.footnote_props,
            SectionNoteScope::Endnote => &mut section.endnote_props,
        })
    }

    /// Maps a `w:tabs > w:tab` custom tab stop. A `clear` or unknown alignment, a
    /// missing/out-of-range `w:pos`, or an overflow past the bound is reported.
    fn apply_tab_stop(&mut self, element: &BytesStart<'_>) {
        let alignment = match attribute_value(element, b"val").as_deref() {
            Some("start" | "left") => TabAlignment::Start,
            Some("center") => TabAlignment::Center,
            Some("end" | "right") => TabAlignment::End,
            Some("decimal") => TabAlignment::Decimal,
            Some("bar") => TabAlignment::Bar,
            _ => {
                self.reporter.report(b"tab");
                return;
            }
        };
        let position_twips = match attr_i32(element, b"pos") {
            Some(pos) if (-31_680..=31_680).contains(&pos) => pos,
            _ => {
                self.reporter.report(b"tab");
                return;
            }
        };
        let leader = match attribute_value(element, b"leader").as_deref() {
            Some("dot") => Some(TabLeader::Dot),
            Some("hyphen") => Some(TabLeader::Hyphen),
            Some("underscore") => Some(TabLeader::Underscore),
            Some("middleDot") => Some(TabLeader::MiddleDot),
            Some("heavy") => Some(TabLeader::Heavy),
            _ => None,
        };
        if self.paragraph_properties.tabs.len() < 128 {
            self.paragraph_properties.tabs.push(TabStop {
                position_twips,
                alignment,
                leader,
            });
        } else {
            self.reporter.report(b"tab");
        }
    }

    /// Builds a `BorderEdge` from an edge element's attributes. Returns `None`
    /// (caller reports) when the required `w:val` style is missing/empty/oversized.
    fn build_border_edge(&self, element: &BytesStart<'_>) -> Option<BorderEdge> {
        let style = attribute_value(element, b"val").filter(|v| !v.is_empty() && v.len() <= 32)?;
        let size_eighth_points = attribute_value(element, b"sz")
            .and_then(|value| value.parse::<u32>().ok())
            .map(|size| size.min(1024));
        // Only an explicit sRGB color is modeled; `auto`/theme colors are dropped
        // (the edge itself is still captured).
        let color = attribute_value(element, b"color")
            .filter(|value| value != "auto")
            .and_then(|value| parse_rgb(&value));
        let space_points = attribute_value(element, b"space")
            .and_then(|value| value.parse::<u32>().ok())
            .map(|space| space.min(31));
        Some(BorderEdge {
            style,
            size_eighth_points,
            color,
            space_points,
        })
    }

    /// Applies a `w:tblLook`: either the explicit boolean attributes
    /// (`firstRow`/`lastRow`/…) or, failing those, the legacy hex `@w:val`
    /// bitmask (`0x0020` first-row, `0x0040` last-row, `0x0080` first-col,
    /// `0x0100` last-col, `0x0200` no-h-band, `0x0400` no-v-band).
    fn apply_table_look(&mut self, element: &BytesStart<'_>) {
        let flags: [&[u8]; 6] = [
            b"firstRow",
            b"lastRow",
            b"firstColumn",
            b"lastColumn",
            b"noHBand",
            b"noVBand",
        ];
        let mut any_explicit = false;
        for flag in flags {
            if let Some(value) = attribute_value(element, flag) {
                any_explicit = true;
                self.tables.set_table_look_flag(flag, is_true(Some(&value)));
            }
        }
        if any_explicit {
            return;
        }
        if let Some(mask) = attribute_value(element, b"val")
            .and_then(|value| u32::from_str_radix(value.trim_start_matches("0x"), 16).ok())
        {
            self.tables
                .set_table_look_flag(b"firstRow", mask & 0x0020 != 0);
            self.tables
                .set_table_look_flag(b"lastRow", mask & 0x0040 != 0);
            self.tables
                .set_table_look_flag(b"firstColumn", mask & 0x0080 != 0);
            self.tables
                .set_table_look_flag(b"lastColumn", mask & 0x0100 != 0);
            self.tables
                .set_table_look_flag(b"noHBand", mask & 0x0200 != 0);
            self.tables
                .set_table_look_flag(b"noVBand", mask & 0x0400 != 0);
        }
    }

    fn build_section(&mut self, accumulator: SectionAccumulator) -> Result<SectionId, ImportError> {
        let id = SectionId::new(self.next_id()?);
        let page_size = PageSize {
            width_twips: accumulator.page_width.unwrap_or(12_240).clamp(1, 31_680),
            height_twips: accumulator.page_height.unwrap_or(15_840).clamp(1, 31_680),
        };
        let page_margins = PageMargins {
            top_twips: accumulator.margin_top.unwrap_or(1_440).clamp(0, 31_680),
            bottom_twips: accumulator.margin_bottom.unwrap_or(1_440).clamp(0, 31_680),
            start_twips: accumulator.margin_start.unwrap_or(1_440).clamp(0, 31_680),
            end_twips: accumulator.margin_end.unwrap_or(1_440).clamp(0, 31_680),
            header_twips: accumulator.margin_header.map(|v| v.clamp(0, 31_680)),
            footer_twips: accumulator.margin_footer.map(|v| v.clamp(0, 31_680)),
        };
        let columns = SectionColumns {
            count: accumulator.columns.unwrap_or(1).clamp(1, 64),
            space_twips: accumulator.column_space.map(|v| v.clamp(0, 31_680)),
            separator: accumulator.column_separator,
            equal_width: accumulator.column_equal_width,
            columns: accumulator.column_defs,
        };
        let page_numbering = PageNumbering {
            format: accumulator.page_number_format,
            start: accumulator.page_number_start.map(|v| v.clamp(0, 1_000_000)),
        };
        let doc_grid = DocGrid {
            grid_type: accumulator.doc_grid_type,
            line_pitch: accumulator.doc_grid_line_pitch.map(|v| v.clamp(0, 31_680)),
            char_space: accumulator.doc_grid_char_space.map(|v| v.clamp(0, 31_680)),
        };
        let paper_source = PaperSource {
            first: accumulator.paper_first.map(|v| v.clamp(0, 32_767)),
            other: accumulator.paper_other.map(|v| v.clamp(0, 32_767)),
        };
        let page_borders = PageBorders {
            display: accumulator.page_border_display,
            offset_from: accumulator.page_border_offset,
            top: accumulator.page_border_top,
            bottom: accumulator.page_border_bottom,
            start: accumulator.page_border_start,
            end: accumulator.page_border_end,
        };
        let line_numbering = LineNumbering {
            count_by: accumulator.line_count_by.map(|v| v.clamp(0, 32_767)),
            start: accumulator.line_start.map(|v| v.clamp(0, 32_767)),
            distance: accumulator.line_distance.map(|v| v.clamp(0, 31_680)),
            restart: accumulator.line_restart,
        };
        let clamp_note = |mut props: NoteProperties| -> NoteProperties {
            props.number_start = props.number_start.map(|v| v.clamp(0, 1_000_000));
            props
        };
        self.sections.push(SectionBoundary {
            id,
            page_size,
            page_margins,
            columns,
            headers: accumulator.headers,
            footers: accumulator.footers,
            section_type: accumulator.section_type,
            title_page: accumulator.title_page,
            vertical_alignment: accumulator.vertical_alignment,
            page_numbering,
            doc_grid,
            orientation: accumulator.orientation,
            paper_source,
            page_borders,
            line_numbering,
            footnote_props: clamp_note(accumulator.footnote_props),
            endnote_props: clamp_note(accumulator.endnote_props),
            text_direction: accumulator.text_direction,
            bidi: accumulator.bidi,
        });
        Ok(id)
    }

    /// Enters a text box: allocate its id (document order), then suspend the
    /// enclosing content context so the box's own paragraphs/runs/tables build
    /// into a fresh context and cannot corrupt the enclosing paragraph or drawing.
    fn enter_textbox(&mut self) -> Result<(), ImportError> {
        let node_id = self.next_id()?;
        // Text-box-only nesting depth (block-sdt frames do not bump it).
        self.open_textboxes += 1;
        let depth = self.open_textboxes;
        self.push_frame(FrameKind::TextBox, node_id, depth, SdtProperties::default());
        Ok(())
    }

    /// Enters a block content control's content (`w:sdtContent`): allocate the sdt
    /// id (document order), take its accumulated properties, then suspend the
    /// enclosing context so the control's blocks build into a fresh container
    /// (its own table stack) exactly like a text box. A missing `w:sdtPr` yields
    /// default properties (never a panic).
    fn enter_sdt_block(&mut self) -> Result<(), ImportError> {
        let node_id = self.next_id()?;
        let properties = self.pending_block_sdt_props.pop().unwrap_or_default();
        self.push_frame(FrameKind::BlockSdt, node_id, 0, properties);
        Ok(())
    }

    /// Suspends the enclosing content context into a new [`ContentFrame`], so the
    /// frame's own paragraphs/runs/tables/controls build into a fresh context.
    fn push_frame(
        &mut self,
        kind: FrameKind,
        node_id: NodeId,
        depth: u32,
        sdt_properties: SdtProperties,
    ) {
        let frame = ContentFrame {
            kind,
            node_id,
            sdt_properties,
            depth,
            paragraph_open: std::mem::take(&mut self.paragraph_open),
            paragraph_id: self.paragraph_id.take(),
            paragraph_properties: std::mem::take(&mut self.paragraph_properties),
            ppr_depth: std::mem::take(&mut self.ppr_depth),
            numpr_depth: std::mem::take(&mut self.numpr_depth),
            pending_num_id: self.pending_num_id.take(),
            pending_ilvl: std::mem::take(&mut self.pending_ilvl),
            run_open: std::mem::take(&mut self.run_open),
            run_properties: std::mem::take(&mut self.run_properties),
            rpr_depth: std::mem::take(&mut self.rpr_depth),
            in_text: std::mem::take(&mut self.in_text),
            text_buffer: std::mem::take(&mut self.text_buffer),
            drawing_depth: std::mem::take(&mut self.drawing_depth),
            blipfill_depth: std::mem::take(&mut self.blipfill_depth),
            pending_embed: self.pending_embed.take(),
            pending_extent: self.pending_extent.take(),
            drawing_extra: std::mem::take(&mut self.drawing_extra),
            pict_depth: std::mem::take(&mut self.pict_depth),
            pending_graphic: std::mem::take(&mut self.pending_graphic),
            pending_anchor: self.pending_anchor.take(),
            object_depth: std::mem::take(&mut self.object_depth),
            pending_object: std::mem::take(&mut self.pending_object),
            group_stack: std::mem::take(&mut self.group_stack),
            pending_shape: self.pending_shape.take(),
            pending_group: self.pending_group.take(),
            pending_alt_chunk: self.pending_alt_chunk.take(),
            hyperlink: self.hyperlink.take(),
            hyperlink_depth: std::mem::take(&mut self.hyperlink_depth),
            field: self.field.take(),
            field_depth: std::mem::take(&mut self.field_depth),
            in_instr: std::mem::take(&mut self.in_instr),
            ruby_annotation_depth: std::mem::take(&mut self.ruby_annotation_depth),
            instr_buffer: std::mem::take(&mut self.instr_buffer),
            revisions: std::mem::take(&mut self.revisions),
            suppressed_revision_depth: std::mem::take(&mut self.suppressed_revision_depth),
            wrapper_order: std::mem::take(&mut self.wrapper_order),
            tables: std::mem::take(&mut self.tables),
            tcpr_depth: std::mem::take(&mut self.tcpr_depth),
            tblpr_depth: std::mem::take(&mut self.tblpr_depth),
            trpr_depth: std::mem::take(&mut self.trpr_depth),
            pr_change_depth: std::mem::take(&mut self.pr_change_depth),
            edge_scope: std::mem::replace(&mut self.edge_scope, EdgeScope::None),
            in_tabs: std::mem::take(&mut self.in_tabs),
            mark_rpr_depth: std::mem::take(&mut self.mark_rpr_depth),
            mark_rpr_seen: std::mem::take(&mut self.mark_rpr_seen),
            mark_run_properties: std::mem::take(&mut self.mark_run_properties),
            suppressed_tbl_depth: std::mem::take(&mut self.suppressed_tbl_depth),
            sdts: std::mem::take(&mut self.sdts),
            sdt_scopes: std::mem::take(&mut self.sdt_scopes),
            pending_block_sdt_props: std::mem::take(&mut self.pending_block_sdt_props),
            sdt_prop_depth: std::mem::take(&mut self.sdt_prop_depth),
            segments: std::mem::take(&mut self.segments),
            blocks: std::mem::take(&mut self.blocks),
        };
        self.frames.push(frame);
    }

    /// Exits a suspended frame (`w:txbxContent` or a block `w:sdtContent`): finish
    /// its content, restore the enclosing context, then emit either an inline
    /// `TextBox` segment or a `BlockNode::Sdt`. An empty (or over-deep text box)
    /// frame is reported and dropped (never silent).
    fn exit_frame(&mut self) -> Result<(), ImportError> {
        if self.paragraph_open {
            self.finish_paragraph()?;
        }
        let blocks = std::mem::take(&mut self.blocks);
        let Some(frame) = self.frames.pop() else {
            return Ok(());
        };
        self.paragraph_open = frame.paragraph_open;
        self.paragraph_id = frame.paragraph_id;
        self.paragraph_properties = frame.paragraph_properties;
        self.ppr_depth = frame.ppr_depth;
        self.numpr_depth = frame.numpr_depth;
        self.pending_num_id = frame.pending_num_id;
        self.pending_ilvl = frame.pending_ilvl;
        self.run_open = frame.run_open;
        self.run_properties = frame.run_properties;
        self.rpr_depth = frame.rpr_depth;
        self.in_text = frame.in_text;
        self.text_buffer = frame.text_buffer;
        self.drawing_depth = frame.drawing_depth;
        self.blipfill_depth = frame.blipfill_depth;
        self.pending_embed = frame.pending_embed;
        self.pending_extent = frame.pending_extent;
        self.drawing_extra = frame.drawing_extra;
        self.pict_depth = frame.pict_depth;
        self.pending_graphic = frame.pending_graphic;
        self.pending_anchor = frame.pending_anchor;
        self.object_depth = frame.object_depth;
        self.pending_object = frame.pending_object;
        self.group_stack = frame.group_stack;
        self.pending_shape = frame.pending_shape;
        self.pending_group = frame.pending_group;
        self.pending_alt_chunk = frame.pending_alt_chunk;
        self.hyperlink = frame.hyperlink;
        self.hyperlink_depth = frame.hyperlink_depth;
        self.field = frame.field;
        self.field_depth = frame.field_depth;
        self.in_instr = frame.in_instr;
        self.ruby_annotation_depth = frame.ruby_annotation_depth;
        self.instr_buffer = frame.instr_buffer;
        self.revisions = frame.revisions;
        self.suppressed_revision_depth = frame.suppressed_revision_depth;
        self.wrapper_order = frame.wrapper_order;
        self.tables = frame.tables;
        self.tcpr_depth = frame.tcpr_depth;
        self.tblpr_depth = frame.tblpr_depth;
        self.trpr_depth = frame.trpr_depth;
        self.pr_change_depth = frame.pr_change_depth;
        self.edge_scope = frame.edge_scope;
        self.in_tabs = frame.in_tabs;
        self.mark_rpr_depth = frame.mark_rpr_depth;
        self.mark_rpr_seen = frame.mark_rpr_seen;
        self.mark_run_properties = frame.mark_run_properties;
        self.suppressed_tbl_depth = frame.suppressed_tbl_depth;
        self.sdts = frame.sdts;
        self.sdt_scopes = frame.sdt_scopes;
        self.pending_block_sdt_props = frame.pending_block_sdt_props;
        self.sdt_prop_depth = frame.sdt_prop_depth;
        self.segments = frame.segments;
        self.blocks = frame.blocks;
        match frame.kind {
            FrameKind::TextBox => {
                self.open_textboxes = self.open_textboxes.saturating_sub(1);
                if blocks.is_empty() || frame.depth > MAX_TEXTBOX_DEPTH {
                    self.reporter.report(b"txbxContent");
                } else if let Some(shape) = self.pending_shape.as_mut() {
                    // A DrawingML `wps:wsp` text box: hand the flowed blocks to the
                    // open shape builder. The enclosing `wps:wsp` close routes it to
                    // a group child or a floating/inline text box; `frame.node_id`
                    // is unused (the shape's own id identifies the box).
                    shape.textbox_blocks = Some(blocks);
                } else {
                    // A legacy VML `v:textbox` (`w:pict`): emit its flowed blocks as
                    // an inline `TextBox` segment in document order — whether or not it
                    // sits inside an open `w:pict` capture. VML text boxes render inline
                    // (their pre-VML-paint behavior), NOT as floats at their absolute
                    // `v:shape` box: those box positions overlap each other and the body
                    // text on real documents (the SDS content pages). The box's
                    // fill/border/position are intentionally dropped here — inline is the
                    // known-good, readable result. The enclosing `commit_pict` skips the
                    // matching `v:textbox` drawing so the box is not also painted as a
                    // float; positioned VML shapes and images still float.
                    self.push_segment(Segment::TextBox(TextBox {
                        id: frame.node_id,
                        anchor: None,
                        relative_height: None,
                        extent: None,
                        fill: None,
                        border: None,
                        blocks,
                    }));
                }
            }
            FrameKind::BlockSdt => {
                if blocks.is_empty() {
                    // An empty content control carries no block content; report and
                    // drop it (parallel to the empty-text-box path).
                    self.reporter.report(b"sdtContent");
                } else {
                    let block = BlockNode::Sdt(BlockSdt {
                        id: frame.node_id,
                        properties: frame.sdt_properties,
                        blocks,
                    });
                    // Route into the enclosing open cell, if any; otherwise the
                    // body root — exactly like a finished paragraph or table.
                    if let Some(returned) = self.tables.push_block(block) {
                        self.blocks.push(returned);
                    }
                }
            }
        }
        Ok(())
    }

    /// Opens a note (`w:footnote`/`w:endnote`) as a block container. Separator and
    /// continuation-separator notes (a non-`normal` `w:type`) are presentation and
    /// are skipped (reported). A content note allocates its id in document order.
    fn open_note(&mut self, element: &BytesStart<'_>) -> Result<(), ImportError> {
        self.close_note()?;
        let is_content = attribute_value(element, b"type")
            .map(|kind| kind == "normal")
            .unwrap_or(true);
        if !is_content {
            self.skip_note = true;
            self.reporter.report(self.note_container.unwrap_or(b"note"));
            return Ok(());
        }
        self.skip_note = false;
        let source_id = attribute_value(element, b"id").unwrap_or_default();
        let node_id = self.next_id()?;
        // Comment attributes (absent on footnote/endnote elements -> None).
        let meta = CommentMeta {
            author: attribute_value(element, b"author").filter(|v| !v.is_empty() && v.len() <= 255),
            date: attribute_value(element, b"date").filter(|v| !v.is_empty() && v.len() <= 64),
            initials: attribute_value(element, b"initials")
                .filter(|v| !v.is_empty() && v.len() <= 255),
        };
        self.current_note = Some((source_id, node_id, meta));
        self.in_body = true;
        self.blocks.clear();
        Ok(())
    }

    /// Closes the open note, committing its block content keyed by source `w:id`.
    fn close_note(&mut self) -> Result<(), ImportError> {
        if !self.skip_note && self.current_note.is_some() {
            // Unwind open text boxes FIRST — each restores its enclosing
            // paragraph — then finish that paragraph so its content (and the text
            // box) is committed, not dropped.
            while !self.frames.is_empty() {
                self.exit_frame()?;
            }
            if self.paragraph_open {
                self.finish_paragraph()?;
            }
            // Commit any table left open by truncated markup so its content is not
            // stranded in the shared `TableStack`, and so it cannot bleed into the
            // next note/comment parsed by this reused parser.
            let roots = self.tables.flush_open(&mut *self.ids)?;
            self.blocks.extend(roots);
        }
        // Clear residual table-suppression so the next note/comment starts clean.
        self.suppressed_tbl_depth = 0;
        // A bookmark never legitimately spans two notes/comments; clear the pairing
        // map defensively so an unclosed start cannot pair across containers.
        self.bookmark_ids.clear();
        // Content-control state never spans two notes/comments either; a
        // truncated block control (missing `</w:sdt>`) would otherwise leak a
        // `Block` scope and a non-zero depth into the next note parsed by this
        // reused parser, tightening its `MAX_SDT_DEPTH` and mis-scoping controls.
        self.sdt_depth = 0;
        self.sdt_scopes.clear();
        self.sdt_prop_depth = 0;
        self.pending_block_sdt_props.clear();
        self.in_body = false;
        if let Some((source_id, node_id, meta)) = self.current_note.take() {
            let blocks = std::mem::take(&mut self.blocks);
            self.notes.push((source_id, node_id, meta, blocks));
        }
        self.skip_note = false;
        Ok(())
    }

    /// Routes a segment into the innermost open wrapper as identified by the top
    /// of `wrapper_order`, else the paragraph. Hyperlinks, fields, and revisions
    /// nest in any order (a revision inside a hyperlink, a hyperlink inside a
    /// revision, revisions inside revisions), so the discriminator stack — not a
    /// fixed field-then-hyperlink precedence — determines which nests deepest.
    ///
    /// All display segments of an open field are captured into its cached result,
    /// including any that arrive before `separate` — a well-formed field has none
    /// there (the instruction is `w:instrText`, routed separately), so this only
    /// preserves malformed pre-`separate` display content instead of dropping it.
    fn push_segment(&mut self, segment: Segment) {
        match self.wrapper_order.last() {
            Some(WrapperKind::Field) => {
                if let Some(field) = self.field.as_mut() {
                    field.segments.push(segment);
                    return;
                }
            }
            Some(WrapperKind::Hyperlink) => {
                if let Some(hyperlink) = self.hyperlink.as_mut() {
                    hyperlink.segments.push(segment);
                    return;
                }
            }
            Some(WrapperKind::Revision) => {
                if let Some(revision) = self.revisions.last_mut() {
                    revision.segments.push(segment);
                    return;
                }
            }
            Some(WrapperKind::Sdt) => {
                if let Some(sdt) = self.sdts.last_mut() {
                    sdt.segments.push(segment);
                    return;
                }
            }
            None => {}
        }
        self.segments.push(segment);
    }

    /// Routes one `w:ffData` child element onto the open field's form builder.
    /// The payload containers (`w:textInput`/`w:checkBox`/`w:ddList`) set the
    /// builder's kind; their own children (default/type/size/entries/…) mutate
    /// that payload. Unmodeled children (`w:label`, `w:tabIndex`, a drop-down
    /// `w:default` index, a child in the wrong payload) are reported so nothing
    /// is dropped silently.
    fn ffdata_child(&mut self, local: &[u8], element: &BytesStart<'_>) {
        let mut unhandled = false;
        if let Some(field) = self.field.as_mut()
            && let Some(form) = field.form.as_mut()
        {
            match local {
                b"name" => form.name = ffdata_string(element),
                b"enabled" => {
                    form.enabled = Some(is_true(attribute_value(element, b"val").as_deref()));
                }
                b"calcOnExit" => {
                    form.calc_on_exit = Some(is_true(attribute_value(element, b"val").as_deref()));
                }
                b"helpText" => form.help_text = ffdata_string(element),
                b"statusText" => form.status_text = ffdata_string(element),
                b"entryMacro" => form.entry_macro = ffdata_string(element),
                b"exitMacro" => form.exit_macro = ffdata_string(element),
                b"textInput" => {
                    form.kind = Some(FormFieldKind::TextInput(FormTextInput::default()));
                }
                b"checkBox" => {
                    form.kind = Some(FormFieldKind::CheckBox(FormCheckBox::default()));
                }
                b"ddList" => {
                    form.kind = Some(FormFieldKind::DropDown(FormDropDown::default()));
                }
                b"type" => {
                    if let Some(FormFieldKind::TextInput(text)) = form.kind.as_mut() {
                        text.text_type = ffdata_text_type(element);
                    } else {
                        unhandled = true;
                    }
                }
                b"maxLength" => {
                    if let Some(FormFieldKind::TextInput(text)) = form.kind.as_mut() {
                        text.max_length = ffdata_u32(element);
                    } else {
                        unhandled = true;
                    }
                }
                b"format" => {
                    if let Some(FormFieldKind::TextInput(text)) = form.kind.as_mut() {
                        text.format = ffdata_string(element);
                    } else {
                        unhandled = true;
                    }
                }
                b"size" => {
                    if let Some(FormFieldKind::CheckBox(check)) = form.kind.as_mut() {
                        check.size = ffdata_u32(element).map(FormCheckBoxSize::Explicit);
                    } else {
                        unhandled = true;
                    }
                }
                b"sizeAuto" => {
                    if let Some(FormFieldKind::CheckBox(check)) = form.kind.as_mut() {
                        if is_true(attribute_value(element, b"val").as_deref()) {
                            check.size = Some(FormCheckBoxSize::Auto);
                        }
                    } else {
                        unhandled = true;
                    }
                }
                b"checked" => {
                    if let Some(FormFieldKind::CheckBox(check)) = form.kind.as_mut() {
                        check.checked = Some(is_true(attribute_value(element, b"val").as_deref()));
                    } else {
                        unhandled = true;
                    }
                }
                b"result" => {
                    if let Some(FormFieldKind::DropDown(list)) = form.kind.as_mut() {
                        list.result = ffdata_u32(element);
                    } else {
                        unhandled = true;
                    }
                }
                b"listEntry" => {
                    if let Some(FormFieldKind::DropDown(list)) = form.kind.as_mut() {
                        if let Some(value) = ffdata_string(element)
                            && list.entries.len() < MAX_FORM_FIELD_ENTRIES
                        {
                            list.entries.push(value);
                        } else {
                            unhandled = true;
                        }
                    } else {
                        unhandled = true;
                    }
                }
                // `w:default` is a text-input default string or a checkbox default
                // state; a drop-down default *index* is not modeled (reported).
                b"default" => match form.kind.as_mut() {
                    Some(FormFieldKind::TextInput(text)) => text.default = ffdata_string(element),
                    Some(FormFieldKind::CheckBox(check)) => {
                        check.default = Some(is_true(attribute_value(element, b"val").as_deref()));
                    }
                    _ => unhandled = true,
                },
                _ => unhandled = true,
            }
        }
        if unhandled {
            self.reporter.report(local);
        }
    }

    /// Opens a complex field on a `fldChar begin`. A field nested in another
    /// wrapper (field or hyperlink) is not modeled as structure; it is reported
    /// and its result content flattens into the enclosing wrapper.
    fn begin_field(&mut self) {
        self.field_depth += 1;
        if self.field_depth == 1 && self.field.is_none() && self.hyperlink.is_none() {
            self.field = Some(FieldAccumulator {
                instruction: String::new(),
                in_result: false,
                segments: Vec::new(),
                form: None,
            });
            self.wrapper_order.push(WrapperKind::Field);
        } else {
            self.reporter.report(b"fldChar");
        }
    }

    /// Opens a simple `w:fldSimple` field (instruction inline, result as children).
    fn open_simple_field(&mut self, element: &BytesStart<'_>) {
        self.field_depth += 1;
        if self.field_depth == 1 && self.field.is_none() && self.hyperlink.is_none() {
            let instruction = attribute_value(element, b"instr").unwrap_or_default();
            self.field = Some(FieldAccumulator {
                instruction,
                in_result: true,
                segments: Vec::new(),
                form: None,
            });
            self.wrapper_order.push(WrapperKind::Field);
        } else {
            self.reporter.report(b"fldSimple");
        }
    }

    /// Handles a `fldChar separate`: the outermost field switches to collecting
    /// its cached result.
    fn separate_field(&mut self) {
        if self.field_depth == 1
            && let Some(field) = self.field.as_mut()
        {
            field.in_result = true;
        }
    }

    /// Closes the outermost field on `fldChar end` or `</w:fldSimple>`, committing
    /// it; inner (nested) delimiters only balance the depth counter.
    fn close_field(&mut self) {
        if self.field_depth == 1 {
            self.commit_field();
        }
        self.field_depth = self.field_depth.saturating_sub(1);
    }

    /// Commits the open field. A valid instruction becomes a `Field` segment; an
    /// empty or over-long instruction is reported and the cached-result content is
    /// flattened into the enclosing stream so no display text is lost. The field's
    /// segment routes into the enclosing wrapper (an outer revision) or paragraph.
    fn commit_field(&mut self) {
        if let Some(field) = self.field.take() {
            // Drop the field's wrapper marker (it is the innermost) before routing
            // so its committed segment lands in the enclosing wrapper.
            self.pop_wrapper(WrapperKind::Field);
            if field.instruction.is_empty() || field.instruction.len() > MAX_FIELD_INSTRUCTION_BYTES
            {
                self.reporter.report(b"fldChar");
                for segment in field.segments {
                    self.push_segment(segment);
                }
            } else {
                let children = normalize_segments(field.segments);
                // Finalize an accumulated `w:ffData` block: it becomes form data
                // only once its payload kind (`w:textInput`/`w:checkBox`/`w:ddList`)
                // was seen; a payload-less `w:ffData` is reported so nothing is
                // lost silently and the field stays a plain field.
                let form = match field.form {
                    Some(builder) => match builder.kind {
                        Some(kind) => Some(FormFieldData {
                            name: builder.name,
                            enabled: builder.enabled,
                            calc_on_exit: builder.calc_on_exit,
                            help_text: builder.help_text,
                            status_text: builder.status_text,
                            entry_macro: builder.entry_macro,
                            exit_macro: builder.exit_macro,
                            kind,
                        }),
                        None => {
                            self.reporter.report(b"ffData");
                            None
                        }
                    },
                    None => None,
                };
                self.push_segment(Segment::Field {
                    instruction: field.instruction,
                    children,
                    form,
                });
            }
        }
    }

    /// Commits the open hyperlink, routing its segment into the enclosing wrapper
    /// (an outer revision) or the paragraph. An empty link is reported and dropped.
    fn commit_hyperlink(&mut self) {
        if let Some(accumulator) = self.hyperlink.take() {
            self.pop_wrapper(WrapperKind::Hyperlink);
            let children = normalize_segments(accumulator.segments);
            if children.is_empty() {
                self.reporter.report(b"hyperlink");
            } else {
                self.push_segment(Segment::Hyperlink {
                    target: accumulator.target,
                    tooltip: accumulator.tooltip,
                    children,
                });
            }
        }
    }

    /// Whether the innermost open wrapper is a tracked-change range.
    fn top_wrapper_is_revision(&self) -> bool {
        matches!(self.wrapper_order.last(), Some(WrapperKind::Revision))
    }

    /// Opens a tracked-change range (`w:ins`/`w:del`). The caller guarantees the
    /// nesting bound and paragraph-flow position; this pushes the accumulator and
    /// its wrapper marker.
    /// Begins a run-properties format-change (`w:rPrChange`). Routes to the run's
    /// own rPr accumulator (`run_properties`) when a run's rPr is open, else to the
    /// paragraph-mark's rPr accumulator (`mark_run_properties`).
    fn begin_run_prop_change(&mut self, element: &BytesStart<'_>) {
        let meta = PropChangeMeta::from_element(element);
        // `rpr_depth > 0` means a run's own rPr is open; otherwise the open rPr is
        // the paragraph mark's (the arm guard guarantees one of the two).
        let mark = self.rpr_depth == 0;
        let saved = if mark {
            std::mem::take(&mut self.mark_run_properties)
        } else {
            std::mem::take(&mut self.run_properties)
        };
        self.prop_change
            .push(PropChangeCapture::Run { meta, saved, mark });
    }

    /// Whether the innermost open property-change capture matches the closing
    /// `w:*PrChange` tag, so a mismatched/orphan close (malformed input) is left
    /// for the skip catch rather than finishing the wrong capture.
    fn prop_change_top_matches(&self, local: &[u8]) -> bool {
        matches!(
            (local, self.prop_change.last()),
            (b"rPrChange", Some(PropChangeCapture::Run { .. }))
                | (b"pPrChange", Some(PropChangeCapture::Paragraph { .. }))
                | (b"tblPrChange", Some(PropChangeCapture::Table { .. }))
                | (b"trPrChange", Some(PropChangeCapture::Row { .. }))
                | (b"tcPrChange", Some(PropChangeCapture::Cell { .. }))
                | (b"tblGridChange", Some(PropChangeCapture::Grid { .. }))
        )
    }

    /// Finishes the innermost open property-change capture (`w:*PrChange`): the
    /// prior snapshot has accumulated into the live accumulator, so take it out,
    /// restore the saved current properties, and attach the built change to them.
    fn finish_prop_change(&mut self) {
        let Some(capture) = self.prop_change.pop() else {
            return;
        };
        match capture {
            PropChangeCapture::Run { meta, saved, mark } => {
                let slot = if mark {
                    &mut self.mark_run_properties
                } else {
                    &mut self.run_properties
                };
                let prior = std::mem::replace(slot, saved);
                slot.prop_change = Some(meta.into_change(prior));
            }
            PropChangeCapture::Paragraph { meta, saved } => {
                let prior = std::mem::replace(&mut self.paragraph_properties, saved);
                self.paragraph_properties.prop_change = Some(meta.into_change(prior));
            }
            PropChangeCapture::Table { meta, saved } => {
                if let Some(prior) = self.tables.take_table_properties() {
                    let mut restored = saved;
                    restored.prop_change = Some(meta.into_change(prior));
                    self.tables.set_table_properties(restored);
                }
            }
            PropChangeCapture::Row { meta, saved } => {
                if let Some(prior) = self.tables.take_row_properties() {
                    let mut restored = saved;
                    restored.prop_change = Some(meta.into_change(prior));
                    self.tables.set_row_properties(restored);
                }
            }
            PropChangeCapture::Cell { meta, saved } => {
                if let Some(prior) = self.tables.take_cell_properties() {
                    let mut restored = saved;
                    restored.prop_change = Some(meta.into_change(prior));
                    self.tables.set_cell_properties(restored);
                }
            }
            PropChangeCapture::Grid { meta, saved } => {
                if let Some(prior) = self.tables.take_grid() {
                    self.tables.set_grid(saved);
                    self.tables.set_grid_change(meta.into_change(prior));
                }
            }
        }
    }

    fn open_revision(&mut self, local: &[u8], element: &BytesStart<'_>) {
        let kind = match local {
            b"ins" => RevisionKind::Insertion,
            b"del" => RevisionKind::Deletion,
            b"moveFrom" => RevisionKind::MoveFrom,
            // The caller only routes `ins`/`del`/`moveFrom`/`moveTo` here.
            _ => RevisionKind::MoveTo,
        };
        self.revisions.push(RevisionAccumulator {
            kind,
            author: attribute_value(element, b"author")
                .filter(|value| !value.is_empty() && value.len() <= 255),
            date: attribute_value(element, b"date")
                .filter(|value| !value.is_empty() && value.len() <= 64),
            revision_id: attribute_value(element, b"id")
                .filter(|value| !value.is_empty() && value.len() <= 64),
            segments: Vec::new(),
        });
        self.wrapper_order.push(WrapperKind::Revision);
    }

    /// Commits the innermost open tracked-change range, routing its segment into
    /// the enclosing wrapper (an outer revision/hyperlink) or the paragraph. An
    /// empty range is reported and dropped.
    fn commit_revision(&mut self) {
        if let Some(accumulator) = self.revisions.pop() {
            self.pop_wrapper(WrapperKind::Revision);
            let children = normalize_segments(accumulator.segments);
            if children.is_empty() {
                self.reporter.report(match accumulator.kind {
                    RevisionKind::Insertion => b"ins",
                    RevisionKind::Deletion => b"del",
                    RevisionKind::MoveFrom => b"moveFrom",
                    RevisionKind::MoveTo => b"moveTo",
                });
            } else {
                self.push_segment(Segment::Revision {
                    kind: accumulator.kind,
                    author: accumulator.author,
                    date: accumulator.date,
                    revision_id: accumulator.revision_id,
                    children,
                });
            }
        }
    }

    /// Pops the innermost wrapper marker, asserting it matches the wrapper being
    /// committed (the three wrappers close in strict XML nesting order, so the
    /// innermost marker is always the one being committed).
    fn pop_wrapper(&mut self, expected: WrapperKind) {
        debug_assert_eq!(self.wrapper_order.last(), Some(&expected));
        if self.wrapper_order.last() == Some(&expected) {
            self.wrapper_order.pop();
        }
    }

    /// Commits the innermost open wrapper (whichever kind), used to drain wrappers
    /// left open by malformed input at paragraph end.
    fn commit_top_wrapper(&mut self) {
        match self.wrapper_order.last() {
            Some(WrapperKind::Field) => self.commit_field(),
            Some(WrapperKind::Hyperlink) => self.commit_hyperlink(),
            Some(WrapperKind::Revision) => self.commit_revision(),
            Some(WrapperKind::Sdt) => self.commit_sdt(),
            None => {}
        }
    }

    /// Commits the innermost open inline content control, routing its segment into
    /// the enclosing wrapper (an outer hyperlink/field/revision/sdt) or the
    /// paragraph. An empty control is reported and dropped (like an empty revision).
    fn commit_sdt(&mut self) {
        if let Some(accumulator) = self.sdts.pop() {
            self.pop_wrapper(WrapperKind::Sdt);
            let children = normalize_segments(accumulator.segments);
            if children.is_empty() {
                self.reporter.report(b"sdt");
            } else {
                self.push_segment(Segment::Sdt {
                    properties: accumulator.properties,
                    children,
                });
            }
        }
    }

    /// Decides the scope of a `w:sdt` from the surrounding parser state, exactly as
    /// `p`/`r`/`tbl` positions are decided. Over `MAX_SDT_DEPTH`, or in any position
    /// that is neither run-flow nor block-flow (a run interior, or a table's
    /// row/cell-structural gap), the control is a reported passthrough.
    fn decide_sdt_scope(&self) -> SdtScope {
        if self.sdt_depth >= MAX_SDT_DEPTH {
            return SdtScope::Passthrough;
        }
        // Run position: inside a paragraph, not inside a run, drawing, picture, or
        // a sdt property subtree.
        if self.paragraph_open
            && !self.run_open
            && self.sdt_prop_depth == 0
            && self.drawing_depth == 0
            && self.pict_depth == 0
        {
            return SdtScope::Inline;
        }
        // Block position: where a paragraph/table goes — no open paragraph/run or
        // property context, and either no table is active or a cell is open (not a
        // structural gap between rows/cells).
        if self.in_body
            && !self.paragraph_open
            && !self.run_open
            && self.ppr_depth == 0
            && self.rpr_depth == 0
            && self.sdt_prop_depth == 0
            && (!self.tables.is_active() || self.tables.in_cell())
        {
            return SdtScope::Block;
        }
        SdtScope::Passthrough
    }

    /// Mutable access to the innermost open control's properties, routed by scope.
    /// Yields `None` for a passthrough control (its properties are discarded).
    fn current_sdt_properties(&mut self) -> Option<&mut SdtProperties> {
        match self.sdt_scopes.last()? {
            SdtScope::Inline => self
                .sdts
                .last_mut()
                .map(|accumulator| &mut accumulator.properties),
            SdtScope::Block => self.pending_block_sdt_props.last_mut(),
            SdtScope::Passthrough => None,
        }
    }

    /// The control-specific `data` of the current open content control, if it was
    /// opened by a combo/dropdown, date, or checkbox type marker.
    fn current_sdt_data(&mut self) -> Option<&mut SdtControlData> {
        self.current_sdt_properties()?.data.as_mut()
    }

    /// Reads a `w:sdtPr` marker's bounded `@w:val`. A value that is present but out
    /// of domain (empty or too long) is reported and dropped rather than mapped.
    fn sdt_bounded_value(
        &mut self,
        element: &BytesStart<'_>,
        local: &[u8],
        max: usize,
    ) -> Option<String> {
        match attribute_value(element, b"val") {
            Some(value) if !value.is_empty() && value.len() <= max => Some(value),
            Some(_) => {
                self.reporter.report(local);
                None
            }
            None => None,
        }
    }

    /// Resolves a `w:hyperlink`'s target: an external URL through the
    /// relationship graph (`r:id`) or an internal bookmark (`w:anchor`).
    /// Returns `None` (report + flatten) when neither resolves in domain.
    fn resolve_hyperlink_target(
        &self,
        element: &BytesStart<'_>,
    ) -> Option<(HyperlinkTarget, Option<String>)> {
        let tooltip = attribute_value(element, b"tooltip")
            .filter(|value| !value.is_empty() && value.len() <= 255);
        if let Some(relationship_id) = attribute_value(element, b"id") {
            let url = self.hyperlink_rels.get(&relationship_id)?;
            if url.is_empty() || url.len() > 2048 {
                return None;
            }
            return Some((
                HyperlinkTarget::External(ExternalTarget { url: url.clone() }),
                tooltip,
            ));
        }
        let anchor = attribute_value(element, b"anchor")?;
        if anchor.is_empty() || anchor.len() > 255 {
            return None;
        }
        Some((
            HyperlinkTarget::Internal(InternalTarget { anchor }),
            tooltip,
        ))
    }

    fn finish_paragraph(&mut self) -> Result<(), ImportError> {
        self.paragraph_open = false;
        self.ppr_depth = 0;
        self.run_open = false;
        // Robustness: a `w:p` that closes with wrappers still open (a malformed
        // hyperlink/field/revision missing its close) is drained innermost-first
        // so each commits its accumulated content into the next-enclosing wrapper
        // (or the paragraph) — nothing is dropped and nesting order is preserved.
        while !self.wrapper_order.is_empty() {
            let before = self.wrapper_order.len();
            self.commit_top_wrapper();
            if self.wrapper_order.len() == before {
                // Defensive: a marker with no live accumulator would not pop;
                // drop it so the drain always terminates.
                self.wrapper_order.pop();
            }
        }
        self.hyperlink_depth = 0;
        self.field_depth = 0;
        // An inline content control never spans a paragraph. The drain above
        // committed any left open by malformed input via `commit_top_wrapper`; now
        // clear their scope entries and the shared `sdt_depth` path counter so an
        // unclosed inline `w:sdt` cannot desync a later paragraph (a stray
        // `</w:sdt>` then finds no scope and is inert). Every scope entry present
        // at paragraph end is necessarily inline — a block control opens only when
        // no paragraph is open.
        while matches!(self.sdt_scopes.last(), Some(SdtScope::Inline)) {
            self.sdt_scopes.pop();
            self.sdt_depth = self.sdt_depth.saturating_sub(1);
        }
        // A revision never spans a paragraph; reset the suppression counter so a
        // malformed unbalanced property-context marker cannot leak into the next.
        self.suppressed_revision_depth = 0;
        self.in_instr = false;
        self.instr_buffer.clear();
        self.drawing_depth = 0;
        self.blipfill_depth = 0;
        // Parity with the drawing counters: reset the VML picture depth too, so a
        // pict left open across a paragraph flush cannot leak and defeat the next
        // picture's `imagedata` guard.
        self.pict_depth = 0;
        // A ruby annotation never spans a paragraph; reset defensively so a
        // malformed unclosed `w:rt` cannot suppress a later paragraph's text.
        self.ruby_annotation_depth = 0;
        // Paragraph-property containers never span a paragraph; reset defensively
        // so a malformed unclosed `w:pBdr`/`w:tabs`/mark-`w:rPr` cannot leak.
        self.in_tabs = false;
        self.mark_rpr_depth = 0;
        // A paragraph-mark `w:rPr` (even present-but-empty) is preserved as the
        // paragraph's `mark_run`; `Some` records the mark's own formatting.
        if std::mem::take(&mut self.mark_rpr_seen) {
            self.paragraph_properties.mark_run =
                Some(Box::new(std::mem::take(&mut self.mark_run_properties)));
        }
        if self.edge_scope == EdgeScope::ParagraphBorders {
            self.edge_scope = EdgeScope::None;
        }
        let paragraph_id = self
            .paragraph_id
            .take()
            .expect("paragraph id was allocated");
        let normalized = normalize_segments(std::mem::take(&mut self.segments));
        let mut inlines = Vec::with_capacity(normalized.len());
        for segment in normalized {
            inlines.push(self.segment_to_inline(segment)?);
        }
        let block = BlockNode::Paragraph(Paragraph {
            id: paragraph_id,
            properties: std::mem::take(&mut self.paragraph_properties),
            inlines,
        });
        // Route into the open table cell, if any; otherwise the body root.
        if let Some(returned) = self.tables.push_block(block) {
            self.blocks.push(returned);
        }
        Ok(())
    }

    /// Assigns ids in document order (an opening tag before its children) and
    /// builds the inline node. A hyperlink's own id precedes its children's.
    fn segment_to_inline(&mut self, segment: Segment) -> Result<InlineNode, ImportError> {
        match segment {
            Segment::Run { properties, text } => {
                let id = self.next_id()?;
                Ok(InlineNode::Run(Run {
                    id,
                    properties,
                    text,
                }))
            }
            Segment::Tab => {
                let id = self.next_id()?;
                Ok(InlineNode::Tab(Tab { id }))
            }
            Segment::Break(kind) => {
                let id = self.next_id()?;
                Ok(InlineNode::Break(Break { id, kind }))
            }
            Segment::Drawing { media, extent } => {
                let id = self.next_id()?;
                Ok(InlineNode::Drawing(Drawing { id, media, extent }))
            }
            Segment::AnchoredDrawing {
                media,
                extent,
                anchor,
                descr,
                relative_height,
            } => {
                let id = self.next_id()?;
                Ok(InlineNode::AnchoredDrawing(AnchoredDrawing {
                    id,
                    media,
                    extent,
                    anchor,
                    descr,
                    relative_height,
                }))
            }
            Segment::EmbeddedObject {
                kind,
                part,
                extra_parts,
                preview,
                extent,
                prog_id,
            } => {
                let id = self.next_id()?;
                Ok(InlineNode::EmbeddedObject(EmbeddedObject {
                    id,
                    kind,
                    part,
                    extra_parts,
                    preview,
                    extent,
                    prog_id,
                }))
            }
            Segment::Hyperlink {
                target,
                tooltip,
                children,
            } => {
                let id = self.next_id()?;
                let mut inlines = Vec::with_capacity(children.len());
                for child in children {
                    inlines.push(self.segment_to_inline(child)?);
                }
                Ok(InlineNode::Hyperlink(Hyperlink {
                    id,
                    target,
                    tooltip,
                    inlines,
                }))
            }
            Segment::Field {
                instruction,
                children,
                form,
            } => {
                let id = self.next_id()?;
                let mut inlines = Vec::with_capacity(children.len());
                for child in children {
                    inlines.push(self.segment_to_inline(child)?);
                }
                Ok(InlineNode::Field(Field {
                    id,
                    instruction,
                    inlines,
                    form,
                }))
            }
            Segment::Math { omml, text } => {
                let id = self.next_id()?;
                Ok(InlineNode::Math(Math { id, omml, text }))
            }
            Segment::Symbol { font, char } => {
                let id = self.next_id()?;
                Ok(InlineNode::Symbol(Symbol { id, font, char }))
            }
            Segment::NoBreakHyphen => {
                let id = self.next_id()?;
                Ok(InlineNode::NoBreakHyphen(NoBreakHyphen { id }))
            }
            Segment::SoftHyphen => {
                let id = self.next_id()?;
                Ok(InlineNode::SoftHyphen(SoftHyphen { id }))
            }
            Segment::PositionalTab {
                alignment,
                relative_to,
                leader,
            } => {
                let id = self.next_id()?;
                Ok(InlineNode::PositionalTab(PositionalTab {
                    id,
                    alignment,
                    relative_to,
                    leader,
                }))
            }
            // A text box is already fully built (id and inner ids allocated while
            // parsing its content), so it converts directly.
            Segment::TextBox(text_box) => Ok(InlineNode::TextBox(text_box)),
            Segment::Group(group) => Ok(InlineNode::Group(group)),
            Segment::NoteReference { kind, note } => {
                let id = self.next_id()?;
                Ok(InlineNode::NoteReference(NoteReference { id, kind, note }))
            }
            Segment::CommentReference { comment } => {
                let id = self.next_id()?;
                Ok(InlineNode::CommentReference(CommentReference {
                    id,
                    comment,
                }))
            }
            Segment::CommentRangeStart { comment } => {
                let id = self.next_id()?;
                Ok(InlineNode::CommentRangeStart(CommentRangeStart {
                    id,
                    comment,
                }))
            }
            Segment::CommentRangeEnd { comment } => {
                let id = self.next_id()?;
                Ok(InlineNode::CommentRangeEnd(CommentRangeEnd { id, comment }))
            }
            Segment::Revision {
                kind,
                author,
                date,
                revision_id,
                children,
            } => {
                // The revision's own id precedes its children's (document order).
                let id = self.next_id()?;
                let mut inlines = Vec::with_capacity(children.len());
                for child in children {
                    inlines.push(self.segment_to_inline(child)?);
                }
                Ok(InlineNode::Revision(Revision {
                    id,
                    kind,
                    author,
                    date,
                    revision_id,
                    inlines,
                }))
            }
            Segment::BookmarkStart { bookmark } => {
                let id = self.next_id()?;
                Ok(InlineNode::BookmarkStart(BookmarkStart { id, bookmark }))
            }
            Segment::BookmarkEnd { bookmark } => {
                let id = self.next_id()?;
                Ok(InlineNode::BookmarkEnd(BookmarkEnd { id, bookmark }))
            }
            Segment::MoveRangeStart {
                kind,
                move_id,
                name,
                author,
                date,
            } => {
                let id = self.next_id()?;
                Ok(InlineNode::MoveRangeStart(MoveRangeStart {
                    id,
                    kind,
                    move_id,
                    name,
                    author,
                    date,
                }))
            }
            Segment::MoveRangeEnd { kind, move_id } => {
                let id = self.next_id()?;
                Ok(InlineNode::MoveRangeEnd(MoveRangeEnd { id, kind, move_id }))
            }
            Segment::Sdt {
                properties,
                children,
            } => {
                // The control's own id precedes its children's (document order).
                let id = self.next_id()?;
                let mut inlines = Vec::with_capacity(children.len());
                for child in children {
                    inlines.push(self.segment_to_inline(child)?);
                }
                Ok(InlineNode::Sdt(InlineSdt {
                    id,
                    properties,
                    inlines,
                }))
            }
        }
    }
}

/// Maps a recognized `w:sdtPr` type-marker element name to a control kind. The
/// caller guarantees `local` is one of the mapped markers (the building-block
/// gallery forms `w:docPartObj`/`w:docPartList` are handled separately).
/// Maps a `w:lock@w:val` token to its `SdtLock`; an unknown token yields `None`.
fn sdt_lock(value: &str) -> Option<SdtLock> {
    Some(match value {
        "unlocked" => SdtLock::Unlocked,
        "sdtLocked" => SdtLock::SdtLocked,
        "contentLocked" => SdtLock::ContentLocked,
        "sdtContentLocked" => SdtLock::SdtContentLocked,
        _ => return None,
    })
}

/// Reads a checkbox state glyph (`w14:checkedState` / `w14:uncheckedState`): a
/// bounded `w14:val` code point in an optional `w14:font`. A missing/out-of-bound
/// `val` drops the glyph.
fn sdt_checkbox_symbol(element: &BytesStart<'_>) -> Option<SdtCheckboxSymbol> {
    let val = attribute_value(element, b"val").filter(|v| !v.is_empty() && v.len() <= 8)?;
    let font = attribute_value(element, b"font").filter(|v| !v.is_empty() && v.len() <= 64);
    Some(SdtCheckboxSymbol { val, font })
}

fn sdt_control_kind(local: &[u8]) -> Option<SdtControlKind> {
    Some(match local {
        b"richText" => SdtControlKind::RichText,
        b"text" => SdtControlKind::PlainText,
        b"comboBox" => SdtControlKind::ComboBox,
        b"dropDownList" => SdtControlKind::DropDownList,
        b"date" => SdtControlKind::Date,
        b"picture" => SdtControlKind::Picture,
        b"checkbox" => SdtControlKind::Checkbox,
        b"group" => SdtControlKind::Group,
        b"repeatingSection" => SdtControlKind::RepeatingSection,
        b"citation" => SdtControlKind::Citation,
        b"bibliography" => SdtControlKind::Bibliography,
        _ => return None,
    })
}

/// Maps a `w:type` on a header/footer reference to its page kind (`default`
/// otherwise).
fn header_footer_kind(kind: Option<&str>) -> HeaderFooterKind {
    match kind {
        Some("first") => HeaderFooterKind::First,
        Some("even") => HeaderFooterKind::Even,
        _ => HeaderFooterKind::Default,
    }
}

fn attr_i32(element: &BytesStart<'_>, name: &[u8]) -> Option<i32> {
    attribute_value(element, name).and_then(|value| value.parse().ok())
}

/// Maps a table `w:jc@val` to an `Alignment`. Only the horizontal placements
/// meaningful for a table are accepted; `both`/`distribute` (justify) and any
/// unknown token yield `None` so the caller reports them.
fn table_alignment(element: &BytesStart<'_>) -> Option<Alignment> {
    match attribute_value(element, b"val").as_deref() {
        Some("start" | "left") => Some(Alignment::Start),
        Some("center") => Some(Alignment::Center),
        Some("end" | "right") => Some(Alignment::End),
        _ => None,
    }
}

fn attr_i64(element: &BytesStart<'_>, name: &[u8]) -> Option<i64> {
    attribute_value(element, name).and_then(|value| value.parse().ok())
}

/// Parses a `w:tblpPr` (`CT_TblPPr`) into a [`TableFloatPosition`]. Unrecognized
/// anchor/spec tokens drop the individual attribute (leaving it `None`); signed
/// offsets clamp to `-31_680..=31_680` and unsigned from-text distances to
/// `0..=31_680`, matching the twip bounds the other table properties enforce.
fn table_float_position(element: &BytesStart<'_>) -> TableFloatPosition {
    let anchor = |name: &[u8]| match attribute_value(element, name).as_deref() {
        Some("text") => Some(TableAnchor::Text),
        Some("margin") => Some(TableAnchor::Margin),
        Some("page") => Some(TableAnchor::Page),
        _ => None,
    };
    let x_spec = match attribute_value(element, b"tblpXSpec").as_deref() {
        Some("left") => Some(TableXAlign::Left),
        Some("center") => Some(TableXAlign::Center),
        Some("right") => Some(TableXAlign::Right),
        Some("inside") => Some(TableXAlign::Inside),
        Some("outside") => Some(TableXAlign::Outside),
        _ => None,
    };
    let y_spec = match attribute_value(element, b"tblpYSpec").as_deref() {
        Some("inline") => Some(TableYAlign::Inline),
        Some("top") => Some(TableYAlign::Top),
        Some("center") => Some(TableYAlign::Center),
        Some("bottom") => Some(TableYAlign::Bottom),
        Some("inside") => Some(TableYAlign::Inside),
        Some("outside") => Some(TableYAlign::Outside),
        _ => None,
    };
    let signed = |name: &[u8]| attr_i32(element, name).map(|v| v.clamp(-31_680, 31_680));
    let from_text = |name: &[u8]| attr_i32(element, name).map(|v| v.clamp(0, 31_680));
    TableFloatPosition {
        horz_anchor: anchor(b"horzAnchor"),
        vert_anchor: anchor(b"vertAnchor"),
        tbl_px_twips: signed(b"tblpX"),
        tbl_py_twips: signed(b"tblpY"),
        x_spec,
        y_spec,
        left_from_text_twips: from_text(b"leftFromText"),
        right_from_text_twips: from_text(b"rightFromText"),
        top_from_text_twips: from_text(b"topFromText"),
        bottom_from_text_twips: from_text(b"bottomFromText"),
    }
}

/// Parses a `w:cnfStyle` (`CT_Cnf`) selector. Word writes the whole selector as
/// the 12-bit `@w:val` binary string (`firstRow`, `lastRow`, `firstColumn`,
/// `lastColumn`, `oddVBand`, `evenVBand`, `oddHBand`, `evenHBand`, then the four
/// corner cells in NW/NE/SW/SE order); the equivalent explicit boolean
/// attributes are also accepted and OR in on top. Unset or malformed input
/// yields an all-false selector, which the caller drops.
fn parse_cnf_style(element: &BytesStart<'_>) -> CnfStyle {
    let mut cnf = CnfStyle::default();
    // The twelve flags, paired with their `CT_Cnf` attribute names, in the same
    // order as the `@w:val` bit string. Disjoint fields, so one array of `&mut`.
    let flags: [(&[u8], &mut bool); 12] = [
        (b"firstRow", &mut cnf.first_row),
        (b"lastRow", &mut cnf.last_row),
        (b"firstColumn", &mut cnf.first_column),
        (b"lastColumn", &mut cnf.last_column),
        (b"oddVBand", &mut cnf.odd_v_band),
        (b"evenVBand", &mut cnf.even_v_band),
        (b"oddHBand", &mut cnf.odd_h_band),
        (b"evenHBand", &mut cnf.even_h_band),
        (b"firstRowFirstColumn", &mut cnf.first_row_first_column),
        (b"firstRowLastColumn", &mut cnf.first_row_last_column),
        (b"lastRowFirstColumn", &mut cnf.last_row_first_column),
        (b"lastRowLastColumn", &mut cnf.last_row_last_column),
    ];
    // `@w:val` is the canonical form Word writes: a fixed 12-character binary
    // string, one digit per flag in the array order above. Anything but exactly
    // twelve characters is ignored; the explicit boolean attributes (accepted
    // too) then OR in on top so a mixed encoding is never lost.
    let bits: Option<Vec<bool>> = attribute_value(element, b"val")
        .map(|val| val.chars().map(|c| c == '1').collect())
        .filter(|bits: &Vec<bool>| bits.len() == 12);
    for (index, (attr, slot)) in flags.into_iter().enumerate() {
        if let Some(bits) = &bits {
            *slot = bits[index];
        }
        if let Some(value) = attribute_value(element, attr) {
            *slot = is_true(Some(&value));
        }
    }
    cnf
}

/// Whether a local element name is known DrawingML scaffolding for an embedded
/// picture (consumed silently while inside a `w:drawing`). Anything not listed
/// still reports, so genuinely unmodeled drawing content is never lost.
/// Whether a local element name is an OMML equation root (`m:oMath` or
/// `m:oMathPara`). Matched on local name because these names are unique to the
/// math namespace — no `w:` element shares them — and the retained-subtree
/// capture then swallows every inner `m:` element, so a per-prefix namespace
/// lookup is unnecessary. `m:oMathPara` wraps `m:oMath`, so detecting the
/// outermost root first retains the whole equation as one node.
fn is_math_root(local: &[u8]) -> bool {
    matches!(local, b"oMath" | b"oMathPara")
}

/// Reads a bounded `w:val` string from a `w:ffData` child (empty is permitted; an
/// over-long value is dropped so the string bound always holds).
fn ffdata_string(element: &BytesStart<'_>) -> Option<String> {
    attribute_value(element, b"val").filter(|value| value.len() <= MAX_FORM_FIELD_STRING_BYTES)
}

/// Reads a `w:val` unsigned integer from a `w:ffData` child.
fn ffdata_u32(element: &BytesStart<'_>) -> Option<u32> {
    attribute_value(element, b"val").and_then(|value| value.parse().ok())
}

/// Maps a `w:textInput/w:type@w:val` token (`ST_FFTextType`) to a `FormTextType`.
/// An unknown token yields `None` (the type is left unset).
fn ffdata_text_type(element: &BytesStart<'_>) -> Option<FormTextType> {
    match attribute_value(element, b"val").as_deref() {
        Some("regular") => Some(FormTextType::Regular),
        Some("number") => Some(FormTextType::Number),
        Some("date") => Some(FormTextType::Date),
        Some("currentTime") => Some(FormTextType::CurrentTime),
        Some("currentDate") => Some(FormTextType::CurrentDate),
        Some("calculated") => Some(FormTextType::Calculation),
        _ => None,
    }
}

/// Maps a `wp:positionH@relativeFrom` value to its horizontal reference. An
/// unknown/absent value yields `None` (resolved to the `column` default).
fn horizontal_anchor(value: Option<&str>) -> Option<HorizontalAnchor> {
    Some(match value? {
        "page" => HorizontalAnchor::Page,
        "margin" => HorizontalAnchor::Margin,
        "column" => HorizontalAnchor::Column,
        "character" => HorizontalAnchor::Character,
        "leftMargin" => HorizontalAnchor::LeftMargin,
        "rightMargin" => HorizontalAnchor::RightMargin,
        "insideMargin" => HorizontalAnchor::InsideMargin,
        "outsideMargin" => HorizontalAnchor::OutsideMargin,
        _ => return None,
    })
}

/// Maps a `wp:positionV@relativeFrom` value to its vertical reference. An
/// unknown/absent value yields `None` (resolved to the `paragraph` default).
fn vertical_anchor(value: Option<&str>) -> Option<VerticalAnchor> {
    Some(match value? {
        "page" => VerticalAnchor::Page,
        "margin" => VerticalAnchor::Margin,
        "paragraph" => VerticalAnchor::Paragraph,
        "line" => VerticalAnchor::Line,
        "topMargin" => VerticalAnchor::TopMargin,
        "bottomMargin" => VerticalAnchor::BottomMargin,
        "insideMargin" => VerticalAnchor::InsideMargin,
        "outsideMargin" => VerticalAnchor::OutsideMargin,
        _ => return None,
    })
}

/// Maps a horizontal `wp:align` keyword to its alignment.
fn horizontal_align(value: &str) -> Option<HorizontalAlign> {
    Some(match value {
        "left" => HorizontalAlign::Left,
        "center" => HorizontalAlign::Center,
        "right" => HorizontalAlign::Right,
        "inside" => HorizontalAlign::Inside,
        "outside" => HorizontalAlign::Outside,
        _ => return None,
    })
}

/// Maps a vertical `wp:align` keyword to its alignment.
fn vertical_align(value: &str) -> Option<VerticalAlign> {
    Some(match value {
        "top" => VerticalAlign::Top,
        "center" => VerticalAlign::Center,
        "bottom" => VerticalAlign::Bottom,
        "inside" => VerticalAlign::Inside,
        "outside" => VerticalAlign::Outside,
        _ => return None,
    })
}

/// Maps a `wp:wrap*` element's local name to its wrap mode. Only the five wrap
/// elements reach this (the caller matches them), so the fallback is unreachable
/// in practice and defaults to `wrapNone`.
fn wrap_mode(local: &[u8]) -> WrapMode {
    match local {
        b"wrapSquare" => WrapMode::Square,
        b"wrapTight" => WrapMode::Tight,
        b"wrapThrough" => WrapMode::Through,
        b"wrapTopAndBottom" => WrapMode::TopAndBottom,
        _ => WrapMode::None,
    }
}

/// Resolves a document [`ColorScheme`] into the 12-slot RGBA palette DrawingML
/// `a:schemeClr` targets resolve against (mirrors the layout resolver so a shape
/// fill and a run color agree). Slot order matches [`ColorScheme`]'s fields.
fn resolve_palette(scheme: &ColorScheme) -> [[u8; 4]; 12] {
    [
        resolve_scheme_color(&scheme.dark1),
        resolve_scheme_color(&scheme.light1),
        resolve_scheme_color(&scheme.dark2),
        resolve_scheme_color(&scheme.light2),
        resolve_scheme_color(&scheme.accent1),
        resolve_scheme_color(&scheme.accent2),
        resolve_scheme_color(&scheme.accent3),
        resolve_scheme_color(&scheme.accent4),
        resolve_scheme_color(&scheme.accent5),
        resolve_scheme_color(&scheme.accent6),
        resolve_scheme_color(&scheme.hyperlink),
        resolve_scheme_color(&scheme.followed_hyperlink),
    ]
}

/// Resolves one scheme slot to opaque RGBA: an `a:srgbClr` is its RGB; an
/// `a:sysClr` uses its recorded `lastClr`, else the conventional value for the
/// named system color.
fn resolve_scheme_color(color: &SchemeColor) -> [u8; 4] {
    match color {
        SchemeColor::Srgb(rgb) => [rgb.r, rgb.g, rgb.b, 255],
        SchemeColor::System(sys) => match sys.last_color {
            Some(rgb) => [rgb.r, rgb.g, rgb.b, 255],
            None => match sys.value.as_str() {
                "window" | "background" | "btnFace" | "menu" | "3dLight" => [255, 255, 255, 255],
                _ => [0, 0, 0, 255],
            },
        },
    }
}

/// The [`resolve_palette`] index for a DrawingML `a:schemeClr@val` slot name. The
/// `bg1`/`tx1`/`bg2`/`tx2` aliases map to the light/dark slots (Word swaps
/// background↔dark per the color-map, but the default map — used here — is the
/// identity `bg1=lt1`, `tx1=dk1`). `phClr` (a style placeholder) has no fixed
/// slot and yields `None`.
fn scheme_slot_index(name: &str) -> Option<usize> {
    Some(match name {
        "dk1" | "tx1" => 0,
        "lt1" | "bg1" => 1,
        "dk2" | "tx2" => 2,
        "lt2" | "bg2" => 3,
        "accent1" => 4,
        "accent2" => 5,
        "accent3" => 6,
        "accent4" => 7,
        "accent5" => 8,
        "accent6" => 9,
        "hlink" => 10,
        "folHlink" => 11,
        _ => return None,
    })
}

/// Parses a DrawingML modifier `@val` (`ST_Percentage`, a per-100000 integer such
/// as `85000` = 85%, or a `"85%"` string) into a `0.0..` factor.
fn parse_percent(value: &str) -> Option<f32> {
    let value = value.trim();
    if let Some(pct) = value.strip_suffix('%') {
        pct.trim().parse::<f32>().ok().map(|v| v / 100.0)
    } else {
        value.parse::<f32>().ok().map(|v| v / 100_000.0)
    }
}

/// Folds a [`PendingColor`]'s luminance/tint/shade/alpha modifiers over its base
/// into a concrete [`Rgba`]. `lumMod` scales and `lumOff` offsets luminance
/// (applied channel-wise, exact for the grayscale bases these decorations use);
/// `tint` lightens toward white and `shade` darkens toward black; `alpha` sets
/// opacity.
fn fold_color(color: &PendingColor) -> Rgba {
    let mut rgb = [
        f32::from(color.base[0]),
        f32::from(color.base[1]),
        f32::from(color.base[2]),
    ];
    if let Some(m) = color.lum_mod {
        for c in &mut rgb {
            *c *= m;
        }
    }
    if let Some(o) = color.lum_off {
        for c in &mut rgb {
            *c += o * 255.0;
        }
    }
    if let Some(t) = color.tint {
        let t = t.clamp(0.0, 1.0);
        for c in &mut rgb {
            *c = *c * t + 255.0 * (1.0 - t);
        }
    }
    if let Some(s) = color.shade {
        let s = s.clamp(0.0, 1.0);
        for c in &mut rgb {
            *c *= s;
        }
    }
    let clamp = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    let a = color
        .alpha
        .map_or(color.base[3], |a| clamp(a.clamp(0.0, 1.0) * 255.0));
    Rgba {
        r: clamp(rgb[0]),
        g: clamp(rgb[1]),
        b: clamp(rgb[2]),
        a,
    }
}

fn is_drawing_scaffolding(local: &[u8]) -> bool {
    matches!(
        local,
        b"inline"
            | b"anchor"
            | b"simplePos"
            | b"positionH"
            | b"positionV"
            | b"posOffset"
            | b"align"
            | b"wrapNone"
            | b"wrapSquare"
            | b"wrapTight"
            | b"wrapThrough"
            | b"wrapTopAndBottom"
            | b"wrapPolygon"
            | b"start"
            | b"lineTo"
            | b"effectExtent"
            | b"docPr"
            | b"cNvGraphicFramePr"
            | b"graphicFrameLocks"
            | b"graphic"
            | b"graphicData"
            | b"pic"
            | b"nvPicPr"
            | b"cNvPr"
            | b"cNvPicPr"
            | b"picLocks"
            | b"hlinkClick"
            | b"spPr"
            | b"xfrm"
            | b"off"
            | b"ext"
            | b"prstGeom"
            | b"avLst"
            | b"custGeom"
            | b"ln"
            | b"noFill"
            | b"solidFill"
            | b"srgbClr"
            | b"stretch"
            | b"fillRect"
            | b"srcRect"
            | b"blipFill"
            | b"blip"
            | b"extLst"
            | b"svgBlip"
    )
}

/// Whether a local element name is known VML shape scaffolding inside a
/// `w:object` (consumed silently; the object is modeled as a first-class
/// reference). `imagedata` and `OLEObject` are handled by dedicated arms, not
/// here. Anything not listed still reports, so unmodeled content is never lost.
fn is_object_scaffolding(local: &[u8]) -> bool {
    matches!(
        local,
        b"shape"
            | b"shapetype"
            | b"stroke"
            | b"fill"
            | b"path"
            | b"formulas"
            | b"f"
            | b"lock"
            | b"shadow"
            | b"textpath"
            | b"handles"
            | b"h"
            | b"wrap"
    )
}

fn normalize_segments(segments: Vec<Segment>) -> Vec<Segment> {
    let mut normalized: Vec<Segment> = Vec::with_capacity(segments.len());
    for segment in segments {
        match segment {
            Segment::Run { text, .. } if text.is_empty() => {}
            Segment::Run { properties, text } => {
                if let Some(Segment::Run {
                    properties: previous_properties,
                    text: previous_text,
                }) = normalized.last_mut()
                    && *previous_properties == properties
                {
                    previous_text.push_str(&text);
                    continue;
                }
                normalized.push(Segment::Run { properties, text });
            }
            other => normalized.push(other),
        }
    }
    normalized
}

// --- VML → float-layer mapping helpers -------------------------------------

/// EMU per twip (`914400 EMU/in ÷ 1440 twips/in`).
const EMU_PER_TWIP: i64 = 635;

/// A twip length (non-negative extent) to EMU, clamped to the model's coordinate
/// bound.
fn twip_emu_len(twips: i64) -> i64 {
    twips.saturating_mul(EMU_PER_TWIP).clamp(0, MAX_EMU)
}

/// A signed twip offset (a float may overhang its reference edge) to EMU, clamped
/// to the model's signed coordinate bound.
fn twip_emu_offset(twips: i64) -> i64 {
    twips.saturating_mul(EMU_PER_TWIP).clamp(-MAX_EMU, MAX_EMU)
}

/// The EMU box of a VML shape (its width/height, defaulting a missing dimension to
/// zero — a horizon rule is a full-width, near-zero-height rectangle).
fn vml_extent(position: &VmlPosition) -> Extent {
    Extent {
        width_emu: twip_emu_len(position.width.unwrap_or(0)),
        height_emu: twip_emu_len(position.height.unwrap_or(0)),
    }
}

/// Whether a VML shape is a positioned float (an absolute left/top or an explicit
/// z-order) rather than a genuinely inline shape.
fn vml_is_floating(position: &VmlPosition) -> bool {
    position.left.is_some() || position.top.is_some() || position.z_index.is_some()
}

/// Resolves a VML box into a float-layer [`DrawingAnchor`] plus its z key, using
/// the shape's own left/top offsets.
fn vml_anchor(position: &VmlPosition) -> (DrawingAnchor, Option<u32>) {
    vml_anchor_at(
        position,
        position.left.unwrap_or(0),
        position.top.unwrap_or(0),
    )
}

/// [`vml_anchor`] with explicit left/top twip offsets (a `v:line` anchors at its
/// segment's bounding-box corner, which its style box does not carry).
fn vml_anchor_at(
    position: &VmlPosition,
    left_twips: i64,
    top_twips: i64,
) -> (DrawingAnchor, Option<u32>) {
    let anchor = DrawingAnchor {
        horizontal: AnchorHorizontal {
            relative_from: vml_h_anchor(position.h_relative),
            position: HorizontalPosition::Offset(twip_emu_offset(left_twips)),
        },
        vertical: AnchorVertical {
            relative_from: vml_v_anchor(position.v_relative),
            position: VerticalPosition::Offset(twip_emu_offset(top_twips)),
        },
        wrap: WrapMode::None,
        behind_doc: position.behind_doc(),
    };
    (anchor, position.z_index.map(vml_rel_height))
}

/// Maps a VML `z-index` (a signed 32-bit stacking order, negative meaning behind
/// the text) onto the float layer's monotonic `u32` `relativeHeight` key, order-
/// preservingly (so shapes keep their relative paint order within each band).
fn vml_rel_height(z_index: i32) -> u32 {
    (i64::from(z_index) - i64::from(i32::MIN)) as u32
}

/// Maps a VML horizontal reference frame onto the anchor's `relativeFrom`, falling
/// back to the text margin/column as the float layer does.
fn vml_h_anchor(frame: Option<VmlRelFrame>) -> HorizontalAnchor {
    match frame {
        Some(VmlRelFrame::Page) => HorizontalAnchor::Page,
        Some(VmlRelFrame::Margin) => HorizontalAnchor::Margin,
        Some(VmlRelFrame::Column) => HorizontalAnchor::Column,
        Some(VmlRelFrame::Char) => HorizontalAnchor::Character,
        // `text` is the content area (the text margin box); everything else
        // (line/paragraph/other/unset) falls back to the text margin, which the
        // float layer resolves identically to the column.
        _ => HorizontalAnchor::Margin,
    }
}

/// Maps a VML vertical reference frame onto the anchor's `relativeFrom`.
fn vml_v_anchor(frame: Option<VmlRelFrame>) -> VerticalAnchor {
    match frame {
        Some(VmlRelFrame::Page) => VerticalAnchor::Page,
        Some(VmlRelFrame::Margin) | Some(VmlRelFrame::Text) | Some(VmlRelFrame::Column) => {
            VerticalAnchor::Margin
        }
        Some(VmlRelFrame::Line) => VerticalAnchor::Line,
        // `paragraph` and everything else (char/other/unset) resolve against the
        // anchoring paragraph, the float layer's vertical default.
        _ => VerticalAnchor::Paragraph,
    }
}

/// The resolved fill of a VML shape: `None` when unfilled or when filled with no
/// declared color (a colorless fill is invisible over the page, so it is skipped
/// rather than defaulted).
fn vml_fill(fill: &VmlFill) -> Option<Rgba> {
    if fill.on {
        fill.color.map(vml_rgba)
    } else {
        None
    }
}

/// The resolved outline of a VML shape: `None` when unstroked; otherwise the
/// declared color (VML's default stroke is black) at the declared weight.
fn vml_stroke(stroke: &VmlStroke) -> Option<ShapeStroke> {
    if !stroke.on {
        return None;
    }
    Some(ShapeStroke {
        color: stroke.color.map(vml_rgba).unwrap_or(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }),
        width_emu: twip_emu_len(stroke.weight_twips.unwrap_or(0)),
    })
}

/// The bounding-box outline used to approximate a generic `v:shape` path (see
/// [`BodyParser::vml_shape_segment`]): a hairline stroke in the shape's fill color
/// (the frame color), falling back to its stroke color, then black.
fn vml_path_outline(fill: &VmlFill, stroke: &VmlStroke) -> ShapeStroke {
    let color = fill.color.or(stroke.color).map(vml_rgba).unwrap_or(Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    });
    // A `0`-EMU width paints as the renderer's 1px hairline, matching the thin
    // frame LibreOffice draws for these paths.
    ShapeStroke {
        color,
        width_emu: stroke.weight_twips.map(twip_emu_len).unwrap_or(0),
    }
}

/// A VML color to the model's RGBA.
fn vml_rgba(color: VmlColor) -> Rgba {
    Rgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    }
}
