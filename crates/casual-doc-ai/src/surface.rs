// SPDX-License-Identifier: Apache-2.0

//! Surfaces: the block lists a document's text lives in (`docs/181` §2.2, R2).
//!
//! The body is one surface. Every header, footer, footnote, endnote, comment
//! and text box is another, each with the same recursive block model. A
//! paragraph id is unique across all of them (`docs/85` §8c.3: the model's
//! uniqueness set covers every story), so an anchor never needs a surface to
//! resolve; a *read* does, because it walks one list in order.
//!
//! This is `docs/85`'s *Location* concept, and the engine's own
//! `casual_doc_edit::Surface` (body, headers, footers, notes) extended by the
//! two kinds an agent also reads: text boxes, which hang off an inline inside a
//! paragraph of another surface, and comment bodies, which are read-only here
//! (a comment is changed through the `comment`/`resolve_comment` edits).
//!
//! The pattern is an **index built once per call**: `enumerate` walks the
//! document one time (O(document)) and every per-surface question after that
//! is answered from the list, never by a per-node scan.

use std::fmt;

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, CommentId, Document, GroupChild, HeaderFooterId, HeaderFooterKind, InlineNode,
    NoteId,
};

use crate::error::{AiError, AiResult};
use crate::text::clip;

/// One surface of the document.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Surface {
    /// The main document body.
    Body,
    /// A header definition, by its id.
    Header(NodeId),
    /// A footer definition, by its id.
    Footer(NodeId),
    /// A footnote's own body, by the note's id.
    Footnote(NodeId),
    /// An endnote's own body, by the note's id.
    Endnote(NodeId),
    /// A text box's content (inline, floating, or inside a shape group), by the
    /// text box's id.
    TextBox(NodeId),
    /// A comment's own body, by the comment's id. Read-only through anchors.
    Comment(NodeId),
}

impl Surface {
    /// The kind, as the read tools spell it: `body`, `header`, `footer`,
    /// `footnote`, `endnote`, `text_box` or `comment`.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Header(_) => "header",
            Self::Footer(_) => "footer",
            Self::Footnote(_) => "footnote",
            Self::Endnote(_) => "endnote",
            Self::TextBox(_) => "text_box",
            Self::Comment(_) => "comment",
        }
    }

    /// The surface's own id: `None` for the body.
    #[must_use]
    pub const fn id(self) -> Option<NodeId> {
        match self {
            Self::Body => None,
            Self::Header(id)
            | Self::Footer(id)
            | Self::Footnote(id)
            | Self::Endnote(id)
            | Self::TextBox(id)
            | Self::Comment(id) => Some(id),
        }
    }

    /// Parses `body` or `<kind>:<id>`, the form [`Surface`]'s `Display` writes
    /// and every read result returns.
    ///
    /// # Errors
    ///
    /// `invalid-argument` for an unknown kind or a malformed id.
    pub fn parse(value: &str) -> AiResult<Self> {
        if value == "body" {
            return Ok(Self::Body);
        }
        let bad = || {
            AiError::invalid(format!(
                "`{}` is not a surface: use `body` or `<kind>:<id>` as get_outline lists them \
                 (kinds: header, footer, footnote, endnote, text_box, comment).",
                clip(value, 80)
            ))
        };
        let (kind, id) = value.split_once(':').ok_or_else(bad)?;
        let id: NodeId = id.parse().map_err(|_| bad())?;
        Ok(match kind {
            "header" => Self::Header(id),
            "footer" => Self::Footer(id),
            "footnote" => Self::Footnote(id),
            "endnote" => Self::Endnote(id),
            "text_box" => Self::TextBox(id),
            "comment" => Self::Comment(id),
            _ => return Err(bad()),
        })
    }
}

impl fmt::Display for Surface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.id() {
            None => f.write_str("body"),
            Some(id) => write!(f, "{}:{id}", self.kind()),
        }
    }
}

/// One header or footer use: which section, which page type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Use {
    /// Zero-based section ordinal.
    pub section: usize,
    /// `default`, `first` or `even`.
    pub page: &'static str,
}

/// One surface with its blocks and where it sits.
#[derive(Clone, Debug)]
pub(crate) struct Entry<'a> {
    pub surface: Surface,
    pub blocks: &'a [BlockNode],
    /// For a text box: the surface holding it and the paragraph it hangs off.
    pub host: Option<(Surface, NodeId)>,
    /// For a header or footer: every section and page type that shows it.
    pub uses: Vec<Use>,
}

const fn page_type(kind: HeaderFooterKind) -> &'static str {
    match kind {
        HeaderFooterKind::Default => "default",
        HeaderFooterKind::First => "first",
        HeaderFooterKind::Even => "even",
    }
}

/// Every surface the document has, in a fixed order: body, headers, footers,
/// footnotes, endnotes, comments, then text boxes in the order they are met
/// walking those (a box inside a box follows its host). **O(document)**, once.
pub(crate) fn enumerate(document: &Document) -> Vec<Entry<'_>> {
    let definitions = document.definitions();
    let mut out: Vec<Entry<'_>> = Vec::new();
    out.push(Entry {
        surface: Surface::Body,
        blocks: document.body(),
        host: None,
        uses: Vec::new(),
    });
    let uses_of = |id: HeaderFooterId, header: bool| -> Vec<Use> {
        let mut uses = Vec::new();
        for (section, boundary) in definitions.sections.iter().enumerate() {
            let refs = if header {
                &boundary.headers
            } else {
                &boundary.footers
            };
            for reference in refs.iter().filter(|r| r.reference == id) {
                uses.push(Use {
                    section,
                    page: page_type(reference.kind),
                });
            }
        }
        uses
    };
    for (id, header) in definitions.headers.iter() {
        out.push(Entry {
            surface: Surface::Header(id.node_id()),
            blocks: &header.blocks,
            host: None,
            uses: uses_of(*id, true),
        });
    }
    for (id, footer) in definitions.footers.iter() {
        out.push(Entry {
            surface: Surface::Footer(id.node_id()),
            blocks: &footer.blocks,
            host: None,
            uses: uses_of(*id, false),
        });
    }
    for (id, note) in definitions.footnotes.iter() {
        out.push(Entry {
            surface: Surface::Footnote(id.node_id()),
            blocks: &note.blocks,
            host: None,
            uses: Vec::new(),
        });
    }
    for (id, note) in definitions.endnotes.iter() {
        out.push(Entry {
            surface: Surface::Endnote(id.node_id()),
            blocks: &note.blocks,
            host: None,
            uses: Vec::new(),
        });
    }
    for (id, comment) in definitions.comments.iter() {
        out.push(Entry {
            surface: Surface::Comment(id.node_id()),
            blocks: &comment.blocks,
            host: None,
            uses: Vec::new(),
        });
    }
    // Text boxes: a work list, so a box found inside a box is walked once, in
    // turn. Each block list is visited exactly once overall.
    let mut next = 0;
    while next < out.len() {
        let (surface, blocks) = (out[next].surface, out[next].blocks);
        let mut found = Vec::new();
        text_boxes_in(blocks, &mut found);
        for (id, boxed, paragraph) in found {
            out.push(Entry {
                surface: Surface::TextBox(id),
                blocks: boxed,
                host: Some((surface, paragraph)),
                uses: Vec::new(),
            });
        }
        next += 1;
    }
    out
}

/// The text boxes directly reachable from `blocks` (not those inside a found
/// box: those are found when the box itself is walked), with the paragraph
/// each hangs off. The block and inline matches have no wildcard arm
/// (`docs/181` G2): a new node kind that can hold a story does not compile
/// until someone decides whether it is a surface.
fn text_boxes_in<'a>(blocks: &'a [BlockNode], out: &mut Vec<(NodeId, &'a [BlockNode], NodeId)>) {
    fn group<'a>(
        children: &'a [GroupChild],
        paragraph: NodeId,
        out: &mut Vec<(NodeId, &'a [BlockNode], NodeId)>,
    ) {
        for child in children {
            match child {
                GroupChild::TextBox(text_box) => {
                    out.push((text_box.id, &text_box.blocks, paragraph))
                }
                GroupChild::Group(nested) => group(&nested.children, paragraph, out),
                GroupChild::Picture(_) | GroupChild::Shape(_) => {}
            }
        }
    }
    fn inlines<'a>(
        nodes: &'a [InlineNode],
        paragraph: NodeId,
        out: &mut Vec<(NodeId, &'a [BlockNode], NodeId)>,
    ) {
        for node in nodes {
            match node {
                InlineNode::TextBox(text_box) => {
                    out.push((text_box.id, &text_box.blocks, paragraph))
                }
                InlineNode::Group(wgp) => group(&wgp.children, paragraph, out),
                InlineNode::Hyperlink(link) => inlines(&link.inlines, paragraph, out),
                InlineNode::Field(field) => inlines(&field.inlines, paragraph, out),
                InlineNode::Revision(revision) => inlines(&revision.inlines, paragraph, out),
                InlineNode::Sdt(sdt) => inlines(&sdt.inlines, paragraph, out),
                InlineNode::Run(_)
                | InlineNode::Tab(_)
                | InlineNode::Break(_)
                | InlineNode::Drawing(_)
                | InlineNode::AnchoredDrawing(_)
                | InlineNode::EmbeddedObject(_)
                | InlineNode::NoteReference(_)
                | InlineNode::NoteNumberMark(_)
                | InlineNode::CommentReference(_)
                | InlineNode::CommentRangeStart(_)
                | InlineNode::CommentRangeEnd(_)
                | InlineNode::BookmarkStart(_)
                | InlineNode::BookmarkEnd(_)
                | InlineNode::FieldRangeStart(_)
                | InlineNode::FieldRangeEnd(_)
                | InlineNode::MoveRangeStart(_)
                | InlineNode::MoveRangeEnd(_)
                | InlineNode::Math(_)
                | InlineNode::Symbol(_)
                | InlineNode::HorizontalRule(_)
                | InlineNode::NoBreakHyphen(_)
                | InlineNode::SoftHyphen(_)
                | InlineNode::PositionalTab(_) => {}
            }
        }
    }
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => inlines(&paragraph.inlines, paragraph.id, out),
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        text_boxes_in(&cell.blocks, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => text_boxes_in(&sdt.blocks, out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

/// The block list of one surface. **O(log definitions)** for a header, footer,
/// note or comment; **O(document)** for a text box (one walk).
///
/// # Errors
///
/// `surface-not-found` when the document has no such surface at this revision.
pub(crate) fn blocks_of(document: &Document, surface: Surface) -> AiResult<&[BlockNode]> {
    let definitions = document.definitions();
    let found = match surface {
        Surface::Body => Some(document.body()),
        Surface::Header(id) => definitions
            .headers
            .get(&HeaderFooterId::new(id))
            .map(|h| h.blocks.as_slice()),
        Surface::Footer(id) => definitions
            .footers
            .get(&HeaderFooterId::new(id))
            .map(|h| h.blocks.as_slice()),
        Surface::Footnote(id) => definitions
            .footnotes
            .get(&NoteId::new(id))
            .map(|n| n.blocks.as_slice()),
        Surface::Endnote(id) => definitions
            .endnotes
            .get(&NoteId::new(id))
            .map(|n| n.blocks.as_slice()),
        Surface::Comment(id) => definitions
            .comments
            .get(&CommentId::new(id))
            .map(|c| c.blocks.as_slice()),
        Surface::TextBox(_) => enumerate(document)
            .into_iter()
            .find(|entry| entry.surface == surface)
            .map(|entry| entry.blocks),
    };
    found.ok_or_else(|| {
        AiError::new(
            "surface-not-found",
            "The document has no such surface at this revision; get_outline lists them.",
        )
        .with_details(serde_json::json!({ "surface": surface.to_string() }))
    })
}

/// Whether `blocks` holds a block (paragraph, table, row, cell or content
/// control) with `id`, not descending into text boxes (they are their own
/// surfaces). O(blocks).
pub(crate) fn holds(blocks: &[BlockNode], id: NodeId) -> bool {
    blocks.iter().any(|block| match block {
        BlockNode::Paragraph(paragraph) => paragraph.id == id,
        BlockNode::Table(table) => {
            table.id == id
                || table.rows.iter().any(|row| {
                    row.id == id
                        || row
                            .cells
                            .iter()
                            .any(|cell| cell.id == id || holds(&cell.blocks, id))
                })
        }
        BlockNode::Sdt(sdt) => sdt.id == id || holds(&sdt.blocks, id),
        BlockNode::AltChunk(chunk) => chunk.id == id,
    })
}

/// The surface whose own blocks hold `id`. **O(document)**, once.
pub(crate) fn surface_holding(document: &Document, id: NodeId) -> Option<Surface> {
    enumerate(document)
        .into_iter()
        .find(|entry| holds(entry.blocks, id))
        .map(|entry| entry.surface)
}
