// SPDX-License-Identifier: Apache-2.0

//! Reading: outline, blocks, styles, tables, review content, search, chunks
//! and validation (`docs/178` §4.1, §4.4). Every list is paged by an opaque
//! cursor bound to the revision it was issued at, and every text-bearing result
//! states its truncation (`docs/178` §5.3).
//!
//! Every surface is readable (`docs/181` R2): the body, headers, footers,
//! footnotes, endnotes, comment bodies and text boxes ([`Surface`]).
//! `get_outline` lists them all; `read_blocks`, `search` and `chunks` take one
//! (the body by default) and `search`/`chunks` can take all of them. Every
//! result names the surfaces it covered rather than implying the whole
//! document, and counts what it names but does not show (`omitted`).

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, Document, InlineNode, StyleKind};
use casual_doc_review::body::{
    ReviewCommentSummaryJson, ReviewRevisionSummaryJson, build_review_comment_summaries,
    build_review_revision_summaries,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::{AiError, AiResult};
use crate::project::{self, Omitted};
use crate::session::Session;
use crate::structure::{
    Block, blocks_in, body_paragraphs, find_table, heading_level, paragraphs_in, style_name,
};
use crate::surface::{self, Surface};
use crate::text::{ESTIMATOR, RunView, clip, estimate_tokens, paragraph_text, runs};

/// Most runs projected for one paragraph.
const MAX_RUNS: usize = 200;

/// Most `omitted` items listed in one result; the rest are counted.
const MAX_OMITTED: usize = 50;

/// Why a comment body's paragraphs are not edit targets.
const COMMENT_READ_ONLY: &str =
    "a comment's body changes through the comment and resolve_comment edits, not by anchor";

/// Which surfaces a `search` or `chunks` call covers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scope {
    /// One surface.
    One(Surface),
    /// Every surface the document has.
    All,
}

impl Default for Scope {
    fn default() -> Self {
        Self::One(Surface::Body)
    }
}

impl Scope {
    /// `None` is the body; `all` is every surface; anything else is one
    /// surface as [`Surface::parse`] reads it.
    ///
    /// # Errors
    ///
    /// `invalid-argument` for a malformed surface.
    pub fn parse(value: Option<&str>) -> AiResult<Self> {
        match value {
            None => Ok(Self::default()),
            Some("all") => Ok(Self::All),
            Some(one) => Surface::parse(one).map(Self::One),
        }
    }

    fn tag(self) -> String {
        match self {
            Self::One(Surface::Body) => String::new(),
            Self::One(surface) => format!("~{surface}"),
            Self::All => "~all".to_owned(),
        }
    }

    fn covered(self) -> Value {
        match self {
            Self::One(surface) => json!([surface.to_string()]),
            Self::All => json!("all"),
        }
    }

    /// The surfaces in scope with their blocks. O(1) lookups for one named
    /// surface (O(document) for a text box), O(document) for all.
    fn entries(self, document: &Document) -> AiResult<Vec<(Surface, &[BlockNode])>> {
        match self {
            Self::One(surface) => Ok(vec![(surface, surface::blocks_of(document, surface)?)]),
            Self::All => Ok(surface::enumerate(document)
                .into_iter()
                .map(|entry| (entry.surface, entry.blocks))
                .collect()),
        }
    }
}

/// The `omitted` field of a result: how many named things it did not show,
/// and the first `MAX_OMITTED` of them.
fn omitted_value(items: &[Omitted]) -> Value {
    json!({
        "count": items.len(),
        "items": &items[..items.len().min(MAX_OMITTED)],
    })
}

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

/// Builds the outline: the body's headings and tables, then every surface the
/// document has ([`Surface`]) with its ids, kind, where it shows (section and
/// page type for a header or footer, the referencing paragraph for a note, the
/// host paragraph for a text box) and its estimated tokens, plus the
/// bookmarks and the document-wide counts. Both lists are paged: `cursor` for
/// the headings, `surfaces_cursor` for the surfaces. **O(document)**, one walk.
pub fn outline(
    session: &Session,
    depth: Option<u8>,
    cursor_in: Option<&str>,
    limit: Option<usize>,
) -> AiResult<Value> {
    outline_paged(session, depth, cursor_in, limit, None)
}

/// [`outline`] with a cursor for the surface list too (its
/// `surfacesNextCursor`). **O(document)**.
pub fn outline_paged(
    session: &Session,
    depth: Option<u8>,
    cursor_in: Option<&str>,
    limit: Option<usize>,
    surfaces_cursor: Option<&str>,
) -> AiResult<Value> {
    let document = session.document();
    let revision = session.revision();
    let depth = depth.unwrap_or(3).clamp(1, 9);
    let blocks = blocks_in(document.body());
    // Per-block tokens, so a section's size is a prefix-sum difference.
    let mut tokens = Vec::with_capacity(blocks.len());
    let mut entries = Vec::new();
    let mut path: Vec<(u8, String)> = Vec::new();
    let mut paragraphs = 0usize;
    let mut tables = 0usize;
    for (index, placed) in blocks.iter().enumerate() {
        match &placed.block {
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
            Block::ContentControl(_) | Block::Opaque(..) => tokens.push(1),
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
        .filter_map(|(index, placed)| match placed.block {
            Block::Paragraph(paragraph) => heading_level(document, paragraph).map(|l| (index, l)),
            Block::Table(_) | Block::ContentControl(_) | Block::Opaque(..) => None,
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
    let map = surface_map(document);
    let surface_start = parse_cursor("surfaces", revision, surfaces_cursor)?;
    let surface_page: Vec<&Value> = map
        .surfaces
        .iter()
        .skip(surface_start)
        .take(limit)
        .collect();
    let surface_end = surface_start + surface_page.len();
    let kinds = |kind: &str| {
        map.surfaces
            .iter()
            .filter(|entry| entry["kind"] == kind)
            .count()
    };
    Ok(json!({
        "revision": revision,
        "estimator": ESTIMATOR,
        "totalTokens": prefix[tokens.len()],
        "counts": {
            "paragraphs": paragraphs,
            "tables": tables,
            "comments": comments,
            "trackedChanges": tracked,
            "headings": all_headings.len(),
            "headers": kinds("header"),
            "footers": kinds("footer"),
            "footnotes": kinds("footnote"),
            "endnotes": kinds("endnote"),
            "textBoxes": kinds("text_box"),
            "bookmarks": map.bookmarks.len(),
            "links": map.links,
            "fields": map.fields,
            "objects": map.objects,
            "contentControls": map.controls,
        },
        "entries": page,
        "nextCursor": next,
        "surfaceCount": map.surfaces.len(),
        "surfaces": surface_page,
        "surfacesNextCursor": (surface_end < map.surfaces.len())
            .then(|| cursor("surfaces", revision, surface_end)),
        "bookmarks": &map.bookmarks[..map.bookmarks.len().min(MAX_BOOKMARKS)],
        "bookmarksTruncated": map.bookmarks.len() > MAX_BOOKMARKS,
        "omitted": omitted_value(&map.omitted),
    }))
}

/// Most bookmarks listed by the outline; the rest are counted.
const MAX_BOOKMARKS: usize = 200;

/// Every surface with its facts, built in one walk.
struct SurfaceMap {
    surfaces: Vec<Value>,
    bookmarks: Vec<Value>,
    omitted: Vec<Omitted>,
    links: usize,
    fields: usize,
    objects: usize,
    controls: usize,
}

/// One pass over every paragraph of every surface: per-surface sizes and a
/// preview, where each note is referenced from, the bookmarks, and what is
/// named but not shown. **O(document)**: each surface's blocks are walked
/// once, and each paragraph's inlines once (`project::paragraph_facts`).
fn surface_map(document: &Document) -> SurfaceMap {
    let definitions = document.definitions();
    let entries = surface::enumerate(document);
    let mut referenced: BTreeMap<String, (NodeId, Surface)> = BTreeMap::new();
    let mut bookmarks = Vec::new();
    let mut omitted = Vec::new();
    let (mut links, mut fields, mut objects, mut controls) = (0, 0, 0, 0);
    let mut stats = Vec::with_capacity(entries.len());
    for entry in &entries {
        let mut paragraphs = 0usize;
        let mut tokens = 0u64;
        let mut preview = String::new();
        for (paragraph, _) in paragraphs_in(entry.blocks) {
            paragraphs += 1;
            let text = paragraph_text(paragraph);
            tokens += u64::from(estimate_tokens(&text));
            if preview.is_empty() && !text.trim().is_empty() {
                clip(text.trim(), 160).clone_into(&mut preview);
            }
            let facts = project::paragraph_facts(definitions, paragraph);
            links += facts.links.len();
            fields += facts.fields.len();
            objects += facts.objects.len();
            controls += facts.controls.len();
            for note in &facts.notes {
                if let Some(id) = note["note"].as_str() {
                    referenced
                        .entry(id.to_owned())
                        .or_insert((paragraph.id, entry.surface));
                }
            }
            for mut bookmark in facts.bookmarks {
                bookmark["paragraph"] = json!(paragraph.id.to_string());
                bookmark["surface"] = json!(entry.surface.to_string());
                bookmarks.push(bookmark);
            }
            omitted.extend(facts.omitted);
        }
        let mut tables = 0usize;
        for placed in blocks_in(entry.blocks) {
            match placed.block {
                Block::Table(_) => tables += 1,
                Block::ContentControl(_) => controls += 1,
                Block::Opaque(_, item) => omitted.push(item),
                Block::Paragraph(_) => {}
            }
        }
        stats.push((paragraphs, tables, tokens, preview));
    }
    let surfaces = entries
        .iter()
        .zip(stats)
        .map(|(entry, (paragraphs, tables, tokens, preview))| {
            let mut value = json!({
                "surface": entry.surface.to_string(),
                "kind": entry.surface.kind(),
                "id": entry.surface.id().map(|id| id.to_string()),
                "paragraphs": paragraphs,
                "tables": tables,
                "tokens": tokens,
                "preview": preview,
            });
            if !entry.uses.is_empty() {
                value["uses"] = entry
                    .uses
                    .iter()
                    .map(|u| json!({ "section": u.section, "page": u.page }))
                    .collect();
            }
            if let Some((host, paragraph)) = entry.host {
                value["host"] =
                    json!({ "surface": host.to_string(), "paragraph": paragraph.to_string() });
            }
            match entry.surface {
                Surface::Footnote(id) | Surface::Endnote(id) => {
                    value["referencedFrom"] =
                        referenced
                            .get(&id.to_string())
                            .map_or(Value::Null, |(paragraph, surface)| {
                                json!({ "surface": surface.to_string(), "paragraph": paragraph.to_string() })
                            });
                }
                Surface::Comment(id) => {
                    value["author"] = json!(
                        definitions
                            .comments
                            .get(&casual_doc_model::v1::CommentId::new(id))
                            .and_then(|comment| comment.author.clone())
                    );
                    value["editable"] = json!(false);
                    value["reason"] = json!(COMMENT_READ_ONLY);
                }
                Surface::Body | Surface::Header(_) | Surface::Footer(_) | Surface::TextBox(_) => {}
            }
            value
        })
        .collect();
    SurfaceMap {
        surfaces,
        bookmarks,
        omitted,
        links,
        fields,
        objects,
        controls,
    }
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

/// Reads one surface's blocks in order from `start` (a node id, or the cursor a
/// previous call returned) until the token ceiling.
///
/// The surface is `surface` when given; otherwise the one the cursor was
/// issued for; otherwise the one holding `start` (so a paragraph id found by
/// `search` or the outline reads in place, whatever surface it is on);
/// otherwise the body. **O(blocks returned)** plus one walk of the surface to
/// find the start (and, with only a `start` from another surface, one
/// O(document) walk to find which surface holds it).
pub fn read_blocks(
    session: &Session,
    start: Option<&str>,
    cursor_in: Option<&str>,
    max_tokens: Option<u32>,
    format: ReadFormat,
) -> AiResult<Value> {
    read_blocks_in(session, None, start, cursor_in, max_tokens, format)
}

/// [`read_blocks`] on a named surface (`None` picks it as `read_blocks`
/// describes). **O(blocks returned)** plus the walks `read_blocks` names.
///
/// # Errors
///
/// `surface-not-found`, `anchor-not-found`, `stale-cursor`, or
/// `invalid-argument` for a cursor of another list or another surface.
pub fn read_blocks_in(
    session: &Session,
    surface: Option<Surface>,
    start: Option<&str>,
    cursor_in: Option<&str>,
    max_tokens: Option<u32>,
    format: ReadFormat,
) -> AiResult<Value> {
    let document = session.document();
    let revision = session.revision();
    let budget = session.config().limits.tokens(max_tokens);
    let from_cursor = cursor_in.map(cursor_surface).transpose()?;
    let start_id = match (cursor_in, start) {
        (None, Some(start)) => Some(node_id(start)?),
        _ => None,
    };
    let surface = match (surface, from_cursor) {
        (Some(given), Some(issued)) if given != issued => {
            return Err(AiError::invalid(
                "That cursor belongs to a different surface's list.",
            ));
        }
        (Some(given), _) => given,
        (None, Some(issued)) => issued,
        (None, None) => match start_id {
            Some(id) if !surface::holds(document.body(), id) => {
                surface::surface_holding(document, id).unwrap_or(Surface::Body)
            }
            _ => Surface::Body,
        },
    };
    let blocks = blocks_in(surface::blocks_of(document, surface)?);
    let kind = blocks_kind(surface);
    let mut index = parse_cursor(&kind, revision, cursor_in)?;
    if let Some(id) = start_id {
        index = blocks
            .iter()
            .position(|placed| placed.block.id() == id)
            .or_else(|| {
                // A paragraph inside a table starts at its table.
                blocks.iter().position(|placed| match placed.block {
                    Block::Table(table) => table_holds(table, id),
                    Block::Paragraph(_) | Block::ContentControl(_) | Block::Opaque(..) => false,
                })
            })
            .ok_or_else(|| {
                AiError::new(
                    "anchor-not-found",
                    "No block of that surface has that id at this revision.",
                )
                .with_details(json!({ "surface": surface.to_string() }))
            })?;
    }
    let definitions = document.definitions();
    let mut used = 0u32;
    let mut out_json = Vec::new();
    let mut out_md = String::new();
    let mut omitted = Vec::new();
    let mut cursor_at = index;
    while cursor_at < blocks.len() {
        let placed = &blocks[cursor_at];
        let projected = project_block(document, definitions, placed);
        if used > 0 && used.saturating_add(projected.cost) > budget {
            break;
        }
        used = used.saturating_add(projected.cost);
        out_json.push(projected.value);
        out_md.push_str(&projected.markdown);
        out_md.push_str("\n\n");
        omitted.extend(projected.omitted);
        cursor_at += 1;
        if used >= budget {
            break;
        }
    }
    let truncated = cursor_at < blocks.len();
    let next = truncated.then(|| cursor(&kind, revision, cursor_at));
    let mut result = json!({
        "revision": revision,
        "surface": surface.to_string(),
        "surfaces": [surface.to_string()],
        "estimator": ESTIMATOR,
        "tokens": used,
        "truncated": truncated,
        "nextCursor": next,
        "omitted": omitted_value(&omitted),
    });
    if matches!(surface, Surface::Comment(_)) {
        result["editable"] = json!(false);
        result["reason"] = json!(COMMENT_READ_ONLY);
    }
    match format {
        ReadFormat::Json => result["blocks"] = Value::Array(out_json),
        ReadFormat::Markdown => result["markdown"] = Value::String(out_md.trim_end().to_owned()),
    }
    Ok(result)
}

/// The cursor kind of one surface's block list. The body keeps the bare
/// `blocks` it always had; another surface's kind names it, so a cursor can
/// never page a different surface than the one it was issued for.
fn blocks_kind(surface: Surface) -> String {
    match surface {
        Surface::Body => "blocks".to_owned(),
        other => format!("blocks~{other}"),
    }
}

/// The surface a `read_blocks` cursor was issued for.
fn cursor_surface(cursor: &str) -> AiResult<Surface> {
    let kind = cursor.split('.').next().unwrap_or_default();
    match kind.strip_prefix("blocks") {
        Some("") => Ok(Surface::Body),
        Some(rest) => rest
            .strip_prefix('~')
            .ok_or_else(|| AiError::invalid("That cursor was not issued by this server."))
            .and_then(Surface::parse),
        None => Err(AiError::invalid("That cursor belongs to a different list.")),
    }
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

/// One block as a read returns it.
struct Projected {
    value: Value,
    markdown: String,
    cost: u32,
    omitted: Vec<Omitted>,
}

/// Projects one block. The `match` on [`Block`] has no wildcard, and `Block`
/// is built from `BlockNode` by a wildcard-free match (`docs/181` G2), so a
/// new block kind is classified here or does not compile. **O(block)**; a
/// table is summarised (rows, columns, header), not walked cell by cell.
fn project_block(
    document: &Document,
    definitions: &casual_doc_model::v1::Definitions,
    placed: &crate::structure::Placed<'_>,
) -> Projected {
    let mut projected = match &placed.block {
        Block::Paragraph(paragraph) => {
            let text = paragraph_text(paragraph);
            let level = heading_level(document, paragraph);
            let runs: Vec<RunView> = runs(paragraph, MAX_RUNS);
            let facts = project::paragraph_facts(definitions, paragraph);
            let mut markdown = match level {
                Some(level) => format!(
                    "{{#{}}} {} {}",
                    paragraph.id,
                    "#".repeat(usize::from(level)),
                    text
                ),
                None => format!("{{#{}}} {}", paragraph.id, text),
            };
            let trailer = facts.markdown();
            let cost = estimate_tokens(&text).saturating_add(if trailer.is_empty() {
                0
            } else {
                estimate_tokens(&trailer)
            });
            markdown.push_str(&trailer);
            let mut value = json!({
                "kind": "paragraph",
                "id": paragraph.id.to_string(),
                "style": style_name(document, paragraph),
                "headingLevel": level,
                "list": paragraph.properties.numbering.is_some(),
                "text": text,
                "runs": runs,
            });
            facts.write_into(&mut value);
            Projected {
                value,
                markdown,
                cost,
                omitted: facts.omitted,
            }
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
            Projected {
                value: json!({
                    "kind": "table",
                    "id": table.id.to_string(),
                    "rows": table.rows.len(),
                    "columns": columns,
                    "header": header,
                }),
                markdown,
                cost,
                omitted: Vec::new(),
            }
        }
        Block::ContentControl(sdt) => {
            let mut value = project::block_control(sdt);
            value["kind"] = json!("content_control");
            value["blocks"] = json!(sdt.blocks.len());
            let markdown = format!(
                "{{#{}}} [content control {} alias={:?} tag={:?}; its blocks follow]",
                sdt.id,
                value["type"].as_str().unwrap_or("rich_text"),
                sdt.properties.alias.as_deref().unwrap_or(""),
                sdt.properties.tag.as_deref().unwrap_or("")
            );
            let cost = estimate_tokens(&markdown);
            Projected {
                value,
                markdown,
                cost,
                omitted: Vec::new(),
            }
        }
        Block::Opaque(id, omitted) => Projected {
            value: json!({
                "kind": "opaque",
                "id": id.to_string(),
                "node": omitted.kind,
                "reason": omitted.reason,
            }),
            markdown: format!("{{#{id}}} [{} not shown: {}]", omitted.kind, omitted.reason),
            cost: 1,
            omitted: vec![omitted.clone()],
        },
    };
    if let Some(control) = placed.control {
        projected.value["contentControl"] = json!(control.to_string());
    }
    projected
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
        "surface": surface::surface_holding(document, id).map(|s| s.to_string()),
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

/// Search with anchored hits and a bounded context window, over one surface
/// (the body by default) or every surface (`Scope::All`). Each hit names its
/// surface, and its `node` resolves as an anchor wherever it is.
/// **O(text of the surfaces searched)**.
pub fn search(
    session: &Session,
    query: &str,
    mode: SearchMode,
    cursor_in: Option<&str>,
    limit: Option<usize>,
) -> AiResult<Value> {
    search_in(session, query, mode, Scope::default(), cursor_in, limit)
}

/// [`search`] over a [`Scope`]: one surface or all of them.
/// **O(text of the surfaces searched)**.
///
/// # Errors
///
/// `invalid-argument` for an empty or over-long query, `surface-not-found`,
/// `stale-cursor`.
pub fn search_in(
    session: &Session,
    query: &str,
    mode: SearchMode,
    scope: Scope,
    cursor_in: Option<&str>,
    limit: Option<usize>,
) -> AiResult<Value> {
    if query.is_empty() || query.len() > 1_000 {
        return Err(AiError::invalid("The query must be 1 to 1000 bytes."));
    }
    let document = session.document();
    let revision = session.revision();
    let kind = format!("search{}", scope.tag());
    let start = parse_cursor(&kind, revision, cursor_in)?;
    let limit = session.config().limits.page(limit, 20);
    let mut hits = Vec::new();
    let mut total = 0usize;
    for (surface, blocks) in scope.entries(document)? {
        for (paragraph, table) in paragraphs_in(blocks) {
            let text = paragraph_text(paragraph);
            for (from, to) in find_all(&text, query, mode) {
                if total >= start && hits.len() < limit {
                    let before_start = floor_boundary(&text, from.saturating_sub(60));
                    let after_end = ceil_boundary(&text, (to + 60).min(text.len()));
                    hits.push(json!({
                        "node": paragraph.id.to_string(),
                        "surface": surface.to_string(),
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
    }
    let end = start + hits.len();
    Ok(json!({
        "revision": revision,
        "surfaces": scope.covered(),
        "total": total,
        "hits": hits,
        "nextCursor": (end < total).then(|| cursor(&kind, revision, end)),
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
///
/// `scope` picks the surfaces: the body (the default) also yields one chunk
/// per comment thread, as it always has; one other surface yields that
/// surface's chunks; `Scope::All` yields every surface's chunks (a comment's
/// body through its thread chunk, not twice). Each chunk names its surface.
/// **O(document)**; paged.
pub fn chunks(
    session: &Session,
    max_tokens: Option<u32>,
    cursor_in: Option<&str>,
    limit: Option<usize>,
) -> AiResult<Value> {
    chunks_in(session, max_tokens, Scope::default(), cursor_in, limit)
}

/// [`chunks`] over a [`Scope`]. **O(document)**; paged.
///
/// # Errors
///
/// `surface-not-found`, `stale-cursor`.
pub fn chunks_in(
    session: &Session,
    max_tokens: Option<u32>,
    scope: Scope,
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
                 kind: &str,
                 surface: &str| {
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
            "surface": surface,
        }));
    };
    let threads = matches!(scope, Scope::One(Surface::Body) | Scope::All);
    let entries: Vec<(Surface, &[BlockNode])> = scope
        .entries(document)?
        .into_iter()
        .filter(|(surface, _)| !(scope == Scope::All && matches!(surface, Surface::Comment(_))))
        .collect();
    for (surface, blocks) in entries {
        let key = surface.to_string();
        path.clear();
        for placed in blocks_in(blocks) {
            match placed.block {
                Block::Paragraph(paragraph) => {
                    let text = paragraph_text(paragraph);
                    let cost = estimate_tokens(&text);
                    if let Some(level) = heading_level(document, paragraph) {
                        flush(&mut current, &path, &mut out, "section", &key);
                        while path.last().is_some_and(|(at, _)| *at >= level) {
                            path.pop();
                        }
                        path.push((level, clip(&text, 200).to_owned()));
                    } else if current.2.saturating_add(cost) > ceiling {
                        flush(&mut current, &path, &mut out, "section", &key);
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
                    flush(&mut current, &path, &mut out, "section", &key);
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
                            flush(&mut rows, &path, &mut out, "table", &key);
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
                    flush(&mut rows, &path, &mut out, "table", &key);
                }
                Block::ContentControl(_) | Block::Opaque(..) => {}
            }
        }
        flush(&mut current, &path, &mut out, "section", &key);
    }
    for comment in build_review_comment_summaries(document)
        .into_iter()
        .filter(|_| threads)
    {
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
    let kind = format!("chunks{}", scope.tag());
    let start = parse_cursor(&kind, revision, cursor_in)?;
    let limit = session.config().limits.page(limit, 50);
    let total = out.len();
    let page: Vec<Value> = out.into_iter().skip(start).take(limit).collect();
    let end = start + page.len();
    Ok(json!({
        "revision": revision,
        "chunkerVersion": CHUNKER_VERSION,
        "estimator": ESTIMATOR,
        "surfaces": match scope {
            Scope::One(Surface::Body) => json!(["body", "comments"]),
            other => other.covered(),
        },
        "total": total,
        "chunks": page,
        "nextCursor": (end < total).then(|| cursor(&kind, revision, end)),
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
