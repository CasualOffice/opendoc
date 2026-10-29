// SPDX-License-Identifier: Apache-2.0

//! docxtemplater-style templating, filled through the engine.
//!
//! The tag language is docxtemplater's (`docs/176` §12.3):
//!
//! | Tag | Meaning |
//! | --- | --- |
//! | `{name}`, `{user.name}` | The value, looked up through the enclosing scopes |
//! | `{#items}` … `{/items}` (or `{/}`) | A loop over an array, a scope for an object, a condition for anything else |
//! | `{^items}` … `{/items}` | Inverted: shown when the value is missing, false, or an empty array |
//! | `{.}` | The current loop item |
//!
//! - **Paragraph loops** (docxtemplater's `paragraphLoop: true`, here always
//!   on): when an opening and a closing tag each stand alone in their own
//!   paragraph, the paragraphs (and tables) between them repeat, and the two
//!   tag paragraphs disappear.
//! - **Table-row loops**: an opening tag at the start of a row's first cell and
//!   the closing tag at the end of a row's last cell repeat those rows.
//! - **Inline sections**: an opening and closing tag in one paragraph repeat
//!   the text between them, in place.
//! - **Line breaks** (docxtemplater's `linebreaks: true`, here always on): a
//!   `\n` in a value becomes a line break, a `\t` a tab.
//!
//! Every change is an engine operation (`InsertText`, `DeleteText`,
//! `InsertBlocks`, `DeleteBlocks`, `InsertRow`, `DeleteRow`, `SetInlines`), so
//! the filled document keeps the template's formatting, styles, headers and
//! everything the importer preserved verbatim, and it saves loss-aware like any
//! other edit. Errors collect like docxtemplater's `MultiError`: every bad tag
//! is reported at once, each with a stable `id` and the paragraph it is in.

use casual_doc_edit::Operation;
use casual_doc_edit::clone::{CloneReport, clone_blocks_with_fresh_ids};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, Break, BreakKind, InlineNode, Run, Tab, node_plain_text};
use serde::Serialize;
use serde_json::Value;

use crate::edit::{Editor, locate_block};
use crate::error::{Error, ErrorCode, Result};

/// Template options.
#[derive(Clone, Debug)]
pub struct TemplateOptions {
    /// The opening delimiter (docxtemplater `delimiters.start`).
    pub start: String,
    /// The closing delimiter.
    pub end: String,
    /// What a missing or `null` value renders as (docxtemplater's `nullGetter`
    /// returning a constant). docxtemplater's default prints `undefined`; the
    /// default here is the empty string.
    pub null_value: String,
    /// Fail on a missing value instead of rendering `null_value`.
    pub strict: bool,
}

impl Default for TemplateOptions {
    fn default() -> Self {
        Self {
            start: "{".to_owned(),
            end: "}".to_owned(),
            null_value: String::new(),
            strict: false,
        }
    }
}

/// One template error.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateIssue {
    /// docxtemplater's id where one exists: `"unclosed_tag"`, `"unopened_tag"`,
    /// `"unclosed_loop"`, `"unopened_loop"`, `"closing_tag_does_not_match_opening_tag"`,
    /// plus `"undefined_value"` (strict mode).
    pub id: &'static str,
    /// One sentence.
    pub explanation: String,
    /// The tag as written.
    pub tag: String,
    /// The paragraph it is in.
    pub paragraph: String,
}

/// A template that could not be filled: every issue found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TemplateError {
    /// The issues, in document order.
    pub errors: Vec<TemplateIssue>,
}

impl From<TemplateError> for Error {
    fn from(error: TemplateError) -> Self {
        let first = error
            .errors
            .first()
            .map(|e| e.explanation.clone())
            .unwrap_or_default();
        Error::new(
            ErrorCode::Template,
            format!(
                "{} template error(s): {first} | {}",
                error.errors.len(),
                serde_json::to_string(&error.errors).unwrap_or_default()
            ),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TagKind {
    Value,
    Open,
    Inverted,
    Close,
}

#[derive(Clone, Debug)]
struct Tag {
    kind: TagKind,
    name: String,
    /// Character range of the whole tag, delimiters included.
    start: usize,
    end: usize,
    raw: String,
}

/// The tags in `text`, by character offset.
fn scan(text: &str, options: &TemplateOptions) -> std::result::Result<Vec<Tag>, (String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let open: Vec<char> = options.start.chars().collect();
    let close: Vec<char> = options.end.chars().collect();
    let matches_at = |i: usize, pat: &[char]| chars.get(i..i + pat.len()) == Some(pat);
    let mut tags = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if matches_at(i, &close) {
            return Err((
                "unopened_tag".to_owned(),
                chars[i..(i + close.len()).min(chars.len())]
                    .iter()
                    .collect(),
            ));
        }
        if !matches_at(i, &open) {
            i += 1;
            continue;
        }
        let body_start = i + open.len();
        let mut j = body_start;
        while j < chars.len() && !matches_at(j, &close) {
            if matches_at(j, &open) {
                break;
            }
            j += 1;
        }
        if j >= chars.len() || !matches_at(j, &close) {
            return Err((
                "unclosed_tag".to_owned(),
                chars[i..j.min(chars.len())].iter().collect(),
            ));
        }
        let body: String = chars[body_start..j]
            .iter()
            .collect::<String>()
            .trim()
            .to_owned();
        let end = j + close.len();
        let raw: String = chars[i..end].iter().collect();
        let (kind, name) = match body.chars().next() {
            Some('#') => (TagKind::Open, body[1..].trim().to_owned()),
            Some('^') => (TagKind::Inverted, body[1..].trim().to_owned()),
            Some('/') => (TagKind::Close, body[1..].trim().to_owned()),
            _ => (TagKind::Value, body.clone()),
        };
        tags.push(Tag {
            kind,
            name,
            start: i,
            end,
            raw,
        });
        i = end;
    }
    Ok(tags)
}

/// Looks `path` up through the scope stack, innermost first.
fn lookup<'v>(scopes: &[&'v Value], path: &str) -> Option<&'v Value> {
    if path == "." || path.is_empty() {
        return scopes.last().copied();
    }
    for scope in scopes.iter().rev() {
        let mut current = *scope;
        let mut found = true;
        for (index, part) in path.split('.').enumerate() {
            match current.get(part.trim()) {
                Some(next) => current = next,
                None => {
                    found = false;
                    // Only the first segment searches outward.
                    if index > 0 {
                        return None;
                    }
                    break;
                }
            }
        }
        if found {
            return Some(current);
        }
    }
    None
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().is_some_and(|v| v != 0.0),
        Some(Value::Object(_)) => true,
    }
}

/// The scopes a section renders with: one per array item, one for any other
/// truthy value (an object becomes the scope; a scalar keeps the parent's),
/// none when falsy. Inverted: one when falsy.
fn section_scopes<'v>(
    value: Option<&'v Value>,
    inverted: bool,
    parent: &'v Value,
) -> Vec<&'v Value> {
    if inverted {
        return if truthy(value) {
            Vec::new()
        } else {
            vec![parent]
        };
    }
    match value {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(object @ Value::Object(_)) => vec![object],
        other if truthy(other) => vec![parent],
        _ => Vec::new(),
    }
}

/// The text a value renders as.
fn display(value: Option<&Value>, options: &TemplateOptions) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        Some(Value::Bool(b)) => Some(b.to_string()),
        Some(other) => Some(other.to_string()),
    }
    .or_else(|| (!options.strict).then(|| options.null_value.clone()))
}

/// The filler: walks stories, lowering every change to operations.
struct Filler<'e, 'a> {
    editor: &'e mut Editor<'a>,
    options: &'e TemplateOptions,
    issues: Vec<TemplateIssue>,
}

impl Filler<'_, '_> {
    fn issue(&mut self, id: &'static str, explanation: String, tag: &str, paragraph: NodeId) {
        self.issues.push(TemplateIssue {
            id,
            explanation,
            tag: tag.to_owned(),
            paragraph: paragraph.to_string(),
        });
    }

    /// The current block with `id` in `container`, cloned.
    fn block(&self, id: NodeId) -> Option<BlockNode> {
        let model = self.editor.model();
        let (container, index) = locate_block(model.body(), id, None)?;
        let blocks = container_blocks(model, container)?;
        blocks.get(index).cloned()
    }

    fn paragraph_text(&self, id: NodeId) -> Option<String> {
        casual_doc_edit::find_paragraph_any(self.editor.model(), id)
            .map(|p| node_plain_text(&p.inlines))
    }

    /// A paragraph that is exactly one section tag (whitespace aside).
    fn lone_tag(&self, id: NodeId) -> Option<Tag> {
        let text = self.paragraph_text(id)?;
        let tags = scan(&text, self.options).ok()?;
        let [tag] = tags.as_slice() else { return None };
        let before: String = text.chars().take(tag.start).collect();
        let after: String = text.chars().skip(tag.end).collect();
        (tag.kind != TagKind::Value && before.trim().is_empty() && after.trim().is_empty())
            .then(|| tag.clone())
    }

    /// Renders a list of sibling blocks (by id) with `scopes`.
    fn blocks(&mut self, ids: &[NodeId], scopes: &[&Value]) -> Result<()> {
        let mut i = 0;
        while i < ids.len() {
            let id = ids[i];
            let Some(block) = self.block(id) else {
                i += 1;
                continue;
            };
            if let BlockNode::Paragraph(_) = &block
                && let Some(open) = self.lone_tag(id)
                && open.kind != TagKind::Close
            {
                // Find the matching lone closing paragraph among the siblings.
                let mut depth = 0_usize;
                let mut close_at = None;
                for (j, other) in ids.iter().enumerate().skip(i + 1) {
                    if let Some(tag) = self.lone_tag(*other) {
                        match tag.kind {
                            TagKind::Open | TagKind::Inverted => depth += 1,
                            TagKind::Close if depth == 0 => {
                                if !tag.name.is_empty() && tag.name != open.name {
                                    self.issue(
                                        "closing_tag_does_not_match_opening_tag",
                                        format!(
                                            "the loop \"{}\" is closed by \"{}\"",
                                            open.name, tag.name
                                        ),
                                        &tag.raw,
                                        *other,
                                    );
                                }
                                close_at = Some(j);
                                break;
                            }
                            TagKind::Close => depth -= 1,
                            TagKind::Value => {}
                        }
                    }
                }
                let Some(j) = close_at else {
                    // Not a paragraph loop: an inline section whose closing
                    // tag is elsewhere, which is an unclosed loop here.
                    self.issue(
                        "unclosed_loop",
                        format!("the loop \"{}\" is never closed", open.name),
                        &open.raw,
                        id,
                    );
                    i += 1;
                    continue;
                };
                self.paragraph_loop(&open, &ids[i..=j], scopes)?;
                i = j + 1;
                continue;
            }
            match block {
                BlockNode::Paragraph(_) => self.paragraph(id, scopes)?,
                BlockNode::Table(table) => self.table(&table, scopes)?,
                BlockNode::Sdt(sdt) => {
                    let children: Vec<NodeId> = sdt.blocks.iter().map(block_id).collect();
                    self.blocks(&children, scopes)?;
                }
                BlockNode::AltChunk(_) => {}
            }
            i += 1;
        }
        Ok(())
    }

    /// Repeats the blocks strictly between the first and last of `span` once
    /// per scope, and removes the two tag paragraphs.
    fn paragraph_loop(&mut self, open: &Tag, span: &[NodeId], scopes: &[&Value]) -> Result<()> {
        let first = span[0];
        let model = self.editor.model();
        let (container, index) = locate_block(model.body(), first, None)
            .ok_or_else(|| Error::invalid("a loop's paragraph vanished"))?;
        let siblings = container_blocks(model, container)
            .ok_or_else(|| Error::invalid("a loop's container vanished"))?;
        let inner: Vec<BlockNode> = siblings[index + 1..index + span.len() - 1].to_vec();
        let siblings_len = siblings.len();
        let value = lookup(scopes, &open.name);
        let parent = scopes.last().copied().unwrap_or(&Value::Null);
        let item_scopes = section_scopes(value, open.kind == TagKind::Inverted, parent);
        let mut groups: Vec<Vec<NodeId>> = Vec::new();
        let mut copies: Vec<BlockNode> = Vec::new();
        for _ in &item_scopes {
            let mut report = CloneReport::default();
            let mut ids = IdSource(&mut *self.editor);
            let cloned = clone_blocks_with_fresh_ids(&inner, &mut ids, &mut report)
                .ok_or_else(|| Error::new(ErrorCode::Exhausted, "id space exhausted"))?;
            groups.push(cloned.iter().map(block_id).collect());
            copies.extend(cloned);
        }
        let count = u32::try_from(span.len()).unwrap_or(u32::MAX);
        let index32 = u32::try_from(index).unwrap_or(u32::MAX);
        let mut ops = Vec::new();
        if !copies.is_empty() {
            ops.push(Operation::InsertBlocks {
                container,
                index: index32 + count,
                blocks: copies,
            });
        }
        // A cell must keep one block: when the loop is the cell's whole
        // content and renders nothing, leave one empty paragraph.
        let would_empty = container.is_some() && siblings_len == span.len() && groups.is_empty();
        if would_empty {
            let empty = self.editor.new_paragraph("", None)?;
            ops.push(Operation::InsertBlocks {
                container,
                index: index32 + count,
                blocks: vec![BlockNode::Paragraph(empty)],
            });
        }
        ops.push(Operation::DeleteBlocks {
            container,
            index: index32,
            count,
        });
        self.editor.apply(ops)?;
        for (group, scope) in groups.iter().zip(item_scopes) {
            let mut inner_scopes = scopes.to_vec();
            inner_scopes.push(scope);
            self.blocks(group, &inner_scopes)?;
        }
        Ok(())
    }

    fn table(&mut self, table: &casual_doc_model::v1::Table, scopes: &[&Value]) -> Result<()> {
        let mut r = 0;
        while r < table.rows.len() {
            if let Some((open, close_row)) = self.row_loop_at(table, r) {
                self.row_loop(table, r, close_row, &open, scopes)?;
                // The rendered rows were filled in `row_loop`; continue after
                // the template rows in the ORIGINAL numbering.
                r = close_row + 1;
                continue;
            }
            for cell in &table.rows[r].cells {
                let children: Vec<NodeId> = cell.blocks.iter().map(block_id).collect();
                self.blocks(&children, scopes)?;
            }
            r += 1;
        }
        Ok(())
    }

    /// An opening tag that begins row `r`'s first cell and a matching closing
    /// tag that ends some row's last cell.
    fn row_loop_at(&self, table: &casual_doc_model::v1::Table, r: usize) -> Option<(Tag, usize)> {
        let first_paragraph = |row: &casual_doc_model::v1::TableRow, last: bool| {
            let cell = if last {
                row.cells.last()
            } else {
                row.cells.first()
            }?;
            let block = if last {
                cell.blocks.last()
            } else {
                cell.blocks.first()
            }?;
            match block {
                BlockNode::Paragraph(p) => Some(node_plain_text(&p.inlines)),
                _ => None,
            }
        };
        let text = first_paragraph(&table.rows[r], false)?;
        let tags = scan(&text, self.options).ok()?;
        let open = tags.first()?.clone();
        if !matches!(open.kind, TagKind::Open | TagKind::Inverted)
            || !text
                .chars()
                .take(open.start)
                .collect::<String>()
                .trim()
                .is_empty()
        {
            return None;
        }
        // A section closed inside the same paragraph is inline, not a row loop.
        if tags.iter().any(|t| t.kind == TagKind::Close) {
            return None;
        }
        for (index, row) in table.rows.iter().enumerate().skip(r) {
            let text = first_paragraph(row, true)?;
            let tags = scan(&text, self.options).ok()?;
            if let Some(close) = tags.last()
                && close.kind == TagKind::Close
                && (close.name.is_empty() || close.name == open.name)
                && text
                    .chars()
                    .skip(close.end)
                    .collect::<String>()
                    .trim()
                    .is_empty()
                && (index != r || tags.len() >= 2)
            {
                return Some((open, index));
            }
        }
        None
    }

    fn row_loop(
        &mut self,
        table: &casual_doc_model::v1::Table,
        first: usize,
        last: usize,
        open: &Tag,
        scopes: &[&Value],
    ) -> Result<()> {
        let value = lookup(scopes, &open.name);
        let parent = scopes.last().copied().unwrap_or(&Value::Null);
        let item_scopes = section_scopes(value, open.kind == TagKind::Inverted, parent);
        // The current row positions: earlier loops in this table may have
        // inserted rows, so find the template's first row by id.
        let first_id = table.rows[first].id;
        let current = casual_doc_edit::find_table(self.editor.model(), table.id)
            .ok_or_else(|| Error::invalid("the table vanished"))?
            .clone();
        let start = current
            .rows
            .iter()
            .position(|row| row.id == first_id)
            .ok_or_else(|| Error::invalid("a loop row vanished"))?;
        let template: Vec<_> = current.rows[start..=start + (last - first)].to_vec();
        let mut inserted: Vec<(Vec<NodeId>, &Value)> = Vec::new();
        let mut at = u32::try_from(start + template.len()).unwrap_or(u32::MAX);
        for scope in &item_scopes {
            let mut row_ids = Vec::new();
            for row in &template {
                let wrapped = BlockNode::Table(Box::new(casual_doc_model::v1::Table {
                    rows: vec![row.clone()],
                    ..current.clone()
                }));
                let mut report = CloneReport::default();
                let mut ids = IdSource(&mut *self.editor);
                let cloned = clone_blocks_with_fresh_ids(
                    std::slice::from_ref(&wrapped),
                    &mut ids,
                    &mut report,
                )
                .ok_or_else(|| Error::new(ErrorCode::Exhausted, "id space exhausted"))?;
                let Some(BlockNode::Table(cloned)) = cloned.into_iter().next() else {
                    return Err(Error::invalid("a row did not clone"));
                };
                let Some(new_row) = cloned.rows.into_iter().next() else {
                    return Err(Error::invalid("a row did not clone"));
                };
                row_ids.push(new_row.id);
                self.editor.apply(vec![Operation::InsertRow {
                    table: table.id,
                    index: at,
                    row: Box::new(new_row),
                }])?;
                at += 1;
            }
            inserted.push((row_ids, scope));
        }
        if item_scopes.is_empty() && template.len() == current.rows.len() {
            // Deleting every row would delete the table's last row, which the
            // engine refuses: remove the table instead.
            self.editor
                .apply(vec![Operation::DeleteTable { table: table.id }])?;
            return Ok(());
        }
        for _ in &template {
            self.editor.apply(vec![Operation::DeleteRow {
                table: table.id,
                index: u32::try_from(start).unwrap_or(u32::MAX),
            }])?;
        }
        for (row_ids, scope) in inserted {
            let mut inner_scopes = scopes.to_vec();
            inner_scopes.push(scope);
            let t = casual_doc_edit::find_table(self.editor.model(), table.id)
                .ok_or_else(|| Error::invalid("the table vanished"))?
                .clone();
            for (k, row_id) in row_ids.iter().enumerate() {
                let Some(row) = t.rows.iter().find(|row| row.id == *row_id) else {
                    continue;
                };
                for (c, cell) in row.cells.iter().enumerate() {
                    // Strip the loop's own tags from the first and last cells.
                    if k == 0
                        && c == 0
                        && let Some(BlockNode::Paragraph(p)) = cell.blocks.first()
                    {
                        self.strip_first_tag(p.id)?;
                    }
                    if k + 1 == row_ids.len()
                        && c + 1 == row.cells.len()
                        && let Some(BlockNode::Paragraph(p)) = cell.blocks.last()
                    {
                        self.strip_last_tag(p.id)?;
                    }
                    let children: Vec<NodeId> =
                        casual_doc_edit::find_table(self.editor.model(), table.id)
                            .and_then(|t| t.rows.iter().find(|r| r.id == *row_id).cloned())
                            .and_then(|r| r.cells.get(c).cloned())
                            .map(|cell| cell.blocks.iter().map(block_id).collect())
                            .unwrap_or_default();
                    self.blocks(&children, &inner_scopes)?;
                }
            }
        }
        Ok(())
    }

    fn strip_first_tag(&mut self, paragraph: NodeId) -> Result<()> {
        let text = self.paragraph_text(paragraph).unwrap_or_default();
        if let Some(tag) = scan(&text, self.options)
            .ok()
            .and_then(|t| t.first().cloned())
        {
            self.editor
                .delete_text(&paragraph.to_string(), tag.start, tag.end)?;
        }
        Ok(())
    }

    fn strip_last_tag(&mut self, paragraph: NodeId) -> Result<()> {
        let text = self.paragraph_text(paragraph).unwrap_or_default();
        if let Some(tag) = scan(&text, self.options)
            .ok()
            .and_then(|t| t.last().cloned())
        {
            self.editor
                .delete_text(&paragraph.to_string(), tag.start, tag.end)?;
        }
        Ok(())
    }

    /// Renders the tags inside one paragraph, right to left, so earlier
    /// offsets stay valid. Values and inline sections become text in place.
    fn paragraph(&mut self, id: NodeId, scopes: &[&Value]) -> Result<()> {
        let Some(text) = self.paragraph_text(id) else {
            return Ok(());
        };
        let tags = match scan(&text, self.options) {
            Ok(tags) => tags,
            Err((code, raw)) => {
                let code = if code == "unclosed_tag" {
                    "unclosed_tag"
                } else {
                    "unopened_tag"
                };
                self.issue(
                    code,
                    format!("the tag \"{raw}\" is not closed or opened"),
                    &raw,
                    id,
                );
                return Ok(());
            }
        };
        if tags.is_empty() {
            return Ok(());
        }
        // Pair the top-level sections; everything else is a value.
        let mut replacements: Vec<(usize, usize, String)> = Vec::new();
        let mut k = 0;
        let chars: Vec<char> = text.chars().collect();
        while k < tags.len() {
            let tag = &tags[k];
            match tag.kind {
                TagKind::Value => {
                    match display(lookup(scopes, &tag.name), self.options) {
                        Some(shown) => replacements.push((tag.start, tag.end, shown)),
                        None => self.issue(
                            "undefined_value",
                            format!("no value for \"{}\"", tag.name),
                            &tag.raw,
                            id,
                        ),
                    }
                    k += 1;
                }
                TagKind::Close => {
                    self.issue(
                        "unopened_loop",
                        format!("\"{}\" closes a loop that was never opened", tag.raw),
                        &tag.raw,
                        id,
                    );
                    k += 1;
                }
                TagKind::Open | TagKind::Inverted => {
                    let mut depth = 0;
                    let mut close = None;
                    for (m, other) in tags.iter().enumerate().skip(k + 1) {
                        match other.kind {
                            TagKind::Open | TagKind::Inverted => depth += 1,
                            TagKind::Close if depth == 0 => {
                                close = Some(m);
                                break;
                            }
                            TagKind::Close => depth -= 1,
                            TagKind::Value => {}
                        }
                    }
                    let Some(m) = close else {
                        self.issue(
                            "unclosed_loop",
                            format!("the loop \"{}\" is never closed", tag.name),
                            &tag.raw,
                            id,
                        );
                        k += 1;
                        continue;
                    };
                    let inner: String = chars[tag.end..tags[m].start].iter().collect();
                    let value = lookup(scopes, &tag.name);
                    let parent = scopes.last().copied().unwrap_or(&Value::Null);
                    let mut rendered = String::new();
                    for scope in section_scopes(value, tag.kind == TagKind::Inverted, parent) {
                        let mut inner_scopes = scopes.to_vec();
                        inner_scopes.push(scope);
                        rendered.push_str(&self.render_text(&inner, &inner_scopes, id));
                    }
                    replacements.push((tag.start, tags[m].end, rendered));
                    k = m + 1;
                }
            }
        }
        let paragraph = id.to_string();
        for (start, end, text) in replacements.into_iter().rev() {
            self.editor.delete_text(&paragraph, start, end)?;
            if !text.is_empty() {
                self.editor.insert_text(&paragraph, start, &text)?;
            }
        }
        Ok(())
    }

    /// Renders tags in a plain string (the inside of an inline section).
    fn render_text(&mut self, text: &str, scopes: &[&Value], paragraph: NodeId) -> String {
        let Ok(tags) = scan(text, self.options) else {
            return text.to_owned();
        };
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::new();
        let mut cursor = 0;
        let mut k = 0;
        while k < tags.len() {
            let tag = tags[k].clone();
            out.extend(&chars[cursor..tag.start]);
            match tag.kind {
                TagKind::Value => {
                    match display(lookup(scopes, &tag.name), self.options) {
                        Some(shown) => out.push_str(&shown),
                        None => self.issue(
                            "undefined_value",
                            format!("no value for \"{}\"", tag.name),
                            &tag.raw,
                            paragraph,
                        ),
                    }
                    cursor = tag.end;
                    k += 1;
                }
                TagKind::Open | TagKind::Inverted => {
                    let mut depth = 0;
                    let close = tags.iter().enumerate().skip(k + 1).find_map(|(m, other)| {
                        match other.kind {
                            TagKind::Open | TagKind::Inverted => depth += 1,
                            TagKind::Close if depth == 0 => return Some(m),
                            TagKind::Close => depth -= 1,
                            TagKind::Value => {}
                        }
                        None
                    });
                    let Some(m) = close else {
                        cursor = tag.end;
                        k += 1;
                        continue;
                    };
                    let inner: String = chars[tag.end..tags[m].start].iter().collect();
                    let value = lookup(scopes, &tag.name);
                    let parent = scopes.last().copied().unwrap_or(&Value::Null);
                    for scope in section_scopes(value, tag.kind == TagKind::Inverted, parent) {
                        let mut inner_scopes = scopes.to_vec();
                        inner_scopes.push(scope);
                        let piece = self.render_text(&inner, &inner_scopes, paragraph);
                        out.push_str(&piece);
                    }
                    cursor = tags[m].end;
                    k = m + 1;
                }
                TagKind::Close => {
                    cursor = tag.end;
                    k += 1;
                }
            }
        }
        out.extend(&chars[cursor.min(chars.len())..]);
        out
    }
}

/// An id source over the editor's generator.
pub(crate) struct IdSource<'e, 'a>(pub(crate) &'e mut Editor<'a>);

impl casual_doc_edit::RunIds for IdSource<'_, '_> {
    fn next(&mut self) -> Option<NodeId> {
        self.0.fresh().ok()
    }
}

pub(crate) fn split_controls(
    inlines: &[InlineNode],
    ids: &mut dyn casual_doc_edit::RunIds,
) -> Result<Vec<InlineNode>> {
    let exhausted = || Error::new(ErrorCode::Exhausted, "id space exhausted");
    let mut out = Vec::with_capacity(inlines.len());
    for inline in inlines {
        match inline {
            InlineNode::Run(run) if run.text.contains(['\n', '\t']) => {
                let mut piece = String::new();
                let mut first = true;
                let flush = |piece: &mut String,
                             first: &mut bool,
                             out: &mut Vec<InlineNode>,
                             ids: &mut dyn casual_doc_edit::RunIds|
                 -> Result<()> {
                    if !piece.is_empty() {
                        out.push(InlineNode::Run(Run {
                            id: if *first {
                                run.id
                            } else {
                                ids.next().ok_or_else(exhausted)?
                            },
                            properties: run.properties.clone(),
                            text: std::mem::take(piece),
                        }));
                        *first = false;
                    }
                    Ok(())
                };
                for c in run.text.chars() {
                    match c {
                        '\n' => {
                            flush(&mut piece, &mut first, &mut out, ids)?;
                            out.push(InlineNode::Break(Break {
                                id: ids.next().ok_or_else(exhausted)?,
                                kind: BreakKind::Line,
                            }));
                        }
                        '\t' => {
                            flush(&mut piece, &mut first, &mut out, ids)?;
                            out.push(InlineNode::Tab(Tab {
                                id: ids.next().ok_or_else(exhausted)?,
                            }));
                        }
                        c => piece.push(c),
                    }
                }
                flush(&mut piece, &mut first, &mut out, ids)?;
            }
            InlineNode::Hyperlink(link) => {
                let mut link = link.clone();
                link.inlines = split_controls(&link.inlines, ids)?;
                out.push(InlineNode::Hyperlink(link));
            }
            InlineNode::Sdt(sdt) => {
                let mut sdt = sdt.clone();
                sdt.inlines = split_controls(&sdt.inlines, ids)?;
                out.push(InlineNode::Sdt(sdt));
            }
            other => out.push(other.clone()),
        }
    }
    Ok(out)
}

fn block_id(block: &BlockNode) -> NodeId {
    match block {
        BlockNode::Paragraph(p) => p.id,
        BlockNode::Table(t) => t.id,
        BlockNode::Sdt(s) => s.id,
        BlockNode::AltChunk(a) => a.id,
    }
}

/// The blocks of a container (`None` = the body). **O(blocks)**.
fn container_blocks(
    model: &casual_doc_model::v1::Document,
    container: Option<NodeId>,
) -> Option<&[BlockNode]> {
    fn find(blocks: &[BlockNode], id: NodeId) -> Option<&[BlockNode]> {
        for block in blocks {
            match block {
                BlockNode::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            if cell.id == id {
                                return Some(&cell.blocks);
                            }
                            if let Some(found) = find(&cell.blocks, id) {
                                return Some(found);
                            }
                        }
                    }
                }
                BlockNode::Sdt(s) => {
                    if s.id == id {
                        return Some(&s.blocks);
                    }
                    if let Some(found) = find(&s.blocks, id) {
                        return Some(found);
                    }
                }
                BlockNode::Paragraph(_) | BlockNode::AltChunk(_) => {}
            }
        }
        None
    }
    match container {
        None => Some(model.body()),
        Some(id) => find(model.body(), id),
    }
}

/// Fills the template in `editor`'s document with `data`. Headers, footers,
/// footnotes and endnotes get value tags and inline sections (their block
/// lists are not containers the operation set inserts into, so a paragraph
/// loop there is reported, not run).
pub(crate) fn fill(
    editor: &mut Editor<'_>,
    data: &Value,
    options: &TemplateOptions,
) -> Result<usize> {
    if options.start.is_empty() || options.end.is_empty() {
        return Err(Error::invalid("template delimiters cannot be empty"));
    }
    let body: Vec<NodeId> = editor.model().body().iter().map(block_id).collect();
    let mut filler = Filler {
        editor,
        options,
        issues: Vec::new(),
    };
    filler.blocks(&body, &[data])?;
    // The other stories: every paragraph, inline tags only.
    let mut others: Vec<NodeId> = Vec::new();
    {
        let defs = filler.editor.model().definitions();
        let mut collect = |blocks: &[BlockNode]| {
            casual_doc_model::v1::visit_paragraphs(blocks, &mut |p| others.push(p.id));
        };
        for (_, story) in defs.headers.iter().chain(defs.footers.iter()) {
            collect(&story.blocks);
        }
        for (_, note) in defs.footnotes.iter().chain(defs.endnotes.iter()) {
            collect(&note.blocks);
        }
    }
    for id in others {
        filler.paragraph(id, &[data])?;
    }
    if filler.issues.is_empty() {
        Ok(0)
    } else {
        Err(TemplateError {
            errors: filler.issues,
        }
        .into())
    }
}

/// The tags a template uses, docxtemplater's inspect module (`getTags`): every
/// distinct name, in document order, with its kind.
pub(crate) fn tags(
    model: &casual_doc_model::v1::Document,
    options: &TemplateOptions,
) -> Vec<(String, &'static str)> {
    let mut out: Vec<(String, &'static str)> = Vec::new();
    casual_doc_edit::visit_paragraphs_all_surfaces(model, &mut |p| {
        let text = node_plain_text(&p.inlines);
        if let Ok(tags) = scan(&text, options) {
            for tag in tags {
                let kind = match tag.kind {
                    TagKind::Value => "value",
                    TagKind::Open => "loop",
                    TagKind::Inverted => "inverted",
                    TagKind::Close => continue,
                };
                if !out.iter().any(|(n, k)| *n == tag.name && *k == kind) {
                    out.push((tag.name, kind));
                }
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scanner_reads_each_tag_kind() {
        let tags = scan(
            "Hi {name}! {#items}{.}{/items} {^none}x{/}",
            &TemplateOptions::default(),
        )
        .unwrap();
        let kinds: Vec<_> = tags
            .iter()
            .map(|t| (t.kind.clone(), t.name.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (TagKind::Value, "name"),
                (TagKind::Open, "items"),
                (TagKind::Value, "."),
                (TagKind::Close, "items"),
                (TagKind::Inverted, "none"),
                (TagKind::Close, ""),
            ]
        );
        assert!(scan("broken {tag", &TemplateOptions::default()).is_err());
    }

    #[test]
    fn lookups_search_outward_and_follow_paths() {
        let data = serde_json::json!({"company": {"name": "ACME"}, "who": "root"});
        let item = serde_json::json!({"who": "item"});
        let scopes = [&data, &item];
        assert_eq!(lookup(&scopes, "who"), Some(&Value::from("item")));
        assert_eq!(lookup(&scopes, "company.name"), Some(&Value::from("ACME")));
        assert_eq!(lookup(&scopes, "company.missing"), None);
    }
}
