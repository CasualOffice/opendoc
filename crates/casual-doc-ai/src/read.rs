// SPDX-License-Identifier: Apache-2.0

//! Reading: outline, blocks, styles, tables, review content, search, chunks
//! and validation (`docs/178` §4.1, §4.4). Every list is paged by an opaque
//! cursor bound to the revision it was issued at, and every text-bearing result
//! states its truncation (`docs/178` §5.3).
//!
//! Reading covers the **body** (including table cells and content controls).
//! Headers, footers, notes and text boxes are other surfaces; every result says
//! which surfaces it covered rather than implying the whole document.

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, InlineNode, StyleKind};
use casual_doc_review::body::{
    ReviewCommentSummaryJson, ReviewRevisionSummaryJson, build_review_comment_summaries,
    build_review_revision_summaries,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::{AiError, AiResult};
use crate::session::Session;
use crate::structure::{
    Block, body_blocks, body_paragraphs, find_table, heading_level, style_name,
};
use crate::text::{ESTIMATOR, RunView, clip, estimate_tokens, paragraph_text, runs};

/// The surfaces every read result covers today.
pub const SURFACES: &[&str] = &["body"];

/// Most runs projected for one paragraph.
const MAX_RUNS: usize = 200;

/// An opaque page cursor: `<kind>.<revision>.<index>`. Bound to the revision it
/// was issued at, so a cursor from before an edit is refused rather than
/// silently pointing somewhere else.
pub(crate) fn cursor(kind: &str, revision: u64, index: usize) -> String {
    format!("{kind}.{revision}.{index}")
}

pub(crate) fn parse_cursor(kind: &str, revision: u64, cursor: Option<&str>) -> AiResult<usize> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let mut parts = cursor.split('.');
    let (Some(k), Some(rev), Some(index), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(AiError::invalid(
            "That cursor was not issued by this server.",
        ));
    };
    if k != kind {
        return Err(AiError::invalid("That cursor belongs to a different list."));
    }
    if rev.parse::<u64>().ok() != Some(revision) {
        return Err(AiError::new(
            "stale-cursor",
            "The document changed since this cursor was issued; start the list again.",
        )
        .with_details(json!({ "revision": revision })));
    }
    index
        .parse()
        .map_err(|_| AiError::invalid("That cursor was not issued by this server."))
}

pub(crate) fn node_id(value: &str) -> AiResult<NodeId> {
    value
        .parse()
        .map_err(|_| AiError::invalid(format!("`{}` is not a node id.", clip(value, 64))))
}

/// One outline entry.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineEntry {
    /// The heading paragraph's or table's id: an anchor for `read_blocks`.
    pub node: String,
    /// `heading` or `table`.
    pub kind: &'static str,
    /// Heading level, 1-9.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    /// Heading text, or a table's header row.
    pub text: String,
    /// The headings above this one, outermost first.
    pub path: Vec<String>,
    /// Estimated tokens from this entry to the next entry of the same or a
    /// higher level (the section a `read_blocks` from here returns).
    pub tokens: u32,
    /// For a table: rows and columns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<[usize; 2]>,
}

/// Builds the outline. **O(body blocks)**.
pub fn outline(
    session: &Session,
    depth: Option<u8>,
    cursor_in: Option<&str>,
    limit: Option<usize>,
) -> AiResult<Value> {
    let document = session.document();
    let revision = session.revision();
    let depth = depth.unwrap_or(3).clamp(1, 9);
    let blocks = body_blocks(document);
    // Per-block tokens, so a section's size is a prefix-sum difference.
    let mut tokens = Vec::with_capacity(blocks.len());
    let mut entries = Vec::new();
    let mut path: Vec<(u8, String)> = Vec::new();
    let mut paragraphs = 0usize;
    let mut tables = 0usize;
    for (index, block) in blocks.iter().enumerate() {
        match block {
            Block::Paragraph(paragraph) => {
                paragraphs += 1;
                let text = paragraph_text(paragraph);
                tokens.push(estimate_tokens(&text));
                if let Some(level) = heading_level(document, paragraph) {
                    while path.last().is_some_and(|(at, _)| *at >= level) {
                        path.pop();
                    }
                    if level <= depth {
                        entries.push((
                            index,
                            OutlineEntry {
                                node: paragraph.id.to_string(),
                                kind: "heading",
                                level: Some(level),
                                text: clip(&text, 300).to_owned(),
                                path: path.iter().map(|(_, t)| clip(t, 120).to_owned()).collect(),
                                tokens: 0,
                                dimensions: None,
                            },
                        ));
                    }
                    path.push((level, text));
                }
            }
            Block::Table(table) => {
                tables += 1;
                let header = table
                    .rows
                    .first()
                    .map(|row| {
                        row.cells
                            .iter()
                            .map(|cell| cell_text(&cell.blocks))
                            .collect::<Vec<_>>()
                            .join(" | ")
                    })
                    .unwrap_or_default();
                let mut table_tokens = 0u32;
                for row in &table.rows {
                    for cell in &row.cells {
                        table_tokens =
                            table_tokens.saturating_add(estimate_tokens(&cell_text(&cell.blocks)));
                    }
                }
                tokens.push(table_tokens);
                entries.push((
                    index,
                    OutlineEntry {
                        node: table.id.to_string(),
                        kind: "table",
                        level: None,
                        text: clip(&header, 300).to_owned(),
                        path: path.iter().map(|(_, t)| clip(t, 120).to_owned()).collect(),
                        tokens: table_tokens,
                        dimensions: Some([
                            table.rows.len(),
                            table
                                .rows
                                .iter()
                                .map(|row| row.cells.len())
                                .max()
                                .unwrap_or(0),
                        ]),
                    },
                ));
            }
            Block::Other(_) => tokens.push(1),
        }
    }
    // A heading's section runs to the next heading of the same or higher level.
    let headings: Vec<(usize, u8)> = entries
        .iter()
        .filter_map(|(index, entry)| entry.level.map(|level| (*index, level)))
        .collect();
    let all_headings: Vec<(usize, u8)> = blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| match block {
            Block::Paragraph(paragraph) => heading_level(document, paragraph).map(|l| (index, l)),
            _ => None,
        })
        .collect();
    let mut prefix = vec![0u64; tokens.len() + 1];
    for (index, value) in tokens.iter().enumerate() {
        prefix[index + 1] = prefix[index] + u64::from(*value);
    }
    for (index, level) in headings {
        let end = all_headings
            .iter()
            .find(|(at, l)| *at > index && *l <= level)
            .map_or(blocks.len(), |(at, _)| *at);
        if let Some((_, entry)) = entries.iter_mut().find(|(at, _)| *at == index) {
            entry.tokens = u32::try_from(prefix[end] - prefix[index]).unwrap_or(u32::MAX);
        }
    }
    let start = parse_cursor("outline", revision, cursor_in)?;
    let limit = session.config().limits.page(limit, 100);
    let page: Vec<&OutlineEntry> = entries
        .iter()
        .skip(start)
        .take(limit)
        .map(|(_, e)| e)
        .collect();
    let next = (start + page.len() < entries.len())
        .then(|| cursor("outline", revision, start + page.len()));
    let comments = document.definitions().comments.iter().count();
    let tracked = build_review_revision_summaries(document).len();
    Ok(json!({
        "revision": revision,
        "surfaces": SURFACES,
        "estimator": ESTIMATOR,
        "totalTokens": prefix[tokens.len()],
        "counts": {
            "paragraphs": paragraphs,
            "tables": tables,
            "comments": comments,
            "trackedChanges": tracked,
            "headings": all_headings.len(),
        },
        "entries": page,
        "nextCursor": next,
    }))
}

fn cell_text(blocks: &[BlockNode]) -> String {
    let mut out = Vec::new();
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => out.push(paragraph_text(paragraph)),
            BlockNode::Table(table) => out.push(format!("[nested table {}]", table.id)),
            BlockNode::Sdt(sdt) => out.push(cell_text(&sdt.blocks)),
            BlockNode::AltChunk(_) => {}
        }
    }
    out.join("\n")
}

/// `json` or anchored `markdown`.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReadFormat {
    /// Typed blocks.
    #[default]
    Json,
    /// Markdown with each block prefixed by `{#id}`.
    Markdown,
}

/// Reads blocks in document order from `start` (a node id, or the cursor a
/// previous call returned) until the token ceiling. **O(blocks returned)** plus
/// one walk to find the start.
pub fn read_blocks(
    session: &Session,
    start: Option<&str>,
    cursor_in: Option<&str>,
    max_tokens: Option<u32>,
    format: ReadFormat,
) -> AiResult<Value> {
    let document = session.document();
    let revision = session.revision();
    let budget = session.config().limits.tokens(max_tokens);
    let blocks = body_blocks(document);
    let mut index = parse_cursor("blocks", revision, cursor_in)?;
    if cursor_in.is_none()
        && let Some(start) = start
    {
        let id = node_id(start)?;
        index = blocks
            .iter()
            .position(|block| block.id() == id)
            .or_else(|| {
                // A paragraph inside a table starts at its table.
                blocks.iter().position(|block| match block {
                    Block::Table(table) => table_holds(table, id),
                    _ => false,
                })
            })
            .ok_or_else(|| {
                AiError::new(
                    "anchor-not-found",
                    "No body block has that id at this revision.",
                )
            })?;
    }
    let mut used = 0u32;
    let mut out_json = Vec::new();
    let mut out_md = String::new();
    let mut cursor_at = index;
    while cursor_at < blocks.len() {
        let block = &blocks[cursor_at];
        let (value, markdown, cost) = project_block(session, block);
        if used > 0 && used.saturating_add(cost) > budget {
            break;
        }
        used = used.saturating_add(cost);
        out_json.push(value);
        out_md.push_str(&markdown);
        out_md.push_str("\n\n");
        cursor_at += 1;
        if used >= budget {
            break;
        }
    }
    let truncated = cursor_at < blocks.len();
    let next = truncated.then(|| cursor("blocks", revision, cursor_at));
    let mut result = json!({
        "revision": revision,
        "surfaces": SURFACES,
        "estimator": ESTIMATOR,
        "tokens": used,
        "truncated": truncated,
        "nextCursor": next,
    });
    match format {
        ReadFormat::Json => result["blocks"] = Value::Array(out_json),
        ReadFormat::Markdown => result["markdown"] = Value::String(out_md.trim_end().to_owned()),
    }
    Ok(result)
}

fn table_holds(table: &casual_doc_model::v1::Table, id: NodeId) -> bool {
    table.rows.iter().any(|row| {
        row.cells.iter().any(|cell| {
            cell.id == id
                || cell.blocks.iter().any(|block| match block {
                    BlockNode::Paragraph(paragraph) => paragraph.id == id,
                    BlockNode::Table(inner) => inner.id == id || table_holds(inner, id),
                    BlockNode::Sdt(sdt) => sdt.id == id,
                    BlockNode::AltChunk(_) => false,
                })
        })
    })
}

fn project_block(session: &Session, block: &Block<'_>) -> (Value, String, u32) {
    let document = session.document();
    match block {
        Block::Paragraph(paragraph) => {
            let text = paragraph_text(paragraph);
            let level = heading_level(document, paragraph);
            let runs: Vec<RunView> = runs(paragraph, MAX_RUNS);
            let cost = estimate_tokens(&text);
            let markdown = match level {
                Some(level) => format!(
                    "{{#{}}} {} {}",
                    paragraph.id,
                    "#".repeat(usize::from(level)),
                    text
                ),
                None => format!("{{#{}}} {}", paragraph.id, text),
            };
            (
                json!({
                    "kind": "paragraph",
                    "id": paragraph.id.to_string(),
                    "style": style_name(document, paragraph),
                    "headingLevel": level,
                    "list": paragraph.properties.numbering.is_some(),
                    "text": text,
                    "runs": runs,
                }),
                markdown,
                cost,
            )
        }
        Block::Table(table) => {
            let columns = table
                .rows
                .iter()
                .map(|row| row.cells.len())
                .max()
                .unwrap_or(0);
            let header = table
                .rows
                .first()
                .map(|row| {
                    row.cells
                        .iter()
                        .map(|cell| clip(&cell_text(&cell.blocks), 80).to_owned())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let markdown = format!(
                "{{#{}}} [table: {} rows x {} columns; header: {}; read it with read_table]",
                table.id,
                table.rows.len(),
                columns,
                header.join(" | ")
            );
            let cost = estimate_tokens(&markdown);
            (
                json!({
                    "kind": "table",
                    "id": table.id.to_string(),
                    "rows": table.rows.len(),
                    "columns": columns,
                    "header": header,
                }),
                markdown,
                cost,
            )
        }
        Block::Other(id) => (
            json!({ "kind": "other", "id": id.to_string(), "note": "preserved content this projection does not read" }),
            format!("{{#{id}}} [preserved content]"),
            1,
        ),
    }
}

/// Paragraph and character styles: the ones in use, with how often, and the
/// formatting each declares (not cascaded). **O(body paragraphs + styles)**.
pub fn styles(session: &Session) -> AiResult<Value> {
    let document = session.document();
    let mut used: BTreeMap<NodeId, usize> = BTreeMap::new();
    for (paragraph, _) in body_paragraphs(document) {
        if let Some(style) = paragraph.properties.style_ref {
            *used.entry(style.node_id()).or_default() += 1;
        }
        for inline in &paragraph.inlines {
            if let InlineNode::Run(run) = inline
                && let Some(style) = run.properties.style_ref
            {
                *used.entry(style.node_id()).or_default() += 1;
            }
        }
    }
    let mut out = Vec::new();
    for (id, style) in document.definitions().styles.iter() {
        let kind = match style.kind {
            StyleKind::Paragraph => "paragraph",
            StyleKind::Character => "character",
            _ => continue,
        };
        let run = style.run.as_ref();
        out.push(json!({
            "id": id.node_id().to_string(),
            "name": style.name,
            "kind": kind,
            "basedOn": style.based_on.and_then(|base| document.definitions().styles.get(&base)).and_then(|s| s.name.clone()),
            "default": style.is_default,
            "uses": used.get(&id.node_id()).copied().unwrap_or(0),
            "declared": {
                "bold": run.and_then(|r| r.bold),
                "italic": run.and_then(|r| r.italic),
                "sizePoints": run.and_then(|r| r.size_half_points).map(|h| f64::from(h) / 2.0),
                "headingLevel": style.paragraph.as_ref().and_then(|p| p.outline_level).filter(|l| *l < 9).map(|l| l + 1),
            },
        }));
    }
    Ok(
        json!({ "revision": session.revision(), "styles": out, "note": "`declared` is what the style itself sets, before inheritance." }),
    )
}

/// One table's cells, row-paged, with merged-cell geometry. **O(rows returned)**.
pub fn read_table(
    session: &Session,
    table: &str,
    cursor_in: Option<&str>,
    rows: Option<usize>,
) -> AiResult<Value> {
    let document = session.document();
    let revision = session.revision();
    let id = node_id(table)?;
    let table = find_table(document, id).ok_or_else(|| {
        AiError::new("anchor-not-found", "No table has that id at this revision.")
    })?;
    let start = parse_cursor("table", revision, cursor_in)?;
    let limit = session.config().limits.page(rows, 50);
    let mut out = Vec::new();
    for (index, row) in table.rows.iter().enumerate().skip(start).take(limit) {
        let cells: Vec<Value> = row
            .cells
            .iter()
            .enumerate()
            .map(|(column, cell)| {
                let first = cell.blocks.iter().find_map(|block| match block {
                    BlockNode::Paragraph(paragraph) => Some(paragraph.id.to_string()),
                    _ => None,
                });
                json!({
                    "column": column,
                    "text": cell_text(&cell.blocks),
                    "paragraph": first,
                    "gridSpan": cell.properties.grid_span.unwrap_or(1),
                    "verticalMerge": cell.properties.vertical_merge.map(|merge| format!("{merge:?}").to_ascii_lowercase()),
                })
            })
            .collect();
        out.push(json!({ "row": index, "cells": cells }));
    }
    let end = start + out.len();
    Ok(json!({
        "revision": revision,
        "table": id.to_string(),
        "rowCount": table.rows.len(),
        "rows": out,
        "nextCursor": (end < table.rows.len()).then(|| cursor("table", revision, end)),
    }))
}

/// Comment threads, paged. **O(comments + document)** for the anchor walk.
pub fn list_comments(
    session: &Session,
    cursor_in: Option<&str>,
    limit: Option<usize>,
) -> AiResult<Value> {
    let revision = session.revision();
    let all: Vec<ReviewCommentSummaryJson> = build_review_comment_summaries(session.document());
    let start = parse_cursor("comments", revision, cursor_in)?;
    let limit = session.config().limits.page(limit, 50);
    let page: Vec<&ReviewCommentSummaryJson> = all.iter().skip(start).take(limit).collect();
    let end = start + page.len();
    Ok(json!({
        "revision": revision,
        "total": all.len(),
        "comments": page,
        "nextCursor": (end < all.len()).then(|| cursor("comments", revision, end)),
    }))
}

/// Tracked changes, paged, in document order. **O(document)**.
pub fn list_tracked_changes(
    session: &Session,
    cursor_in: Option<&str>,
    limit: Option<usize>,
) -> AiResult<Value> {
    let revision = session.revision();
    let all: Vec<ReviewRevisionSummaryJson> = build_review_revision_summaries(session.document());
    let start = parse_cursor("changes", revision, cursor_in)?;
    let limit = session.config().limits.page(limit, 50);
    let page: Vec<&ReviewRevisionSummaryJson> = all.iter().skip(start).take(limit).collect();
    let end = start + page.len();
    Ok(json!({
        "revision": revision,
        "surfaces": "all",
        "total": all.len(),
        "trackedChanges": page,
        "nextCursor": (end < all.len()).then(|| cursor("changes", revision, end)),
    }))
}

/// How `search` matches.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    /// Case-sensitive substring.
    #[default]
    Exact,
    /// Case-insensitive substring.
    Insensitive,
    /// Case-insensitive, on word boundaries.
    Words,
}

fn find_all(text: &str, query: &str, mode: SearchMode) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let (hay, needle) = match mode {
        SearchMode::Exact => (text.to_owned(), query.to_owned()),
        // ASCII folding keeps byte offsets identical to `text`'s, so a hit maps
        // back to the paragraph exactly. Non-ASCII case pairs are matched
        // case-sensitively; stated in the tool description.
        SearchMode::Insensitive | SearchMode::Words => {
            (text.to_ascii_lowercase(), query.to_ascii_lowercase())
        }
    };
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = hay[from..].find(&needle) {
        let start = from + found;
        let end = start + needle.len();
        let boundary = |at: Option<char>| at.is_none_or(|c| !c.is_alphanumeric());
        if mode != SearchMode::Words
            || (boundary(hay[..start].chars().next_back()) && boundary(hay[end..].chars().next()))
        {
            out.push((start, end));
        }
        from = start + needle.chars().next().map_or(1, char::len_utf8);
    }
    out
}

/// Search with anchored hits and a bounded context window. **O(body text)**.
pub fn search(
    session: &Session,
    query: &str,
    mode: SearchMode,
    cursor_in: Option<&str>,
    limit: Option<usize>,
) -> AiResult<Value> {
    if query.is_empty() || query.len() > 1_000 {
        return Err(AiError::invalid("The query must be 1 to 1000 bytes."));
    }
    let document = session.document();
    let revision = session.revision();
    let start = parse_cursor("search", revision, cursor_in)?;
    let limit = session.config().limits.page(limit, 20);
    let mut hits = Vec::new();
    let mut total = 0usize;
    for (paragraph, table) in body_paragraphs(document) {
        let text = paragraph_text(paragraph);
        for (from, to) in find_all(&text, query, mode) {
            if total >= start && hits.len() < limit {
                let before_start = floor_boundary(&text, from.saturating_sub(60));
                let after_end = ceil_boundary(&text, (to + 60).min(text.len()));
                hits.push(json!({
                    "node": paragraph.id.to_string(),
                    "table": table.map(|t| t.to_string()),
                    "start": from,
                    "end": to,
                    "exact": &text[from..to],
                    "prefix": &text[before_start..from],
                    "suffix": &text[to..after_end],
                }));
            }
            total += 1;
        }
    }
    let end = start + hits.len();
    Ok(json!({
        "revision": revision,
        "surfaces": SURFACES,
        "total": total,
        "hits": hits,
        "nextCursor": (end < total).then(|| cursor("search", revision, end)),
    }))
}

fn floor_boundary(text: &str, mut at: usize) -> usize {
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn ceil_boundary(text: &str, mut at: usize) -> usize {
    while at < text.len() && !text.is_char_boundary(at) {
        at += 1;
    }
    at
}

/// The chunker's version: part of every chunk id, so a change to how chunks
/// are cut changes every id instead of silently reusing stale vectors.
pub const CHUNKER_VERSION: u32 = 1;

fn fnv1a(parts: &[&[u8]]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in *part {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Structure-aware chunks for retrieval (`docs/178` §4.4): a heading and the
/// paragraphs after it up to the ceiling, a table's header with a bounded run
/// of rows, and each comment thread with its anchor. Ids are deterministic: a
/// hash of the chunker version, the ordered anchors and the text.
/// **O(document)**; paged.
pub fn chunks(
    session: &Session,
    max_tokens: Option<u32>,
    cursor_in: Option<&str>,
    limit: Option<usize>,
) -> AiResult<Value> {
    let document = session.document();
    let revision = session.revision();
    let ceiling = max_tokens
        .unwrap_or(400)
        .clamp(32, session.config().limits.max_tokens_ceiling);
    let mut out: Vec<Value> = Vec::new();
    let mut path: Vec<(u8, String)> = Vec::new();
    let mut current: (Vec<String>, String, u32) = (Vec::new(), String::new(), 0);
    let flush = |current: &mut (Vec<String>, String, u32),
                 path: &[(u8, String)],
                 out: &mut Vec<Value>,
                 kind: &str| {
        if current.0.is_empty() {
            return;
        }
        let anchors = std::mem::take(&mut current.0);
        let text = std::mem::take(&mut current.1);
        let tokens = std::mem::take(&mut current.2);
        let joined = anchors.join(",");
        let id = fnv1a(&[
            &CHUNKER_VERSION.to_le_bytes(),
            joined.as_bytes(),
            text.as_bytes(),
        ]);
        out.push(json!({
            "id": format!("c{id:016x}"),
            "kind": kind,
            "anchors": anchors,
            "headingPath": path.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>(),
            "text": text,
            "tokens": tokens,
            "surface": "body",
        }));
    };
    for block in body_blocks(document) {
        match block {
            Block::Paragraph(paragraph) => {
                let text = paragraph_text(paragraph);
                let cost = estimate_tokens(&text);
                if let Some(level) = heading_level(document, paragraph) {
                    flush(&mut current, &path, &mut out, "section");
                    while path.last().is_some_and(|(at, _)| *at >= level) {
                        path.pop();
                    }
                    path.push((level, clip(&text, 200).to_owned()));
                } else if current.2.saturating_add(cost) > ceiling {
                    flush(&mut current, &path, &mut out, "section");
                }
                if text.is_empty() {
                    continue;
                }
                current.0.push(paragraph.id.to_string());
                if !current.1.is_empty() {
                    current.1.push('\n');
                }
                // A single paragraph longer than the ceiling is cut, and says so.
                let room = (ceiling as usize).saturating_mul(4);
                current.1.push_str(clip(&text, room));
                current.2 = current.2.saturating_add(cost.min(ceiling));
            }
            Block::Table(table) => {
                flush(&mut current, &path, &mut out, "section");
                let header = table
                    .rows
                    .first()
                    .map(|row| {
                        row.cells
                            .iter()
                            .map(|cell| cell_text(&cell.blocks))
                            .collect::<Vec<_>>()
                            .join(" | ")
                    })
                    .unwrap_or_default();
                let mut rows = (
                    vec![table.id.to_string()],
                    header.clone(),
                    estimate_tokens(&header),
                );
                for row in table.rows.iter().skip(1) {
                    let line = row
                        .cells
                        .iter()
                        .map(|cell| cell_text(&cell.blocks))
                        .collect::<Vec<_>>()
                        .join(" | ");
                    let cost = estimate_tokens(&line);
                    if rows.2.saturating_add(cost) > ceiling {
                        flush(&mut rows, &path, &mut out, "table");
                        rows = (
                            vec![table.id.to_string()],
                            header.clone(),
                            estimate_tokens(&header),
                        );
                    }
                    rows.0.push(row.id.to_string());
                    rows.1.push('\n');
                    rows.1
                        .push_str(clip(&line, (ceiling as usize).saturating_mul(4)));
                    rows.2 = rows.2.saturating_add(cost.min(ceiling));
                }
                flush(&mut rows, &path, &mut out, "table");
            }
            Block::Other(_) => {}
        }
    }
    flush(&mut current, &path, &mut out, "section");
    for comment in build_review_comment_summaries(document) {
        let mut anchors = vec![comment.id.clone()];
        if let Some(anchor) = &comment.anchor {
            anchors.push(anchor.node.clone());
        }
        let text = clip(&comment.text, (ceiling as usize).saturating_mul(4)).to_owned();
        let id = fnv1a(&[
            &CHUNKER_VERSION.to_le_bytes(),
            anchors.join(",").as_bytes(),
            text.as_bytes(),
        ]);
        out.push(json!({
            "id": format!("c{id:016x}"),
            "kind": "comment",
            "anchors": anchors,
            "author": comment.author,
            "text": text,
            "tokens": estimate_tokens(&text),
            "surface": "comments",
        }));
    }
    let start = parse_cursor("chunks", revision, cursor_in)?;
    let limit = session.config().limits.page(limit, 50);
    let total = out.len();
    let page: Vec<Value> = out.into_iter().skip(start).take(limit).collect();
    let end = start + page.len();
    Ok(json!({
        "revision": revision,
        "chunkerVersion": CHUNKER_VERSION,
        "estimator": ESTIMATOR,
        "surfaces": ["body", "comments"],
        "total": total,
        "chunks": page,
        "nextCursor": (end < total).then(|| cursor("chunks", revision, end)),
    }))
}

/// Model invariants, broken references and protection state. **O(document)**.
pub fn validate(session: &Session) -> AiResult<Value> {
    let document = session.document();
    let mut problems: Vec<Value> = Vec::new();
    if let Err(error) = document.validate() {
        problems.push(json!({ "code": "model-invariant", "message": error.to_string() }));
    }
    // Comment and note references against their definitions, on every surface.
    let mut referenced_comments = BTreeSet::new();
    let mut missing_notes = 0usize;
    let definitions = document.definitions();
    fn walk(
        inlines: &[InlineNode],
        comments: &mut BTreeSet<NodeId>,
        missing_notes: &mut usize,
        definitions: &casual_doc_model::v1::Definitions,
    ) {
        for inline in inlines {
            // container-set: Hyperlink, Field, Revision and Sdt hold this
            // paragraph's references; TextBox and Group stories are walked as
            // surfaces of their own by `surface_block_lists`.
            match inline {
                InlineNode::CommentRangeStart(marker) => {
                    comments.insert(marker.comment.node_id());
                }
                InlineNode::CommentReference(marker) => {
                    comments.insert(marker.comment.node_id());
                }
                InlineNode::NoteReference(reference) => {
                    let map = match reference.kind {
                        casual_doc_model::v1::NoteKind::Footnote => &definitions.footnotes,
                        casual_doc_model::v1::NoteKind::Endnote => &definitions.endnotes,
                    };
                    if map.get(&reference.note).is_none() {
                        *missing_notes += 1;
                    }
                }
                InlineNode::Hyperlink(link) => {
                    walk(&link.inlines, comments, missing_notes, definitions)
                }
                InlineNode::Field(field) => {
                    walk(&field.inlines, comments, missing_notes, definitions)
                }
                InlineNode::Revision(revision) => {
                    walk(&revision.inlines, comments, missing_notes, definitions);
                }
                InlineNode::Sdt(sdt) => walk(&sdt.inlines, comments, missing_notes, definitions),
                _ => {}
            }
        }
    }
    for blocks in casual_doc_edit::surface_block_lists(document) {
        casual_doc_review::body::visit_paragraphs(blocks, &mut |paragraph| {
            walk(
                &paragraph.inlines,
                &mut referenced_comments,
                &mut missing_notes,
                definitions,
            );
        });
    }
    let defined: BTreeSet<NodeId> = definitions
        .comments
        .iter()
        .map(|(id, _)| id.node_id())
        .collect();
    let dangling = referenced_comments.difference(&defined).count();
    if dangling > 0 {
        problems.push(
            json!({ "code": "comment-reference-without-definition", "occurrences": dangling }),
        );
    }
    if missing_notes > 0 {
        problems.push(
            json!({ "code": "note-reference-without-definition", "occurrences": missing_notes }),
        );
    }
    // A reply is anchored by its thread; only roots need markers.
    let unanchored = definitions
        .comments
        .iter()
        .filter(|(id, comment)| {
            comment.parent_para_id.is_none() && !referenced_comments.contains(&id.node_id())
        })
        .count();
    if unanchored > 0 {
        problems.push(json!({ "code": "comment-without-anchor", "occurrences": unanchored, "severity": "warning" }));
    }
    let protection = definitions
        .settings
        .document_protection
        .as_ref()
        .map(|p| json!({ "enforced": p.enforcement, "edit": format!("{:?}", p.edit) }));
    Ok(json!({
        "revision": session.revision(),
        "valid": problems.iter().all(|p| p.get("severity").is_some()),
        "problems": problems,
        "protection": protection,
        "trackRevisions": definitions.settings.track_changes,
    }))
}
