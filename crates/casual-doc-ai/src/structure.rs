// SPDX-License-Identifier: Apache-2.0

//! A surface as an agent walks it: a flat, ordered list of blocks (content
//! controls listed and then opened in place, tables kept whole), and heading
//! detection. Every surface (`crate::surface`) is walked by the same code.

use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, BlockSdt, Document, Paragraph, StyleKind, Table};

use crate::project::{ALT_CHUNK, Omitted};

/// One block of a surface in reading order.
#[derive(Clone, Debug)]
pub(crate) enum Block<'a> {
    /// A paragraph.
    Paragraph(&'a Paragraph),
    /// A table, kept whole; its cells are read through `read_table`.
    Table(&'a Table),
    /// A block content control. Its own entry carries its type, tag and alias;
    /// the blocks it wraps follow it, each naming it as their `contentControl`.
    ContentControl(&'a BlockSdt),
    /// Content the projection names but does not read, and why.
    Opaque(NodeId, Omitted),
}

/// A block and the innermost block content control holding it, if any.
#[derive(Clone, Debug)]
pub(crate) struct Placed<'a> {
    pub block: Block<'a>,
    pub control: Option<NodeId>,
}

impl Block<'_> {
    pub(crate) fn id(&self) -> NodeId {
        match self {
            Self::Paragraph(paragraph) => paragraph.id,
            Self::Table(table) => table.id,
            Self::ContentControl(sdt) => sdt.id,
            Self::Opaque(id, _) => *id,
        }
    }
}

/// `blocks` in reading order, block content controls listed and then opened
/// in place. The `match` has no wildcard arm (`docs/181` G2): a new block kind
/// does not compile until it is projected or named opaque. **O(top-level
/// blocks + controls)**.
pub(crate) fn blocks_in(blocks: &[BlockNode]) -> Vec<Placed<'_>> {
    fn walk<'a>(blocks: &'a [BlockNode], control: Option<NodeId>, out: &mut Vec<Placed<'a>>) {
        for block in blocks {
            match block {
                BlockNode::Paragraph(paragraph) => out.push(Placed {
                    block: Block::Paragraph(paragraph),
                    control,
                }),
                BlockNode::Table(table) => out.push(Placed {
                    block: Block::Table(table),
                    control,
                }),
                BlockNode::Sdt(sdt) => {
                    out.push(Placed {
                        block: Block::ContentControl(sdt),
                        control,
                    });
                    walk(&sdt.blocks, Some(sdt.id), out);
                }
                BlockNode::AltChunk(chunk) => out.push(Placed {
                    block: Block::Opaque(chunk.id, Omitted::new("alt_chunk", chunk.id, ALT_CHUNK)),
                    control,
                }),
            }
        }
    }
    let mut out = Vec::new();
    walk(blocks, None, &mut out);
    out
}

/// Every paragraph of `blocks` including those inside table cells, with the
/// id of the outermost table that holds it (if any). Text boxes are not
/// entered: they are surfaces of their own. **O(blocks)**.
pub(crate) fn paragraphs_in(blocks: &[BlockNode]) -> Vec<(&Paragraph, Option<NodeId>)> {
    fn walk<'a>(
        blocks: &'a [BlockNode],
        table: Option<NodeId>,
        out: &mut Vec<(&'a Paragraph, Option<NodeId>)>,
    ) {
        for block in blocks {
            match block {
                BlockNode::Paragraph(paragraph) => out.push((paragraph, table)),
                BlockNode::Table(inner) => {
                    for row in &inner.rows {
                        for cell in &row.cells {
                            walk(&cell.blocks, Some(table.unwrap_or(inner.id)), out);
                        }
                    }
                }
                BlockNode::Sdt(sdt) => walk(&sdt.blocks, table, out),
                BlockNode::AltChunk(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(blocks, None, &mut out);
    out
}

/// Every paragraph of the body, as [`paragraphs_in`]. **O(blocks)**.
pub(crate) fn body_paragraphs(document: &Document) -> Vec<(&Paragraph, Option<NodeId>)> {
    paragraphs_in(document.body())
}

/// A table anywhere in the body, by id. **O(blocks)**.
pub(crate) fn find_table(document: &Document, id: NodeId) -> Option<&Table> {
    casual_doc_edit::find_table(document, id)
}

/// The paragraph style's display name, if the paragraph has one.
pub(crate) fn style_name(document: &Document, paragraph: &Paragraph) -> Option<String> {
    let style = paragraph.properties.style_ref?;
    document
        .definitions()
        .styles
        .get(&style)
        .and_then(|definition| definition.name.clone())
}

/// The heading level (1-9) of a paragraph, or `None` for body text.
///
/// Word's rule: the paragraph's own `w:outlineLvl`, else its style chain's, where
/// 0-8 are levels 1-9 and 9 is body text. A style named `heading N` or `Title`
/// with no outline level is still treated as a heading, as Word's navigation
/// pane does for documents whose styles omit `w:outlineLvl`. **O(style chain)**.
pub(crate) fn heading_level(document: &Document, paragraph: &Paragraph) -> Option<u8> {
    if let Some(level) = paragraph.properties.outline_level {
        return (level < 9).then_some(level + 1);
    }
    let styles = &document.definitions().styles;
    let mut next = paragraph.properties.style_ref;
    let mut hops = 0;
    while let Some(id) = next {
        let style = styles.get(&id)?;
        if style.kind != StyleKind::Paragraph {
            return None;
        }
        if let Some(level) = style
            .paragraph
            .as_ref()
            .and_then(|properties| properties.outline_level)
        {
            return (level < 9).then_some(level + 1);
        }
        if let Some(name) = &style.name {
            let lower = name.to_ascii_lowercase();
            if lower == "title" {
                return Some(1);
            }
            if let Some(rest) = lower.strip_prefix("heading ")
                && let Ok(level) = rest.trim().parse::<u8>()
                && (1..=9).contains(&level)
            {
                return Some(level);
            }
        }
        hops += 1;
        if hops > 32 {
            return None;
        }
        next = style.based_on;
    }
    None
}
