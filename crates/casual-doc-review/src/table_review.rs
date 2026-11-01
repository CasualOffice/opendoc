// SPDX-License-Identifier: Apache-2.0

//! A whole table as ONE tracked change: inserted or deleted under review (`docs/183` §23,
//! `docs/109` RM-52).
//!
//! # What Word writes, and what this models
//!
//! Word records a table inserted with Track Changes on as `w:trPr/w:ins` on every row and
//! `w:ins` around the text typed into its cells; a table deleted with Track Changes on as
//! `w:trPr/w:del` on every row and `w:del` around the cells' text. The model already carried
//! the row mark (`TableRowProperties::row_revision`, imported and exported verbatim) but no
//! command could author one, so `insert_table` was untrackable and an assistant in Suggesting
//! mode could not propose a table at all (`docs/183` §22 item 1).
//!
//! # One card, decided as one
//!
//! A table whose EVERY row carries the same row mark is listed as one review item
//! (`table_insertion` / `table_deletion`, id `<table id>:table`) and its cells' own text
//! revisions are folded into it, not listed again: a reviewer decides "this table", as in
//! Word's balloon, not forty cell insertions. Accepting an insertion (or rejecting a deletion)
//! keeps the table, with the row marks cleared and the cells' revisions decided the same way
//! (one `ReplaceTable`); rejecting an insertion (or accepting a deletion) removes it (one
//! `DeleteTable`). A table with only SOME rows marked is a row-level change and keeps today's
//! per-text listing.
//!
//! Complexity: O(cells) for every function here; finding a table is the edit crate's
//! O(document) walk, paid once per command.

use casual_doc_edit::{Operation, Pos, find_table, surface_block_lists};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Document, InlineNode, MarkRevision, MarkRevisionKind, Revision, RevisionKind, Run,
    RunProperties, Table,
};

use crate::body::{
    collect_block_text, decide_all_review_revisions, validate_authored_revision_author,
};
use crate::lower::{Lowered, ReviewIds};

/// The review id suffix of a whole-table change.
pub const TABLE_REVISION_SUFFIX: &str = ":table";

/// The review id of table `table`'s whole-table change. O(1).
#[must_use]
pub fn table_revision_id(table: NodeId) -> String {
    format!("{table}{TABLE_REVISION_SUFFIX}")
}

/// The kind of whole-table change `table` carries: every row marked, all the same kind.
/// `None` for an untracked table or one with only some rows marked. O(rows).
#[must_use]
pub fn tracked_table_kind(table: &Table) -> Option<MarkRevisionKind> {
    let first = table.rows.first()?.properties.row_revision.as_ref()?.kind;
    table
        .rows
        .iter()
        .all(|row| {
            row.properties
                .row_revision
                .as_ref()
                .is_some_and(|mark| mark.kind == first)
        })
        .then_some(first)
}

/// The first paragraph directly in the table's first cell, where its card anchors.
fn first_paragraph(table: &Table) -> Option<NodeId> {
    table
        .rows
        .first()?
        .cells
        .first()?
        .blocks
        .iter()
        .find_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(paragraph.id),
            _ => None,
        })
}

/// The one review item a whole-table change lists as, in `collect_review_revisions`' shape.
#[must_use]
pub fn table_revision_item(table: &Table, kind: MarkRevisionKind) -> serde_json::Value {
    let mark = table
        .rows
        .first()
        .and_then(|row| row.properties.row_revision.as_ref());
    let header = table
        .rows
        .first()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| {
                    let mut text = Vec::new();
                    collect_block_text(&cell.blocks, &mut text);
                    text.into_iter()
                        .map(|(_, text)| text)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .unwrap_or_default();
    let text: String = header.chars().take(120).collect();
    serde_json::json!({
        "id": table_revision_id(table.id),
        "kind": match kind {
            MarkRevisionKind::Insertion => "table_insertion",
            MarkRevisionKind::Deletion => "table_deletion",
        },
        "author": mark.and_then(|mark| mark.author.clone()),
        "date": mark.and_then(|mark| mark.date.clone()),
        "revisionId": mark.and_then(|mark| mark.revision_id.clone()),
        "text": text,
        "anchor": {
            "node": first_paragraph(table).unwrap_or(table.id).to_string(),
            "start": 0,
            "end": 0,
        },
    })
}

/// One cell's text, made safe for a single paragraph: line breaks and other control
/// characters become spaces, and it is bounded at 4,000 bytes on a character boundary.
fn cell_text(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        let ch = if ch.is_control() { ' ' } else { ch };
        if out.len() + ch.len_utf8() > 4_000 {
            break;
        }
        out.push(ch);
    }
    out.trim().to_owned()
}

/// Marks a freshly built, untracked `InsertTable` lowering as a TRACKED insertion authored
/// `author`: every row `w:trPr/w:ins`, each non-empty `cells[r][c]` typed into its cell's
/// first paragraph as a tracked insertion, and the first row a repeating header row when
/// `header`. `edit` is what `casual_doc_command::table::insert_table` returned; `caret` the
/// paragraph it named. O(cells).
///
/// # Errors
/// No `InsertTable` in `edit`, an invalid author, or ids exhausted.
pub fn suggest_insert_table(
    mut edit: Vec<Operation>,
    caret: Option<NodeId>,
    ids: &mut ReviewIds<'_>,
    cells: &[Vec<String>],
    header: bool,
    author: Option<String>,
    date: Option<String>,
) -> Result<Lowered, String> {
    validate_authored_revision_author(author.as_deref())?;
    let Some(table) = edit.iter_mut().find_map(|op| match op {
        Operation::InsertTable { table, .. } => Some(table),
        _ => None,
    }) else {
        return Err("a tracked table insertion needs a table".to_owned());
    };
    let table_id = table.id;
    for (r, row) in table.rows.iter_mut().enumerate() {
        row.properties.row_revision = Some(MarkRevision {
            kind: MarkRevisionKind::Insertion,
            author: author.clone(),
            date: date.clone(),
            revision_id: Some(ids.revisions.allocate()?),
        });
        if header && r == 0 {
            row.properties.header = true;
        }
        for (c, cell) in row.cells.iter_mut().enumerate() {
            let text = cells
                .get(r)
                .and_then(|row| row.get(c))
                .map(|text| cell_text(text))
                .unwrap_or_default();
            if text.is_empty() {
                continue;
            }
            let Some(BlockNode::Paragraph(paragraph)) = cell
                .blocks
                .iter_mut()
                .find(|block| matches!(block, BlockNode::Paragraph(_)))
            else {
                continue;
            };
            let run = ids
                .nodes
                .next_id()
                .map_err(|_| "id space exhausted".to_owned())?;
            let id = ids
                .nodes
                .next_id()
                .map_err(|_| "id space exhausted".to_owned())?;
            paragraph.inlines = vec![InlineNode::Revision(Box::new(Revision {
                id,
                kind: RevisionKind::Insertion,
                author: author.clone(),
                date: date.clone(),
                revision_id: Some(ids.revisions.allocate()?),
                editor_group: None,
                inlines: vec![InlineNode::Run(Run {
                    id: run,
                    properties: RunProperties::default().into(),
                    text,
                })],
            }))];
        }
    }
    let caret = caret.unwrap_or(table_id);
    Ok(Lowered {
        operations: edit,
        caret: Pos::new(caret, 0),
        group: None,
    })
}

/// Deletes `table` under review: every row `w:trPr/w:del`, and each cell paragraph's text
/// wrapped in a tracked deletion, so the table stays on the page, struck, until a reviewer
/// decides. One `ReplaceTable`. O(cells).
///
/// # Errors
/// An unknown table, one that already carries a row change (decide that first), an invalid
/// author, or ids exhausted.
pub fn suggest_delete_table(
    document: &Document,
    ids: &mut ReviewIds<'_>,
    table: NodeId,
    author: Option<String>,
    date: Option<String>,
) -> Result<Lowered, String> {
    validate_authored_revision_author(author.as_deref())?;
    let found = find_table(document, table).ok_or_else(|| "table not found".to_owned())?;
    if found
        .rows
        .iter()
        .any(|row| row.properties.row_revision.is_some())
    {
        return Err(casual_doc_edit::refused!(
            "review.table-already-tracked",
            "This table already carries a tracked row change. Accept or reject it first."
        )
        .to_string());
    }
    let mut replacement = found.clone();
    for row in &mut replacement.rows {
        row.properties.row_revision = Some(MarkRevision {
            kind: MarkRevisionKind::Deletion,
            author: author.clone(),
            date: date.clone(),
            revision_id: Some(ids.revisions.allocate()?),
        });
        for cell in &mut row.cells {
            for block in &mut cell.blocks {
                let BlockNode::Paragraph(paragraph) = block else {
                    continue;
                };
                let plain = !paragraph.inlines.is_empty()
                    && paragraph
                        .inlines
                        .iter()
                        .all(|inline| matches!(inline, InlineNode::Run(_)));
                if !plain {
                    continue;
                }
                let id = ids
                    .nodes
                    .next_id()
                    .map_err(|_| "id space exhausted".to_owned())?;
                let inlines = std::mem::take(&mut paragraph.inlines);
                paragraph.inlines = vec![InlineNode::Revision(Box::new(Revision {
                    id,
                    kind: RevisionKind::Deletion,
                    author: author.clone(),
                    date: date.clone(),
                    revision_id: Some(ids.revisions.allocate()?),
                    editor_group: None,
                    inlines,
                }))];
            }
        }
    }
    let caret = first_paragraph(&replacement).unwrap_or(table);
    Ok(Lowered {
        operations: vec![Operation::ReplaceTable {
            table,
            replacement: Box::new(replacement),
        }],
        caret: Pos::new(caret, 0),
        group: None,
    })
}

/// Whether the decision keeps the table on the page.
const fn keeps(kind: MarkRevisionKind, accept: bool) -> bool {
    matches!(
        (kind, accept),
        (MarkRevisionKind::Insertion, true) | (MarkRevisionKind::Deletion, false)
    )
}

/// `table` as it is after the decision that keeps it: row marks cleared and the cells'
/// revisions decided `accept`. O(cells).
fn kept(table: &Table, accept: bool) -> Table {
    let mut out = table.clone();
    for row in &mut out.rows {
        row.properties.row_revision = None;
        for cell in &mut row.cells {
            decide_all_review_revisions(&mut cell.blocks, accept);
        }
    }
    out
}

/// Whether `table` is the only block of the table cell that holds it, which `DeleteTable`
/// refuses (a cell's blocks are non-empty). O(document).
fn sole_block_of_a_cell(document: &Document, table: NodeId) -> bool {
    fn walk(blocks: &[BlockNode], table: NodeId, in_cell: bool) -> Option<bool> {
        for block in blocks {
            match block {
                BlockNode::Table(found) if found.id == table => {
                    return Some(in_cell && blocks.len() == 1);
                }
                BlockNode::Table(other) => {
                    for row in &other.rows {
                        for cell in &row.cells {
                            if let Some(answer) = walk(&cell.blocks, table, true) {
                                return Some(answer);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(answer) = walk(&sdt.blocks, table, false) {
                        return Some(answer);
                    }
                }
                BlockNode::Paragraph(_) | BlockNode::AltChunk(_) => {}
            }
        }
        None
    }
    surface_block_lists(document)
        .into_iter()
        .find_map(|blocks| walk(blocks, table, false))
        .unwrap_or(false)
}

/// The operation deciding a whole-table change on `table` (a table found in `document`).
fn decision(document: &Document, table: &Table, accept: bool) -> Result<Operation, String> {
    let kind = tracked_table_kind(table).ok_or_else(|| "revision not found".to_owned())?;
    if keeps(kind, accept) {
        return Ok(Operation::ReplaceTable {
            table: table.id,
            replacement: Box::new(kept(table, accept)),
        });
    }
    if sole_block_of_a_cell(document, table.id) {
        return Err(casual_doc_edit::refused!(
            "review.table-sole-block",
            "This table is the only content of the cell that holds it, so it cannot be removed \
             on its own. Remove the outer table's row instead."
        )
        .to_string());
    }
    Ok(Operation::DeleteTable { table: table.id })
}

/// Accepts or rejects the whole-table change with review id `<table>:table`. O(cells), plus
/// the O(document) walk that finds the table.
///
/// # Errors
/// An unknown table, one that is not a whole-table change, or a removal the cell refuses.
pub fn decide_table_revision(
    document: &Document,
    table: &str,
    accept: bool,
) -> Result<Lowered, String> {
    let id: NodeId = table.parse().map_err(|_| "revision not found".to_owned())?;
    let found = find_table(document, id).ok_or_else(|| "revision not found".to_owned())?;
    let operation = decision(document, found, accept)?;
    let caret = match &operation {
        Operation::ReplaceTable { .. } => first_paragraph(found).unwrap_or(id),
        _ => document.id(),
    };
    Ok(Lowered {
        operations: vec![operation],
        caret: Pos::new(caret, 0),
        group: None,
    })
}

/// Every whole-table change in `blocks`, outermost only (a table inside a tracked table goes
/// with it), in document order.
pub fn collect_tracked_tables<'a>(blocks: &'a [BlockNode], out: &mut Vec<&'a Table>) {
    for block in blocks {
        match block {
            BlockNode::Table(table) if tracked_table_kind(table).is_some() => out.push(table),
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_tracked_tables(&cell.blocks, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => collect_tracked_tables(&sdt.blocks, out),
            BlockNode::Paragraph(_) | BlockNode::AltChunk(_) => {}
        }
    }
}

/// The operations Accept All / Reject All add for whole-table changes, to run AFTER the
/// review-state update: `decided` is each surface with its revisions already decided, so a
/// kept table's replacement carries its cells as decided. O(tracked cells).
///
/// # Errors
/// A removal a cell refuses.
pub fn decide_all_tables(
    document: &Document,
    decided: &[Vec<BlockNode>],
    accept: bool,
) -> Result<Vec<Operation>, String> {
    let mut tables = Vec::new();
    for blocks in surface_block_lists(document) {
        collect_tracked_tables(blocks, &mut tables);
    }
    let mut ops = Vec::new();
    for table in tables {
        let kind = tracked_table_kind(table).expect("collected as tracked");
        if keeps(kind, accept) {
            let after = decided
                .iter()
                .find_map(|blocks| find_in(blocks, table.id))
                .unwrap_or(table);
            let mut replacement = after.clone();
            for row in &mut replacement.rows {
                row.properties.row_revision = None;
            }
            ops.push(Operation::ReplaceTable {
                table: table.id,
                replacement: Box::new(replacement),
            });
        } else {
            ops.push(decision(document, table, accept)?);
        }
    }
    Ok(ops)
}

fn find_in(blocks: &[BlockNode], id: NodeId) -> Option<&Table> {
    for block in blocks {
        match block {
            BlockNode::Table(table) if table.id == id => return Some(table),
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        if let Some(found) = find_in(&cell.blocks, id) {
                            return Some(found);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(found) = find_in(&sdt.blocks, id) {
                    return Some(found);
                }
            }
            BlockNode::Paragraph(_) | BlockNode::AltChunk(_) => {}
        }
    }
    None
}

/// Whether `document` holds any whole-table change, so Accept All has something to do.
#[must_use]
pub fn has_tracked_tables(document: &Document) -> bool {
    let mut tables = Vec::new();
    for blocks in surface_block_lists(document) {
        collect_tracked_tables(blocks, &mut tables);
    }
    !tables.is_empty()
}

/// Gives `paragraph` and every inline in it a fresh id, so a copy can sit beside its source.
/// Refuses an inline that is not plain text or a tab: a copied comment range, bookmark,
/// field or picture would have two homes. O(inlines).
fn fresh_copy(
    paragraph: &mut casual_doc_model::v1::Paragraph,
    ids: &mut ReviewIds<'_>,
) -> Result<(), String> {
    let mut mint = || {
        ids.nodes
            .next_id()
            .map_err(|_| "id space exhausted".to_owned())
    };
    paragraph.id = mint()?;
    for inline in &mut paragraph.inlines {
        match inline {
            InlineNode::Run(run) => run.id = mint()?,
            InlineNode::Tab(tab) => tab.id = mint()?,
            _ => {
                return Err(casual_doc_edit::refused!(
                    "review.text-to-table-objects",
                    "These paragraphs hold more than text (a link, field, picture, comment or \
                     bookmark), which a suggested conversion would have to copy into the table \
                     while the original stays struck through. Convert them in Editing mode."
                )
                .to_string());
            }
        }
    }
    Ok(())
}

/// "Convert text to table" as ONE tracked change (`docs/183` §24, RM-52), Word's shape: the
/// table is a tracked insertion (`w:trPr/w:ins` on every row, its cell text `w:ins`) placed
/// BEFORE the source paragraphs, and the source paragraphs stay on the page struck through
/// (their text `w:del`, their paragraph marks deleted up to the paragraph after them), so
/// Reject restores exactly the original paragraphs and Accept leaves only the table.
///
/// `edit` is what `casual_doc_command::table_tools::text_to_table` returned for `sources`
/// (a `DeleteBlocks` of the sources and an `InsertTable` built from their text); the
/// deletion is replaced by the tracked one, and every id the table reused from a source
/// paragraph is minted fresh, since both now stand side by side. O(source inlines + cells).
///
/// # Errors
/// No `InsertTable` in `edit`, sources that already carry tracked changes or hold objects
/// other than text and tabs, an invalid author, or ids exhausted.
pub fn suggest_text_to_table(
    document: &Document,
    edit: Vec<Operation>,
    sources: &[NodeId],
    ids: &mut ReviewIds<'_>,
    author: Option<String>,
    date: Option<String>,
) -> Result<Lowered, String> {
    validate_authored_revision_author(author.as_deref())?;
    let (first, last) = match (sources.first(), sources.last()) {
        (Some(first), Some(last)) => (*first, *last),
        _ => return Err("Select the paragraphs to convert to a table.".to_owned()),
    };
    for node in sources {
        let paragraph = document
            .paragraph(*node)
            .ok_or_else(|| "paragraph not found".to_owned())?;
        if paragraph.properties.mark_revision.is_some()
            || paragraph.properties.prop_change.is_some()
            || paragraph
                .inlines
                .iter()
                .any(|inline| matches!(inline, InlineNode::Revision(_)))
        {
            return Err(casual_doc_edit::refused!(
                "review.text-to-table-tracked",
                "These paragraphs already carry tracked changes. Accept or reject them first."
            )
            .to_string());
        }
    }
    let Some((container, index, mut table)) = edit.into_iter().find_map(|op| match op {
        Operation::InsertTable {
            container,
            index,
            table,
        } => Some((container, index, table)),
        _ => None,
    }) else {
        return Err("a tracked conversion needs a table".to_owned());
    };
    for row in &mut table.rows {
        row.properties.row_revision = Some(MarkRevision {
            kind: MarkRevisionKind::Insertion,
            author: author.clone(),
            date: date.clone(),
            revision_id: Some(ids.revisions.allocate()?),
        });
        for cell in &mut row.cells {
            for block in &mut cell.blocks {
                let BlockNode::Paragraph(paragraph) = block else {
                    continue;
                };
                fresh_copy(paragraph, ids)?;
                if paragraph.inlines.is_empty() {
                    continue;
                }
                let id = ids
                    .nodes
                    .next_id()
                    .map_err(|_| "id space exhausted".to_owned())?;
                let inlines = std::mem::take(&mut paragraph.inlines);
                paragraph.inlines = vec![InlineNode::Revision(Box::new(Revision {
                    id,
                    kind: RevisionKind::Insertion,
                    author: author.clone(),
                    date: date.clone(),
                    revision_id: Some(ids.revisions.allocate()?),
                    editor_group: None,
                    inlines,
                }))];
            }
        }
    }
    // The sources, struck: through the paragraph after them when there is one, so their
    // marks go too and Accept leaves no empty paragraph behind; otherwise up to the end of
    // the last one (a container ends in a paragraph, as in Word).
    let mut nodes = sources.to_vec();
    let end = match crate::body::adjacent_next_paragraph(document, last) {
        Some(next) => {
            nodes.push(next);
            Pos::new(next, 0)
        }
        None => Pos::new(
            last,
            u32::try_from(crate::lower::paragraph_text(document, last).len())
                .map_err(|_| "paragraph too long".to_owned())?,
        ),
    };
    let mut operations = if nodes.len() > 1 {
        crate::lower::suggest_range_edit(
            document,
            ids,
            Pos::new(first, 0),
            end,
            &nodes,
            None,
            author,
            date,
        )?
        .operations
    } else if end.offset > 0 {
        crate::lower::suggest_delete(
            document,
            ids,
            &first.to_string(),
            0,
            end.offset,
            author,
            date,
        )?
        .operations
    } else {
        Vec::new()
    };
    let caret = first_paragraph(&table).unwrap_or(table.id);
    operations.push(Operation::InsertTable {
        container,
        index,
        table,
    });
    Ok(Lowered {
        operations,
        caret: Pos::new(caret, 0),
        group: None,
    })
}
