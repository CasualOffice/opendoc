//! Main-document body parsing into v1 block nodes.

use std::collections::BTreeMap;

use casual_doc_model::v1::{
    Alignment, BlockNode, BlockSdt, Bookmark, BookmarkEnd, BookmarkId, BookmarkStart, BorderEdge,
    Break, BreakKind, CellVerticalAlignment, Comment, CommentId, CommentReference, DefinitionMap,
    DocGrid, DocGridType, Drawing, Extent, ExternalTarget, Field, HeaderFooterId, HeaderFooterKind,
    HeaderFooterRef, HeightRule, Hyperlink, HyperlinkTarget, InlineNode, InlineSdt, InternalTarget,
    MAX_EMU, MAX_FIELD_INSTRUCTION_BYTES, MAX_REVISION_DEPTH, MAX_SDT_DEPTH, MAX_TEXTBOX_DEPTH,
    MediaId, NoteId, NoteKind, NoteReference, PageMargins, PageNumbering, PageSize,
    PageVerticalAlignment, Paragraph, ParagraphProperties, Revision, RevisionKind, RgbColor, Run,
    RunProperties, SdtControlKind, SdtProperties, SectionBoundary, SectionColumns, SectionId,
    SectionType, StyleKind, Tab, TabAlignment, TabLeader, TabStop, TableLayout, TableOverlap,
    TextBox, TextDirection, VerticalMerge,
};
use casual_doc_model::{IdGenerator, NodeId};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::numbering::Numbering;
use crate::properties::{
    apply_paragraph_property, apply_run_property, attribute_value, break_kind, is_true, parse_rgb,
};
use crate::report::Reporter;
use crate::styles::Styles;
use crate::tables::TableStack;

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
    Hyperlink {
        target: HyperlinkTarget,
        tooltip: Option<String>,
        children: Vec<Segment>,
    },
    Field {
        instruction: String,
        children: Vec<Segment>,
    },
    /// A fully-built text box (its inner ids are already allocated).
    TextBox(TextBox),
    /// A reference to a footnote or endnote definition.
    NoteReference {
        kind: NoteKind,
        note: NoteId,
    },
    /// A reference to a comment definition.
    CommentReference {
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
    /// An inline-level content control (`w:sdt`) wrapping inline content.
    Sdt {
        properties: SdtProperties,
        children: Vec<Segment>,
    },
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
}

/// A tracked-change range (`w:ins`/`w:del`) being accumulated.
struct RevisionAccumulator {
    kind: RevisionKind,
    author: Option<String>,
    date: Option<String>,
    revision_id: Option<String>,
    segments: Vec<Segment>,
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
    columns: Option<u16>,
    column_space: Option<i32>,
    column_separator: Option<bool>,
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
    elements: u64,
    depth: u64,
    text_bytes: usize,
    in_document: bool,
    in_body: bool,
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
    hyperlink: Option<HyperlinkAccumulator>,
    hyperlink_depth: u32,
    /// The open field, if any (simple or complex). Mutually exclusive with an
    /// open hyperlink (a hyperlink and a field never open inside one another).
    field: Option<FieldAccumulator>,
    /// Nesting depth of `<w:fldSimple>` / complex `fldChar` fields, so a
    /// missing/extra delimiter cannot desynchronize field commits.
    field_depth: u32,
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
    /// Depth of an open property-change tracked revision (`w:*PrChange`), whose
    /// nested historical property container must not map over the current values.
    pr_change_depth: u32,
    /// Which table border/margin container is currently open (for edge routing).
    /// A well-formed edge container has no box content, but malformed markup can
    /// nest a `w:txbxContent` inside one; it is saved/restored across a text-box
    /// frame (like the depth counters) so an inner table's edge container cannot
    /// clobber the outer scope and drop the enclosing table's borders.
    edge_scope: EdgeScope,
    /// Whether a `w:tabs` container is open (its `w:tab` children are tab stops,
    /// not the inline run tab). Never spans a text box.
    in_tabs: bool,
    /// Depth of a paragraph-mark `w:rPr` (opened inside `w:pPr` with no run open):
    /// its children are the pilcrow's run properties, so a `w:shd` there is NOT
    /// paragraph shading. Never spans a text box.
    mark_rpr_depth: u32,
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
}

/// Resolution tables the body parser consults while mapping constructs.
pub(crate) struct ParseInputs<'a> {
    pub styles: &'a Styles,
    pub numbering: &'a Numbering,
    pub media_index: &'a BTreeMap<String, MediaId>,
    pub hyperlink_rels: &'a BTreeMap<String, String>,
    pub footnote_ids: &'a BTreeMap<String, NoteId>,
    pub endnote_ids: &'a BTreeMap<String, NoteId>,
    pub header_ids: &'a BTreeMap<String, HeaderFooterId>,
    pub footer_ids: &'a BTreeMap<String, HeaderFooterId>,
    pub comment_ids: &'a BTreeMap<String, CommentId>,
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
        BodyParser {
            ids,
            styles: inputs.styles,
            numbering: inputs.numbering,
            reporter,
            config,
            media_index: inputs.media_index,
            hyperlink_rels: inputs.hyperlink_rels,
            elements: 0,
            depth: 0,
            text_bytes: 0,
            in_document: false,
            in_body: false,
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
            hyperlink: None,
            hyperlink_depth: 0,
            field: None,
            field_depth: 0,
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
            edge_scope: EdgeScope::None,
            in_tabs: false,
            mark_rpr_depth: 0,
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
        }
    }
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
) -> Result<(Vec<BlockNode>, Vec<SectionBoundary>), ImportError> {
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
    Ok((parser.blocks, parser.sections))
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
    let inputs = ParseInputs {
        styles,
        numbering,
        media_index,
        hyperlink_rels,
        footnote_ids: &empty_notes,
        endnote_ids: &empty_notes,
        header_ids: &empty_hf,
        footer_ids: &empty_hf,
        comment_ids: &empty_comment,
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
    let inputs = ParseInputs {
        styles,
        numbering,
        media_index,
        hyperlink_rels,
        footnote_ids: &empty_notes,
        endnote_ids: &empty_notes,
        header_ids: &empty_hf,
        footer_ids: &empty_hf,
        comment_ids: &empty_comment,
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
    let inputs = ParseInputs {
        styles,
        numbering,
        media_index,
        hyperlink_rels,
        footnote_ids: &empty_notes,
        endnote_ids: &empty_notes,
        header_ids: &empty_hf,
        footer_ids: &empty_hf,
        comment_ids: &empty_comment,
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
            match event {
                Event::Eof => break,
                Event::DocType(_) => return Err(ImportError::MalformedXml),
                Event::Start(element) => {
                    self.depth += 1;
                    if self.depth > self.config.max_depth {
                        return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                    }
                    self.on_start(element.local_name().as_ref(), &element)?;
                }
                Event::Empty(element) => {
                    self.on_start(element.local_name().as_ref(), &element)?;
                    self.on_end(element.local_name().as_ref())?;
                }
                Event::End(element) => {
                    self.on_end(element.local_name().as_ref())?;
                    self.depth = self.depth.saturating_sub(1);
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
            // Property-change tracked revisions carry a nested copy of the
            // PREVIOUS property container (e.g. `w:tcPrChange > w:tcPr`). Report
            // the container and skip its entire subtree so the historical values
            // are never mapped over the current ones. The counter is incremented
            // here (before the skip catch) so nested changes still balance.
            b"pPrChange" | b"rPrChange" | b"tblPrChange" | b"trPrChange" | b"tcPrChange"
            | b"sectPrChange" | b"tblGridChange" | b"numberingChange"
                if self.in_document =>
            {
                self.pr_change_depth += 1;
                self.reporter.report(local);
            }
            // Inside a property-change revision: ignore every element (its
            // historical properties are reported via the container above).
            _ if self.pr_change_depth > 0 => {}
            b"document" => self.in_document = true,
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
            b"rPr" if self.ppr_depth > 0 && !self.run_open => self.mark_rpr_depth += 1,
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
            // A comment reference (inside a run) resolves to a comment id; the
            // comment-range markers (`commentRangeStart`/`End`) are not modeled
            // and fall through to the report arm.
            b"commentReference" if self.run_open => {
                match attribute_value(element, b"id")
                    .and_then(|id| self.comment_ids.get(&id).copied())
                {
                    Some(comment) => self.push_segment(Segment::CommentReference { comment }),
                    None => self.reporter.report(b"commentReference"),
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
            b"tab" if self.run_open => self.push_segment(Segment::Tab),
            b"br" if self.run_open => {
                let kind = break_kind(element);
                self.push_segment(Segment::Break(kind));
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
            // A tracked change (`w:ins`/`w:del`). Modeled as a run range ONLY when
            // it sits directly in paragraph flow (not inside a run, not inside a
            // property context) and within the nesting bound; every other position
            // — a paragraph-mark revision (`w:pPr>w:rPr>w:ins`), a run-property
            // revision marker (`w:r>w:rPr>w:ins`), an over-`MAX_REVISION_DEPTH`
            // range, or a row/cell revision outside a paragraph — is reported and
            // counted (`suppressed_revision_depth`) so its matching close balances
            // and never commits an enclosing real revision. The arm is
            // UNCONDITIONAL (both branches handle `ins`/`del`) so start and end
            // stay balanced, exactly like tables' `suppressed_tbl_depth`.
            b"ins" | b"del" => {
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
                }
            }
            b"extent" if self.drawing_depth > 0 => {
                if let (Some(cx), Some(cy)) = (attr_i64(element, b"cx"), attr_i64(element, b"cy")) {
                    if (0..=MAX_EMU).contains(&cx) && (0..=MAX_EMU).contains(&cy) {
                        self.pending_extent = Some(Extent {
                            width_emu: cx,
                            height_emu: cy,
                        });
                    }
                }
            }
            b"blipFill" if self.drawing_depth > 0 => self.blipfill_depth += 1,
            b"blip" if self.blipfill_depth > 0 && self.pending_embed.is_none() => {
                self.pending_embed = attribute_value(element, b"embed");
            }
            // A floating anchor, alt text, click-link, or SVG dual-blip carries
            // detail the model does not capture: flag it so a resolved drawing
            // is still reported (degraded), never silently under-modeled.
            b"anchor" if self.drawing_depth > 0 => self.drawing_extra = true,
            b"docPr" if self.drawing_depth > 0 => {
                if attribute_value(element, b"descr").is_some() {
                    self.drawing_extra = true;
                }
            }
            b"hlinkClick" | b"svgBlip" if self.drawing_depth > 0 => self.drawing_extra = true,
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
                if let Some(section) = self.section.as_mut() {
                    section.page_width = attr_i32(element, b"w");
                    section.page_height = attr_i32(element, b"h");
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
                }
            }
            b"cols" if self.section.is_some() => {
                let space = attr_i32(element, b"space");
                let separator = attribute_value(element, b"sep")
                    .as_deref()
                    .map(|value| is_true(Some(value)));
                if let Some(section) = self.section.as_mut() {
                    section.columns =
                        attribute_value(element, b"num").and_then(|value| value.parse().ok());
                    section.column_space = space;
                    section.column_separator = separator;
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
                {
                    if (1..=16_384).contains(&span) {
                        self.tables.set_grid_span(span);
                    }
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
            // Recognized `w:sdtPr` type markers set the control kind.
            b"richText" | b"text" | b"comboBox" | b"dropDownList" | b"date" | b"picture"
            | b"checkbox" | b"group" | b"repeatingSection" | b"citation" | b"bibliography"
                if self.sdt_prop_depth > 0 =>
            {
                let kind = sdt_control_kind(local);
                if let Some(properties) = self.current_sdt_properties() {
                    properties.control_kind = kind;
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
            // Close of a property-change revision container (balances the on_start
            // increment). Placed BEFORE the skip catch so it is not swallowed.
            b"pPrChange" | b"rPrChange" | b"tblPrChange" | b"trPrChange" | b"tcPrChange"
            | b"sectPrChange" | b"tblGridChange" | b"numberingChange"
                if self.pr_change_depth > 0 =>
            {
                self.pr_change_depth = self.pr_change_depth.saturating_sub(1);
            }
            // Inside a property-change revision: swallow every close so the nested
            // historical property containers never decrement the real depth
            // counters (they never incremented them — their opens were skipped).
            _ if self.pr_change_depth > 0 => {}
            b"document" => self.in_document = false,
            b"body" => self.in_body = false,
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
            b"blipFill" => self.blipfill_depth = self.blipfill_depth.saturating_sub(1),
            b"drawing" if self.drawing_depth > 0 => {
                self.drawing_depth -= 1;
                if self.drawing_depth == 0 {
                    self.commit_drawing();
                }
            }
            b"pict" if self.pict_depth > 0 => {
                self.pict_depth -= 1;
                if self.pict_depth == 0 {
                    self.commit_pict();
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
            b"ins" | b"del" if self.suppressed_revision_depth > 0 => {
                self.suppressed_revision_depth -= 1;
            }
            // A real tracked-change range closes: commit it into the enclosing
            // wrapper (an outer revision/hyperlink) or the paragraph.
            b"ins" | b"del" if self.top_wrapper_is_revision() => self.commit_revision(),
            b"tcPr" => self.tcpr_depth = self.tcpr_depth.saturating_sub(1),
            b"tblPr" => self.tblpr_depth = self.tblpr_depth.saturating_sub(1),
            b"trPr" => self.trpr_depth = self.trpr_depth.saturating_sub(1),
            b"tblBorders" | b"tcBorders" | b"tblCellMar" | b"tcMar" | b"pBdr" => {
                self.edge_scope = EdgeScope::None;
            }
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

    /// Commits the top-level drawing that just closed. A resolved embed becomes
    /// a `Drawing` segment; an unresolved/dangling embed is reported and
    /// dropped. A resolved drawing carrying unmodeled detail is also reported.
    fn commit_drawing(&mut self) {
        let extent = self.pending_extent.take();
        let extra = self.drawing_extra;
        match self.pending_embed.take() {
            Some(embed) => match self.media_index.get(&embed) {
                Some(media) => {
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

    /// Commits a legacy VML picture that just closed. A resolvable
    /// `v:imagedata@r:id` becomes a `Drawing` (no EMU extent — VML sizes in CSS,
    /// which the model does not capture); an unresolved id or an image-less shape
    /// (e.g. a VML text box, handled elsewhere) is reported.
    fn commit_pict(&mut self) {
        match self.pending_embed.take() {
            Some(id) => match self.media_index.get(&id) {
                Some(media) => self.push_segment(Segment::Drawing {
                    media: *media,
                    extent: None,
                }),
                None => self.reporter.report(b"pict"),
            },
            None => self.reporter.report(b"pict"),
        }
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
            EdgeScope::None => {}
        }
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
        };
        let columns = SectionColumns {
            count: accumulator.columns.unwrap_or(1).clamp(1, 64),
            space_twips: accumulator.column_space.map(|v| v.clamp(0, 31_680)),
            separator: accumulator.column_separator,
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
                } else {
                    self.push_segment(Segment::TextBox(TextBox {
                        id: frame.node_id,
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
            });
            self.wrapper_order.push(WrapperKind::Field);
        } else {
            self.reporter.report(b"fldSimple");
        }
    }

    /// Handles a `fldChar separate`: the outermost field switches to collecting
    /// its cached result.
    fn separate_field(&mut self) {
        if self.field_depth == 1 {
            if let Some(field) = self.field.as_mut() {
                field.in_result = true;
            }
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
                self.push_segment(Segment::Field {
                    instruction: field.instruction,
                    children,
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
    fn open_revision(&mut self, local: &[u8], element: &BytesStart<'_>) {
        let kind = if local == b"ins" {
            RevisionKind::Insertion
        } else {
            RevisionKind::Deletion
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
                }))
            }
            // A text box is already fully built (id and inner ids allocated while
            // parsing its content), so it converts directly.
            Segment::TextBox(text_box) => Ok(InlineNode::TextBox(text_box)),
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

/// Whether a local element name is known DrawingML scaffolding for an embedded
/// picture (consumed silently while inside a `w:drawing`). Anything not listed
/// still reports, so genuinely unmodeled drawing content is never lost.
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
                {
                    if *previous_properties == properties {
                        previous_text.push_str(&text);
                        continue;
                    }
                }
                normalized.push(Segment::Run { properties, text });
            }
            other => normalized.push(other),
        }
    }
    normalized
}
