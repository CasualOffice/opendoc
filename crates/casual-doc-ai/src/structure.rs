// SPDX-License-Identifier: Apache-2.0

//! The body as an agent walks it: a flat, ordered list of blocks (content
//! controls opened, tables kept whole), and heading detection.

use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, Document, Paragraph, StyleKind, Table};

/// One block of the body in reading order.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Block<'a> {
    /// A paragraph.
    Paragraph(&'a Paragraph),
    /// A table, kept whole; its cells are read through `read_table`.
    Table(&'a Table),
    /// Preserved content the projection does not read (an `altChunk`).
    Other(NodeId),
}

impl Block<'_> {
    pub(crate) fn id(&self) -> NodeId {
        match self {
            Self::Paragraph(paragraph) => paragraph.id,
            Self::Table(table) => table.id,
            Self::Other(id) => *id,
        }
    }
}

/// The body's blocks in reading order, block content controls opened in
/// place. **O(top-level blocks)**.
pub(crate) fn body_blocks(document: &Document) -> Vec<Block<'_>> {
    fn walk<'a>(blocks: &'a [BlockNode], out: &mut Vec<Block<'a>>) {
        for block in blocks {
            match block {
                BlockNode::Paragraph(paragraph) => out.push(Block::Paragraph(paragraph)),
                BlockNode::Table(table) => out.push(Block::Table(table)),
                BlockNode::Sdt(sdt) => walk(&sdt.blocks, out),
                BlockNode::AltChunk(chunk) => out.push(Block::Other(chunk.id)),
            }
        }
    }
    let mut out = Vec::new();
    walk(document.body(), &mut out);
    out
}

/// Every paragraph of the body including those inside table cells, with the
/// id of the table that holds it (if any). **O(blocks)**.
pub(crate) fn body_paragraphs(document: &Document) -> Vec<(&Paragraph, Option<NodeId>)> {
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
    walk(document.body(), None, &mut out);
    out
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
