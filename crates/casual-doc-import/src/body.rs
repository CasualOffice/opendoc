//! Main-document body parsing into v1 block nodes.

use std::collections::BTreeMap;

use casual_doc_model::v1::{
    BlockNode, Break, BreakKind, Comment, CommentId, CommentReference, Drawing, Extent,
    ExternalTarget, Field, HeaderFooterId, HeaderFooterKind, HeaderFooterRef, Hyperlink,
    HyperlinkTarget, InlineNode, InternalTarget, MAX_EMU, MAX_FIELD_INSTRUCTION_BYTES,
    MAX_TEXTBOX_DEPTH, MediaId, NoteId, NoteKind, NoteReference, PageMargins, PageSize, Paragraph,
    ParagraphProperties, Run, RunProperties, SectionBoundary, SectionColumns, SectionId, StyleKind,
    Tab, TextBox, VerticalMerge,
};
use casual_doc_model::{IdGenerator, NodeId};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::numbering::Numbering;
use crate::properties::{
    apply_paragraph_property, apply_run_property, attribute_value, break_kind,
};
use crate::report::Reporter;
use crate::styles::Styles;
use crate::tables::TableStack;

/// A run/tab/break/drawing/hyperlink/field segment before ids and normalization.
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
    textbox_id: NodeId,
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
    tables: TableStack,
    tcpr_depth: u32,
    suppressed_tbl_depth: u32,
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
    /// open hyperlink (a wrapper never opens inside another wrapper).
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
    tables: TableStack,
    tcpr_depth: u32,
    /// Depth of nested tables refused past `MAX_TABLE_DEPTH`; while non-zero the
    /// table structure is suppressed so it cannot corrupt the enclosing table.
    suppressed_tbl_depth: u32,
    segments: Vec<Segment>,
    blocks: Vec<BlockNode>,
    /// Suspended enclosing contexts, one per open text box.
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
            tables: TableStack::default(),
            tcpr_depth: 0,
            suppressed_tbl_depth: 0,
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
        }
    }
}

/// Parses main-document body bytes into ordered block nodes, allocating ids.
pub(crate) fn parse<'a>(
    xml: &[u8],
    ids: &'a mut IdGenerator,
    reporter: &'a mut Reporter,
    inputs: ParseInputs<'a>,
    config: ImportConfig,
) -> Result<(Vec<BlockNode>, Vec<SectionBoundary>), ImportError> {
    let mut parser = BodyParser::build(ids, reporter, &inputs, None, config);
    parser.run(xml)?;
    // Unwind any text box left open by malformed input so the true body root is
    // restored, then finish a paragraph the unwind may have re-opened so its
    // content is committed, not stranded in a suspended frame.
    while !parser.frames.is_empty() {
        parser.exit_textbox()?;
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
    let mut parser = BodyParser::build(ids, reporter, &inputs, Some(container), config);
    parser.run(xml)?;
    while !parser.frames.is_empty() {
        parser.exit_textbox()?;
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
    let mut parser = BodyParser::build(ids, reporter, &inputs, None, config);
    parser.hf_root = Some(root);
    parser.run(xml)?;
    while !parser.frames.is_empty() {
        parser.exit_textbox()?;
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
    let mut parser = BodyParser::build(ids, reporter, &inputs, Some(b"comment"), config);
    parser.run(xml)?;
    while !parser.frames.is_empty() {
        parser.exit_textbox()?;
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
                        }
                        None => self.reporter.report(b"hyperlink"),
                    }
                } else {
                    // A nested hyperlink is not modeled; its runs flatten into
                    // the outer link and the nesting is reported.
                    self.reporter.report(b"hyperlink");
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
            // Only a true body-level `w:sectPr` is a document section; a `sectPr`
            // inside a text box (frames non-empty), a notes part, or a
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
                if let Some(section) = self.section.as_mut() {
                    section.columns =
                        attribute_value(element, b"num").and_then(|value| value.parse().ok());
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
            b"tr"
                if self.tables.is_active()
                    && self.suppressed_tbl_depth == 0
                    && !self.paragraph_open
                    && !self.run_open =>
            {
                self.tables.open_row(&mut *self.ids)?;
            }
            b"tc"
                if self.tables.is_active()
                    && self.suppressed_tbl_depth == 0
                    && !self.paragraph_open
                    && !self.run_open =>
            {
                self.tcpr_depth = 0;
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
            _ if self.rpr_depth > 0 => {
                if !apply_run_property(&mut self.run_properties, local, element) {
                    self.reporter.report(local);
                }
            }
            _ if self.ppr_depth > 0 => {
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
            b"document" => self.in_document = false,
            b"body" => self.in_body = false,
            b"AlternateContent" => {
                self.alt_stack.pop();
            }
            b"txbxContent" => self.exit_textbox()?,
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
            b"rPr" => self.rpr_depth = self.rpr_depth.saturating_sub(1),
            b"sectPr" => {
                if let Some(accumulator) = self.section.take() {
                    self.build_section(accumulator)?;
                }
            }
            b"t" if self.in_text => {
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
                    if let Some(accumulator) = self.hyperlink.take() {
                        let children = normalize_segments(accumulator.segments);
                        if children.is_empty() {
                            self.reporter.report(b"hyperlink");
                        } else {
                            // Commit to the parent stream: a hyperlink never nests.
                            self.segments.push(Segment::Hyperlink {
                                target: accumulator.target,
                                tooltip: accumulator.tooltip,
                                children,
                            });
                        }
                    }
                }
                self.hyperlink_depth = self.hyperlink_depth.saturating_sub(1);
            }
            b"tcPr" => self.tcpr_depth = self.tcpr_depth.saturating_sub(1),
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

    fn build_section(&mut self, accumulator: SectionAccumulator) -> Result<(), ImportError> {
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
        };
        self.sections.push(SectionBoundary {
            id,
            page_size,
            page_margins,
            columns,
            headers: accumulator.headers,
            footers: accumulator.footers,
        });
        Ok(())
    }

    /// Enters a text box: allocate its id (document order), then suspend the
    /// enclosing content context so the box's own paragraphs/runs/tables build
    /// into a fresh context and cannot corrupt the enclosing paragraph or drawing.
    fn enter_textbox(&mut self) -> Result<(), ImportError> {
        let textbox_id = self.next_id()?;
        let depth = self.frames.len() as u32 + 1;
        let frame = ContentFrame {
            textbox_id,
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
            tables: std::mem::take(&mut self.tables),
            tcpr_depth: std::mem::take(&mut self.tcpr_depth),
            suppressed_tbl_depth: std::mem::take(&mut self.suppressed_tbl_depth),
            segments: std::mem::take(&mut self.segments),
            blocks: std::mem::take(&mut self.blocks),
        };
        self.frames.push(frame);
        Ok(())
    }

    /// Exits a text box: finish its content, restore the enclosing context, and
    /// route the built `TextBox` into the enclosing inline stream. A box that is
    /// empty or nested past the bound is reported and dropped (never silent).
    fn exit_textbox(&mut self) -> Result<(), ImportError> {
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
        self.tables = frame.tables;
        self.tcpr_depth = frame.tcpr_depth;
        self.suppressed_tbl_depth = frame.suppressed_tbl_depth;
        self.segments = frame.segments;
        self.blocks = frame.blocks;
        if blocks.is_empty() || frame.depth > MAX_TEXTBOX_DEPTH {
            self.reporter.report(b"txbxContent");
        } else {
            self.push_segment(Segment::TextBox(TextBox {
                id: frame.textbox_id,
                blocks,
            }));
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
                self.exit_textbox()?;
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
        self.in_body = false;
        if let Some((source_id, node_id, meta)) = self.current_note.take() {
            let blocks = std::mem::take(&mut self.blocks);
            self.notes.push((source_id, node_id, meta, blocks));
        }
        self.skip_note = false;
        Ok(())
    }

    /// Routes a segment into the innermost open wrapper: an open field, then an
    /// open hyperlink, else the paragraph. Fields and hyperlinks never open
    /// inside one another, so at most one wrapper is ever open.
    ///
    /// All display segments of an open field are captured into its cached result,
    /// including any that arrive before `separate` — a well-formed field has none
    /// there (the instruction is `w:instrText`, routed separately), so this only
    /// preserves malformed pre-`separate` display content instead of dropping it.
    fn push_segment(&mut self, segment: Segment) {
        if let Some(field) = self.field.as_mut() {
            field.segments.push(segment);
            return;
        }
        match self.hyperlink.as_mut() {
            Some(accumulator) => accumulator.segments.push(segment),
            None => self.segments.push(segment),
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
            });
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
    /// flattened into the enclosing stream so no display text is lost.
    fn commit_field(&mut self) {
        if let Some(field) = self.field.take() {
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
        // Robustness: a `w:p` that closes with an open hyperlink is malformed;
        // flush what was accumulated so nothing is dropped, then reset state.
        if let Some(accumulator) = self.hyperlink.take() {
            let children = normalize_segments(accumulator.segments);
            if children.is_empty() {
                self.reporter.report(b"hyperlink");
            } else {
                self.segments.push(Segment::Hyperlink {
                    target: accumulator.target,
                    tooltip: accumulator.tooltip,
                    children,
                });
            }
        }
        self.hyperlink_depth = 0;
        // Robustness: a `w:p` that closes with an open field (missing `end`) is
        // malformed; commit what was accumulated so its cached text is not lost.
        self.commit_field();
        self.field_depth = 0;
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
        }
    }
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
