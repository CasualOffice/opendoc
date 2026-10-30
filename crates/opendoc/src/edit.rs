// SPDX-License-Identifier: Apache-2.0

//! Typed edits, lowered to the engine's closed operation set.
//!
//! An [`Editor`] is handed to the closure of [`crate::Document::edit`]. Every
//! method lowers to `casual_doc_edit::Operation`s and applies them at once
//! through `casual_doc_edit::apply` (ADR-030 I1), after the document's own
//! protection check, so a later edit in the same batch sees the earlier ones.
//! Review edits (comments, suggestions, accept/reject) are lowered by
//! `casual_doc_review::lower`, the editor's own lowering, so a headless
//! suggestion is the same tracked change the editor writes.
//!
//! **Offsets are characters** (Unicode scalar values) into a paragraph's
//! `text` as the read view shows it, which is what a Python `str` index is.
//! JavaScript strings index UTF-16 units; the two agree except for characters
//! outside the Basic Multilingual Plane, and the JS package converts.
//!
//! **Atomicity.** The batch runs on a copy and replaces the document only when
//! every edit succeeded: one refusal leaves the document exactly as it was.

use casual_doc_edit::access::Capabilities;
use casual_doc_edit::protection::refuse_if_protected;
use casual_doc_edit::{FormatDelta, Mint, Operation, Pos, Range};
use casual_doc_model::v1::{
    Alignment, BlockNode, CoreProperties, Document as Model, ExternalTarget, GridColumn,
    HyperlinkTarget, InlineNode, InternalTarget, Paragraph, ParagraphProperties, RgbColor, Run,
    RunProperties, StyleId, StyleKind, Table, TableCell, TableCellProperties, TableProperties,
    TableRow, TableRowProperties, TableWidth, WidthType, node_plain_text,
};
use casual_doc_model::{IdGenerator, NodeId};
use casual_doc_review::body::RevisionIdAllocator;
use casual_doc_review::lower::{self, InsertStyle, ReviewIds};
use serde::Deserialize;

use crate::error::{Error, ErrorCode, Result};
use crate::view::parse_id;

/// Character formatting to set over a range. `None` leaves a property as it is.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunFormat {
    /// Bold.
    #[serde(default)]
    pub bold: Option<bool>,
    /// Italic.
    #[serde(default, alias = "italics")]
    pub italic: Option<bool>,
    /// Underline.
    #[serde(default)]
    pub underline: Option<bool>,
    /// Strike-through.
    #[serde(default)]
    pub strike: Option<bool>,
    /// Colour, `RRGGBB`.
    #[serde(default)]
    pub color: Option<String>,
    /// Size in points (halves allowed).
    #[serde(default)]
    pub size_pt: Option<f64>,
    /// Font family.
    #[serde(default)]
    pub font: Option<String>,
    /// Highlight colour name (`"yellow"`).
    #[serde(default)]
    pub highlight: Option<String>,
    /// `"superscript"`, `"subscript"` or `"baseline"`.
    #[serde(default)]
    pub vertical_align: Option<String>,
}

impl RunFormat {
    /// Bold on.
    #[must_use]
    pub fn bold() -> Self {
        Self {
            bold: Some(true),
            ..Self::default()
        }
    }

    /// Italic on.
    #[must_use]
    pub fn italic() -> Self {
        Self {
            italic: Some(true),
            ..Self::default()
        }
    }

    fn delta(&self) -> Result<FormatDelta> {
        let mut delta = FormatDelta {
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            strike: self.strike,
            font: self.font.clone(),
            ..FormatDelta::default()
        };
        if let Some(color) = &self.color {
            delta.color = Some(parse_rgb(color)?);
        }
        if let Some(size) = self.size_pt {
            if !(0.5..=1_638.0).contains(&size) {
                return Err(Error::invalid(format!(
                    "font size {size} pt is out of range"
                )));
            }
            delta.size_half_points = Some((size * 2.0).round() as u32);
        }
        if let Some(name) = &self.highlight {
            delta.highlight = Some(
                serde_json::from_value(serde_json::Value::String(name.clone()))
                    .map_err(|_| Error::invalid(format!("\"{name}\" is not a highlight colour")))?,
            );
        }
        if let Some(v) = &self.vertical_align {
            delta.vertical_alignment = Some(
                serde_json::from_value(serde_json::Value::String(v.clone())).map_err(|_| {
                    Error::invalid(format!("\"{v}\" is not superscript, subscript or baseline"))
                })?,
            );
        }
        Ok(delta)
    }
}

fn parse_rgb(color: &str) -> Result<RgbColor> {
    let c = color.trim().trim_start_matches('#');
    let channel = |i: usize| u8::from_str_radix(c.get(i..i + 2).unwrap_or("zz"), 16);
    match (c.len(), channel(0), channel(2), channel(4)) {
        (6, Ok(r), Ok(g), Ok(b)) => Ok(RgbColor { r, g, b }),
        _ => Err(Error::invalid(format!(
            "\"{color}\" is not a colour like \"FF0000\""
        ))),
    }
}

/// Where a new block goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Position {
    /// The start of the body.
    Start,
    /// The end of the body.
    End,
    /// Before a block (in the body, a cell or a content control).
    Before(String),
    /// After a block.
    After(String),
}

impl Position {
    /// After the block with `id`.
    #[must_use]
    pub fn after(id: impl Into<String>) -> Self {
        Self::After(id.into())
    }

    /// Before the block with `id`.
    #[must_use]
    pub fn before(id: impl Into<String>) -> Self {
        Self::Before(id.into())
    }
}

/// The inverse of an applied batch: applying it restores the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Undo {
    pub(crate) operations: Vec<Operation>,
}

impl Undo {
    /// How many engine operations the batch lowered to.
    #[must_use]
    pub fn len(&self) -> usize {
        self.operations.len()
    }

    /// Whether the batch changed nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// An opaque, serialised form (for bindings that hand it back later).
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.operations).unwrap_or_else(|_| "[]".to_owned())
    }

    /// The opaque form back.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::InvalidArgument`] when it is not an undo this SDK wrote.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .map(|operations| Self { operations })
            .map_err(|_| Error::invalid("not an undo batch"))
    }
}

/// The mutable state an edit batch runs against.
pub(crate) struct EditState {
    pub(crate) model: Model,
    pub(crate) ids: IdGenerator,
    pub(crate) revisions: RevisionIdAllocator,
}

/// The batch editor handed to [`crate::Document::edit`].
pub struct Editor<'a> {
    state: &'a mut EditState,
    inverses: Vec<Operation>,
    author: String,
}

impl std::fmt::Debug for Editor<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Editor")
            .field("applied", &self.inverses.len())
            .finish_non_exhaustive()
    }
}

impl<'a> Editor<'a> {
    pub(crate) fn new(state: &'a mut EditState, author: String) -> Self {
        Self {
            state,
            inverses: Vec::new(),
            author,
        }
    }

    /// Undoes everything this batch applied, newest first, with the inverses
    /// `casual_doc_edit::apply` returned (the engine's exact undo). **O(the
    /// batch)**, not O(document): this is why a batch needs no copy of the
    /// model to be all or nothing.
    pub(crate) fn rollback(mut self) -> Result<()> {
        let inverses = std::mem::take(&mut self.inverses);
        for inverse in inverses.into_iter().rev() {
            let mint = Mint::reserve(&mut self.state.ids, 1).ok_or_else(|| {
                Error::new(ErrorCode::Exhausted, "the document's id space is exhausted")
            })?;
            casual_doc_edit::apply(&mut self.state.model, mint, &inverse).map_err(|error| {
                Error::edit(format!(
                    "rolling back a refused batch failed: {}",
                    error.reason().unwrap_or("an inverse did not apply")
                ))
            })?;
        }
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Undo {
        self.inverses.reverse();
        Undo {
            operations: self.inverses,
        }
    }

    /// Applies operations in order through the engine's choke point.
    pub(crate) fn apply(&mut self, operations: Vec<Operation>) -> Result<()> {
        refuse_if_protected(&self.state.model, &operations, Capabilities::local())
            .map_err(|refusal| Error::edit(refusal.reason()))?;
        for operation in operations {
            let mint = Mint::reserve(&mut self.state.ids, 1).ok_or_else(|| {
                Error::new(ErrorCode::Exhausted, "the document's id space is exhausted")
            })?;
            let inverse = casual_doc_edit::apply(&mut self.state.model, mint, &operation).map_err(
                |error| {
                    Error::edit(
                        error
                            .reason()
                            .map_or_else(|| format!("{error:?}"), str::to_owned),
                    )
                },
            )?;
            self.inverses.push(inverse);
        }
        Ok(())
    }

    pub(crate) fn fresh(&mut self) -> Result<NodeId> {
        self.state
            .ids
            .next_id()
            .map_err(|_| Error::new(ErrorCode::Exhausted, "the document's id space is exhausted"))
    }

    pub(crate) fn model(&self) -> &Model {
        &self.state.model
    }

    /// The author review edits are attributed to.
    #[must_use]
    pub fn author(&self) -> &str {
        &self.author
    }

    // -- text --

    /// The paragraph's text, on any surface.
    fn paragraph_text(&self, paragraph: NodeId) -> Result<String> {
        find_paragraph(&self.state.model, paragraph)
            .map(|p| node_plain_text(&p.inlines))
            .ok_or_else(|| Error::invalid(format!("no paragraph with id {paragraph}")))
    }

    /// Converts a character offset into the byte offset the engine indexes.
    fn byte_offset(text: &str, chars: usize) -> Result<u32> {
        let bytes = if chars == text.chars().count() {
            text.len()
        } else {
            text.char_indices()
                .nth(chars)
                .map(|(i, _)| i)
                .ok_or_else(|| {
                    Error::invalid(format!(
                        "offset {chars} is past the paragraph's {} characters",
                        text.chars().count()
                    ))
                })?
        };
        u32::try_from(bytes).map_err(|_| Error::invalid("paragraph too long"))
    }

    fn range(&self, paragraph: &str, start: usize, end: usize) -> Result<(NodeId, u32, u32)> {
        if start >= end {
            return Err(Error::invalid(format!(
                "range [{start}, {end}) is empty or reversed"
            )));
        }
        let node = parse_id(paragraph)?;
        let text = self.paragraph_text(node)?;
        Ok((
            node,
            Self::byte_offset(&text, start)?,
            Self::byte_offset(&text, end)?,
        ))
    }

    /// Inserts `text` at character `offset` of a paragraph. It takes the
    /// formatting of the text before it, as typing does.
    ///
    /// # Errors
    ///
    /// An unknown paragraph, an offset past its end, or a refusal.
    pub fn insert_text(&mut self, paragraph: &str, offset: usize, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        let node = parse_id(paragraph)?;
        let current = self.paragraph_text(node)?;
        let at = Pos::new(node, Self::byte_offset(&current, offset)?);
        self.apply(vec![Operation::InsertText {
            at,
            text: text.to_owned(),
        }])?;
        if text.contains(['\n', '\t']) {
            self.split_controls(node)?;
        }
        Ok(())
    }

    /// Turns `\n` and `\t` in a paragraph's runs into line breaks and tabs
    /// (one `SetInlines`), so inserted text means what a reader expects.
    fn split_controls(&mut self, paragraph: NodeId) -> Result<()> {
        let Some(p) = find_paragraph(&self.state.model, paragraph) else {
            return Ok(());
        };
        let inlines = p.inlines.clone();
        let mut ids = crate::template::IdSource(self);
        let rebuilt = crate::template::split_controls(&inlines, &mut ids)?;
        self.apply(vec![Operation::SetInlines {
            node: paragraph,
            inlines: rebuilt,
        }])
    }

    /// Deletes characters `[start, end)` of a paragraph.
    ///
    /// # Errors
    ///
    /// As [`Editor::insert_text`].
    pub fn delete_text(&mut self, paragraph: &str, start: usize, end: usize) -> Result<()> {
        let (node, start, end) = self.range(paragraph, start, end)?;
        self.apply(vec![Operation::DeleteText {
            range: Range {
                start: Pos::new(node, start),
                end: Pos::new(node, end),
            },
        }])
    }

    /// Replaces characters `[start, end)` with `text`.
    ///
    /// # Errors
    ///
    /// As [`Editor::insert_text`].
    pub fn replace_text(
        &mut self,
        paragraph: &str,
        start: usize,
        end: usize,
        text: &str,
    ) -> Result<()> {
        self.delete_text(paragraph, start, end)?;
        self.insert_text(paragraph, start, text)
    }

    /// Replaces every occurrence of `find` with `replace` in every paragraph
    /// of every story (body, tables, headers, footers, notes, comments, text
    /// boxes). Matches inside one paragraph; a match spanning a paragraph
    /// break is not one. Returns the number replaced. Aspose's
    /// `Range.Replace(string, string)`.
    ///
    /// **O(document)** for the scan, plus one delete and one insert per match.
    ///
    /// # Errors
    ///
    /// An empty `find`, or an engine refusal (for example inside a locked
    /// content control).
    pub fn replace_all(&mut self, find: &str, replace: &str) -> Result<usize> {
        if find.is_empty() {
            return Err(Error::invalid("the text to find is empty"));
        }
        let mut targets: Vec<(NodeId, Vec<usize>)> = Vec::new();
        casual_doc_edit::visit_paragraphs_all_surfaces(&self.state.model, &mut |p| {
            let text = node_plain_text(&p.inlines);
            let hits: Vec<usize> = text.match_indices(find).map(|(i, _)| i).collect();
            if !hits.is_empty() {
                targets.push((p.id, hits));
            }
        });
        let mut count = 0;
        let length = u32::try_from(find.len()).map_err(|_| Error::invalid("find too long"))?;
        for (node, hits) in targets {
            // Last first, so earlier offsets stay valid.
            for start in hits.into_iter().rev() {
                let start = u32::try_from(start).map_err(|_| Error::invalid("too long"))?;
                let mut ops = vec![Operation::DeleteText {
                    range: Range {
                        start: Pos::new(node, start),
                        end: Pos::new(node, start + length),
                    },
                }];
                if !replace.is_empty() {
                    ops.push(Operation::InsertText {
                        at: Pos::new(node, start),
                        text: replace.to_owned(),
                    });
                }
                self.apply(ops)?;
                count += 1;
            }
        }
        Ok(count)
    }

    /// Sets character formatting over characters `[start, end)`.
    ///
    /// # Errors
    ///
    /// As [`Editor::insert_text`], or an invalid colour/size.
    pub fn set_run_format(
        &mut self,
        paragraph: &str,
        start: usize,
        end: usize,
        format: &RunFormat,
    ) -> Result<()> {
        let (node, start, end) = self.range(paragraph, start, end)?;
        let delta = format.delta()?;
        self.apply(vec![Operation::FormatText {
            range: Range {
                start: Pos::new(node, start),
                end: Pos::new(node, end),
            },
            delta,
        }])
    }

    /// Removes direct character formatting over `[start, end)`.
    ///
    /// # Errors
    ///
    /// As [`Editor::insert_text`].
    pub fn clear_formatting(&mut self, paragraph: &str, start: usize, end: usize) -> Result<()> {
        let (node, start, end) = self.range(paragraph, start, end)?;
        self.apply(vec![Operation::ClearFormatting {
            range: Range {
                start: Pos::new(node, start),
                end: Pos::new(node, end),
            },
        }])
    }

    /// Links characters `[start, end)` to a URL, or to `#bookmark`.
    ///
    /// # Errors
    ///
    /// As [`Editor::insert_text`].
    pub fn set_hyperlink(
        &mut self,
        paragraph: &str,
        start: usize,
        end: usize,
        target: &str,
    ) -> Result<()> {
        let (node, start, end) = self.range(paragraph, start, end)?;
        let target = match target.strip_prefix('#') {
            Some(anchor) => HyperlinkTarget::Internal(InternalTarget {
                anchor: anchor.to_owned(),
            }),
            None => HyperlinkTarget::External(ExternalTarget {
                url: target.to_owned(),
                anchor: None,
            }),
        };
        let id = self.fresh()?;
        self.apply(vec![Operation::SetHyperlink {
            range: Range {
                start: Pos::new(node, start),
                end: Pos::new(node, end),
            },
            id,
            target: Some(target),
            tooltip: None,
        }])
    }

    // -- paragraphs --

    /// A paragraph style's id by display name or id, case-insensitively and
    /// ignoring spaces (`"Heading 1"`, `"heading1"`).
    fn style_id(&self, name: &str, kind: StyleKind) -> Result<StyleId> {
        let squash = |s: &str| s.replace(' ', "").to_ascii_lowercase();
        let wanted = squash(name);
        self.state
            .model
            .definitions()
            .styles
            .iter()
            .find(|(id, style)| {
                style.kind == kind
                    && (style.name.as_deref().is_some_and(|n| squash(n) == wanted)
                        || id.node_id().to_string() == name)
            })
            .map(|(id, _)| *id)
            .ok_or_else(|| Error::invalid(format!("the document has no style named \"{name}\"")))
    }

    fn paragraph_properties(&self, node: NodeId) -> Result<ParagraphProperties> {
        find_paragraph(&self.state.model, node)
            .map(|p| (*p.properties).clone())
            .ok_or_else(|| Error::invalid(format!("no paragraph with id {node}")))
    }

    /// Applies a paragraph style by name (`"Heading 1"`).
    ///
    /// # Errors
    ///
    /// An unknown paragraph or style.
    pub fn set_paragraph_style(&mut self, paragraph: &str, style: &str) -> Result<()> {
        let node = parse_id(paragraph)?;
        let mut properties = self.paragraph_properties(node)?;
        properties.style_ref = Some(self.style_id(style, StyleKind::Paragraph)?);
        self.apply(vec![Operation::SetParagraphProperties {
            node,
            properties: Box::new(properties),
        }])
    }

    /// Sets a paragraph's alignment: `"left"`/`"start"`, `"center"`,
    /// `"right"`/`"end"`, `"justify"`.
    ///
    /// # Errors
    ///
    /// An unknown paragraph or alignment.
    pub fn set_alignment(&mut self, paragraph: &str, alignment: &str) -> Result<()> {
        let node = parse_id(paragraph)?;
        let mut properties = self.paragraph_properties(node)?;
        properties.alignment = Some(match alignment.to_ascii_lowercase().as_str() {
            "left" | "start" => Alignment::Start,
            "center" => Alignment::Center,
            "right" | "end" => Alignment::End,
            "justify" | "justified" | "both" => Alignment::Justify,
            other => return Err(Error::invalid(format!("\"{other}\" is not an alignment"))),
        });
        self.apply(vec![Operation::SetParagraphProperties {
            node,
            properties: Box::new(properties),
        }])
    }

    /// Sets or clears "page break before".
    ///
    /// # Errors
    ///
    /// An unknown paragraph.
    pub fn set_page_break_before(&mut self, paragraph: &str, on: bool) -> Result<()> {
        let node = parse_id(paragraph)?;
        let mut properties = self.paragraph_properties(node)?;
        properties.page_break_before = on.then_some(true);
        self.apply(vec![Operation::SetParagraphProperties {
            node,
            properties: Box::new(properties),
        }])
    }

    /// The container and index a position names.
    fn resolve(&self, at: &Position) -> Result<(Option<NodeId>, u32)> {
        let body_len = u32::try_from(self.state.model.body().len()).unwrap_or(u32::MAX);
        match at {
            Position::Start => Ok((None, 0)),
            Position::End => Ok((None, body_len)),
            Position::Before(id) | Position::After(id) => {
                let node = parse_id(id)?;
                let (container, index) = locate_block(self.state.model.body(), node, None)
                    .ok_or_else(|| {
                        Error::invalid(format!("no block with id {id} in the body or its tables"))
                    })?;
                let index = u32::try_from(index).unwrap_or(u32::MAX);
                Ok((
                    container,
                    if matches!(at, Position::After(_)) {
                        index + 1
                    } else {
                        index
                    },
                ))
            }
        }
    }

    pub(crate) fn new_paragraph(
        &mut self,
        text: &str,
        style: Option<StyleId>,
    ) -> Result<Paragraph> {
        let id = self.fresh()?;
        let mut inlines = Vec::new();
        if !text.is_empty() {
            inlines.push(InlineNode::Run(Run {
                id: self.fresh()?,
                properties: RunProperties::default().into(),
                text: text.to_owned(),
            }));
        }
        Ok(Paragraph {
            id,
            properties: ParagraphProperties {
                style_ref: style,
                ..ParagraphProperties::default()
            }
            .into(),
            inlines,
        })
    }

    /// Inserts a paragraph and returns its id. python-docx's
    /// `add_paragraph(text, style)` is `insert_paragraph(Position::End, ...)`.
    ///
    /// # Errors
    ///
    /// An unknown anchor block or style.
    pub fn insert_paragraph(
        &mut self,
        at: &Position,
        text: &str,
        style: Option<&str>,
    ) -> Result<String> {
        let style = style
            .map(|s| self.style_id(s, StyleKind::Paragraph))
            .transpose()?;
        let (container, index) = self.resolve(at)?;
        let paragraph = self.new_paragraph(text, style)?;
        let id = paragraph.id.to_string();
        self.apply(vec![Operation::InsertBlocks {
            container,
            index,
            blocks: vec![BlockNode::Paragraph(paragraph)],
        }])?;
        Ok(id)
    }

    /// Appends a paragraph to the body and returns its id.
    ///
    /// # Errors
    ///
    /// An unknown style.
    pub fn append_paragraph(&mut self, text: &str, style: Option<&str>) -> Result<String> {
        self.insert_paragraph(&Position::End, text, style)
    }

    /// Appends a heading (`level` 0 is Title, 1-9 Heading N), python-docx's
    /// `add_heading`.
    ///
    /// # Errors
    ///
    /// A level over 9, or a document without that heading style.
    pub fn append_heading(&mut self, text: &str, level: u8) -> Result<String> {
        let style = match level {
            0 => "Title".to_owned(),
            1..=9 => format!("Heading {level}"),
            _ => return Err(Error::invalid(format!("heading level {level} is not 0-9"))),
        };
        self.append_paragraph(text, Some(&style))
    }

    /// Appends a run of text to the end of a paragraph, with formatting, and
    /// returns nothing: python-docx's `paragraph.add_run(text)` then setting
    /// `run.bold`. The run's characters are `[old_len, old_len + text_len)`.
    ///
    /// # Errors
    ///
    /// An unknown paragraph, or an invalid format.
    pub fn append_run(&mut self, paragraph: &str, text: &str, format: &RunFormat) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        let node = parse_id(paragraph)?;
        let current = self.paragraph_text(node)?;
        let start = current.chars().count();
        // A new run, not an extension of the previous one: inserting at the end
        // would inherit the last run's formatting, so the format is set on the
        // new range explicitly, clearing what was inherited.
        self.insert_text(paragraph, start, text)?;
        let end = start + text.chars().count();
        let inherited =
            find_paragraph(&self.state.model, node).is_some_and(|p| p.inlines.len() > 1);
        if inherited {
            self.clear_formatting(paragraph, start, end)?;
        }
        if *format != RunFormat::default() {
            self.set_run_format(paragraph, start, end, format)?;
        }
        Ok(())
    }

    /// Deletes a block (paragraph or table) from the body, a cell or a
    /// content control.
    ///
    /// # Errors
    ///
    /// An unknown block, or the last block of a cell.
    pub fn delete_block(&mut self, id: &str) -> Result<()> {
        let node = parse_id(id)?;
        let (container, index) = locate_block(self.state.model.body(), node, None)
            .ok_or_else(|| Error::invalid(format!("no block with id {id}")))?;
        self.apply(vec![Operation::DeleteBlocks {
            container,
            index: u32::try_from(index).unwrap_or(u32::MAX),
            count: 1,
        }])
    }

    // -- tables --

    /// Inserts a `rows` × `columns` table, filled from `cells` (row-major,
    /// missing cells empty), and returns its id. Columns share a 6.5-inch
    /// text width; the document's "Table Grid" style is applied when it has
    /// one.
    ///
    /// # Errors
    ///
    /// Zero rows or columns, or an unknown anchor.
    pub fn insert_table(
        &mut self,
        at: &Position,
        rows: usize,
        columns: usize,
        cells: &[Vec<String>],
    ) -> Result<String> {
        if rows == 0 || columns == 0 || rows > 4_096 || columns > 63 {
            return Err(Error::invalid(format!(
                "a table is 1-4096 rows by 1-63 columns, not {rows} by {columns}"
            )));
        }
        let (container, index) = self.resolve(at)?;
        let width = 9_360 / i32::try_from(columns).unwrap_or(1);
        let style = self.style_id("Table Grid", StyleKind::Table).ok();
        let mut table_rows = Vec::with_capacity(rows);
        for r in 0..rows {
            let mut row_cells = Vec::with_capacity(columns);
            for c in 0..columns {
                let text = cells
                    .get(r)
                    .and_then(|row| row.get(c))
                    .map_or("", String::as_str);
                let paragraph = self.new_paragraph(text, None)?;
                row_cells.push(TableCell {
                    id: self.fresh()?,
                    properties: TableCellProperties {
                        width: Some(TableWidth {
                            value: width,
                            width_type: WidthType::Dxa,
                        }),
                        ..TableCellProperties::default()
                    },
                    blocks: vec![BlockNode::Paragraph(paragraph)],
                });
            }
            table_rows.push(TableRow {
                id: self.fresh()?,
                properties: TableRowProperties::default(),
                cells: row_cells,
            });
        }
        let table = Table {
            id: self.fresh()?,
            grid: vec![
                GridColumn {
                    width_twips: Some(width)
                };
                columns
            ],
            grid_change: None,
            properties: TableProperties {
                style_ref: style,
                ..TableProperties::default()
            },
            rows: table_rows,
        };
        let id = table.id.to_string();
        self.apply(vec![Operation::InsertBlocks {
            container,
            index,
            blocks: vec![BlockNode::Table(Box::new(table))],
        }])?;
        Ok(id)
    }

    fn table(&self, id: NodeId) -> Result<&Table> {
        casual_doc_edit::find_table(&self.state.model, id)
            .ok_or_else(|| Error::invalid(format!("no table with id {id}")))
    }

    /// Replaces a cell's text: its first paragraph's text, the rest of its
    /// blocks removed. POI's `cell.setText`, python-docx's `cell.text = ...`.
    ///
    /// # Errors
    ///
    /// An unknown table, or a cell index past the row.
    pub fn set_cell_text(
        &mut self,
        table: &str,
        row: usize,
        column: usize,
        text: &str,
    ) -> Result<()> {
        let table_id = parse_id(table)?;
        let (cell_id, blocks) = {
            let t = self.table(table_id)?;
            let cell = t
                .rows
                .get(row)
                .and_then(|r| r.cells.get(column))
                .ok_or_else(|| Error::invalid(format!("no cell ({row}, {column}) in the table")))?;
            (cell.id, cell.blocks.clone())
        };
        // Remove every block after the first, then rewrite the first.
        if blocks.len() > 1 {
            self.apply(vec![Operation::DeleteBlocks {
                container: Some(cell_id),
                index: 1,
                count: u32::try_from(blocks.len() - 1).unwrap_or(u32::MAX),
            }])?;
        }
        match blocks.first() {
            Some(BlockNode::Paragraph(p)) => {
                // The cell's paragraph is reached through its table's route, so
                // the edits below do not walk the document to find it (`docs/176`
                // §12.10). O(1); a stale hint is only a miss.
                let _ = self
                    .state
                    .model
                    .hint_cell_paragraph(table_id, row, column, 0);
                let id = p.id.to_string();
                let length = node_plain_text(&p.inlines).chars().count();
                if length > 0 {
                    self.delete_text(&id, 0, length)?;
                }
                self.insert_text(&id, 0, text)
            }
            _ => {
                let paragraph = self.new_paragraph(text, None)?;
                self.apply(vec![
                    Operation::InsertBlocks {
                        container: Some(cell_id),
                        index: 0,
                        blocks: vec![BlockNode::Paragraph(paragraph)],
                    },
                    Operation::DeleteBlocks {
                        container: Some(cell_id),
                        index: 1,
                        count: 1,
                    },
                ])
            }
        }
    }

    /// Appends a row copying the last row's cell layout, with `cells` as its
    /// text, and returns its id. python-docx's `table.add_row()`.
    ///
    /// # Errors
    ///
    /// An unknown table.
    pub fn add_row(&mut self, table: &str, cells: &[String]) -> Result<String> {
        let table_id = parse_id(table)?;
        let (index, template) = {
            let t = self.table(table_id)?;
            let last = t
                .rows
                .last()
                .ok_or_else(|| Error::invalid("the table has no rows"))?;
            (t.rows.len(), last.clone())
        };
        let mut new_cells = Vec::with_capacity(template.cells.len());
        for (i, cell) in template.cells.iter().enumerate() {
            let text = cells.get(i).map_or("", String::as_str);
            let paragraph = self.new_paragraph(text, None)?;
            new_cells.push(TableCell {
                id: self.fresh()?,
                properties: TableCellProperties {
                    vertical_merge: None,
                    ..cell.properties.clone()
                },
                blocks: vec![BlockNode::Paragraph(paragraph)],
            });
        }
        let row = TableRow {
            id: self.fresh()?,
            properties: TableRowProperties {
                header: false,
                ..template.properties.clone()
            },
            cells: new_cells,
        };
        let id = row.id.to_string();
        self.apply(vec![Operation::InsertRow {
            table: table_id,
            index: u32::try_from(index).unwrap_or(u32::MAX),
            row: Box::new(row),
        }])?;
        Ok(id)
    }

    /// Deletes a row by index.
    ///
    /// # Errors
    ///
    /// An unknown table, an index past the end, or the table's last row.
    pub fn delete_row(&mut self, table: &str, index: usize) -> Result<()> {
        let table = parse_id(table)?;
        self.apply(vec![Operation::DeleteRow {
            table,
            index: u32::try_from(index).unwrap_or(u32::MAX),
        }])
    }

    // -- review --

    fn review_ids(&mut self) -> (ReviewIds<'_>, &Model) {
        let EditState {
            model,
            ids,
            revisions,
        } = self.state;
        (
            ReviewIds {
                nodes: ids,
                revisions,
            },
            model,
        )
    }

    /// Adds a comment on characters `[start, end)` of a paragraph and returns
    /// its id.
    ///
    /// # Errors
    ///
    /// An unknown paragraph or an empty range.
    pub fn add_comment(
        &mut self,
        paragraph: &str,
        start: usize,
        end: usize,
        text: &str,
        date: Option<&str>,
    ) -> Result<String> {
        let (node, start, end) = self.range(paragraph, start, end)?;
        let author = self.author.clone();
        let initials: String = author
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .collect();
        let before: Vec<_> = self
            .state
            .model
            .definitions()
            .comments
            .iter()
            .map(|(id, _)| *id)
            .collect();
        let node = node.to_string();
        let (mut ids, model) = self.review_ids();
        let lowered = lower::add_comment(
            model,
            &mut ids,
            &node,
            start,
            &node,
            end,
            text,
            Some(author),
            (!initials.is_empty()).then_some(initials),
            date.map(str::to_owned),
        )
        .map_err(Error::edit)?;
        self.apply(lowered.operations)?;
        self.state
            .model
            .definitions()
            .comments
            .iter()
            .map(|(id, _)| *id)
            .find(|id| !before.contains(id))
            .map(|id| id.node_id().to_string())
            .ok_or_else(|| Error::edit("the comment was not created"))
    }

    /// Inserts `text` as a tracked insertion (a suggestion) by the editor's
    /// author.
    ///
    /// # Errors
    ///
    /// As [`Editor::insert_text`].
    pub fn suggest_insert(
        &mut self,
        paragraph: &str,
        offset: usize,
        text: &str,
        date: Option<&str>,
    ) -> Result<()> {
        let node = parse_id(paragraph)?;
        let current = self.paragraph_text(node)?;
        let offset = Self::byte_offset(&current, offset)?;
        let author = self.author.clone();
        let node = node.to_string();
        let (mut ids, model) = self.review_ids();
        let lowered = lower::suggest_insert(
            model,
            &mut ids,
            &node,
            offset,
            text,
            InsertStyle::Plain,
            Some(author),
            date.map(str::to_owned),
            None,
        )
        .map_err(Error::edit)?;
        self.apply(lowered.operations)
    }

    /// Marks characters `[start, end)` as a tracked deletion.
    ///
    /// # Errors
    ///
    /// As [`Editor::insert_text`].
    pub fn suggest_delete(
        &mut self,
        paragraph: &str,
        start: usize,
        end: usize,
        date: Option<&str>,
    ) -> Result<()> {
        let (node, start, end) = self.range(paragraph, start, end)?;
        let author = self.author.clone();
        let node = node.to_string();
        let (mut ids, model) = self.review_ids();
        let lowered = lower::suggest_delete(
            model,
            &mut ids,
            &node,
            start,
            end,
            Some(author),
            date.map(str::to_owned),
        )
        .map_err(Error::edit)?;
        self.apply(lowered.operations)
    }

    /// Replaces characters `[start, end)` as a tracked deletion plus a tracked
    /// insertion.
    ///
    /// # Errors
    ///
    /// As [`Editor::insert_text`].
    pub fn suggest_replace(
        &mut self,
        paragraph: &str,
        start: usize,
        end: usize,
        text: &str,
        date: Option<&str>,
    ) -> Result<()> {
        let (node, start, end) = self.range(paragraph, start, end)?;
        let author = self.author.clone();
        let node = node.to_string();
        let (mut ids, model) = self.review_ids();
        let lowered = lower::suggest_replace(
            model,
            &mut ids,
            &node,
            start,
            end,
            text,
            Some(author),
            date.map(str::to_owned),
        )
        .map_err(Error::edit)?;
        self.apply(lowered.operations)
    }

    /// Accepts every tracked change on every surface. Returns whether there
    /// was any.
    ///
    /// # Errors
    ///
    /// An engine refusal.
    pub fn accept_all_revisions(&mut self) -> Result<bool> {
        self.decide_all(true)
    }

    /// Rejects every tracked change on every surface.
    ///
    /// # Errors
    ///
    /// An engine refusal.
    pub fn reject_all_revisions(&mut self) -> Result<bool> {
        self.decide_all(false)
    }

    fn decide_all(&mut self, accept: bool) -> Result<bool> {
        match lower::decide_all_revisions(&self.state.model, accept) {
            Ok(lowered) if lowered.operations.is_empty() => Ok(false),
            Ok(lowered) => {
                self.apply(lowered.operations)?;
                Ok(true)
            }
            // "Nothing to decide" is not a failure of a batch call.
            Err(message) if message.contains("no ") || message.contains("No ") => Ok(false),
            Err(message) => Err(Error::edit(message)),
        }
    }

    /// Sets core document properties; `None` fields keep their value.
    ///
    /// # Errors
    ///
    /// An engine refusal.
    pub fn set_properties(&mut self, patch: &PropertiesPatch) -> Result<()> {
        let mut core = self
            .state
            .model
            .properties()
            .map(|p| p.core.clone())
            .unwrap_or_default();
        let set = |slot: &mut Option<String>, value: &Option<String>| {
            if let Some(value) = value {
                *slot = (!value.is_empty()).then(|| value.clone());
            }
        };
        set(&mut core.title, &patch.title);
        set(&mut core.subject, &patch.subject);
        set(&mut core.creator, &patch.creator);
        set(&mut core.keywords, &patch.keywords);
        set(&mut core.description, &patch.description);
        set(&mut core.last_modified_by, &patch.last_modified_by);
        set(&mut core.category, &patch.category);
        self.apply(vec![Operation::SetCoreProperties {
            properties: Box::new(CoreProperties { ..core }),
        }])
    }
}

/// Core properties to set. `Some("")` clears a property.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertiesPatch {
    /// Title.
    #[serde(default)]
    pub title: Option<String>,
    /// Subject.
    #[serde(default)]
    pub subject: Option<String>,
    /// Author.
    #[serde(default, alias = "author")]
    pub creator: Option<String>,
    /// Keywords.
    #[serde(default)]
    pub keywords: Option<String>,
    /// Description (comments).
    #[serde(default)]
    pub description: Option<String>,
    /// Last modified by.
    #[serde(default)]
    pub last_modified_by: Option<String>,
    /// Category.
    #[serde(default)]
    pub category: Option<String>,
}

/// The paragraph `id`, on any surface.
///
/// The last few body blocks are checked first, because a builder loop
/// (`add_paragraph`, then `add_run`) addresses the paragraph it just appended,
/// and a new paragraph is a cold miss in the model's route cache. Everything
/// else goes to `casual_doc_edit::find_paragraph_any`, which is the model's
/// route-cached locator (`Document::paragraph`): **O(1)** warm, one walk cold.
pub(crate) fn find_paragraph(model: &Model, id: NodeId) -> Option<&Paragraph> {
    let body = model.body();
    for block in body.iter().rev().take(8) {
        if let BlockNode::Paragraph(p) = block
            && p.id == id
        {
            return Some(p);
        }
    }
    casual_doc_edit::find_paragraph_any(model, id)
}

/// Finds the container (`None` for the body, else a cell or content control)
/// and index of the block `id`. **O(blocks)**.
pub(crate) fn locate_block(
    blocks: &[BlockNode],
    id: NodeId,
    container: Option<NodeId>,
) -> Option<(Option<NodeId>, usize)> {
    for (index, block) in blocks.iter().enumerate() {
        let own = match block {
            BlockNode::Paragraph(p) => p.id,
            BlockNode::Table(t) => t.id,
            BlockNode::Sdt(s) => s.id,
            BlockNode::AltChunk(a) => a.id,
        };
        if own == id {
            return Some((container, index));
        }
        match block {
            BlockNode::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        if let Some(found) = locate_block(&cell.blocks, id, Some(cell.id)) {
                            return Some(found);
                        }
                    }
                }
            }
            BlockNode::Sdt(s) => {
                if let Some(found) = locate_block(&s.blocks, id, Some(s.id)) {
                    return Some(found);
                }
            }
            BlockNode::Paragraph(_) | BlockNode::AltChunk(_) => {}
        }
    }
    None
}

/// One edit as data: the JSON form every binding sends (`{"op": "insertText", ...}`).
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
#[non_exhaustive]
pub enum EditCommand {
    /// [`Editor::insert_text`].
    InsertText {
        /// Paragraph id.
        paragraph: String,
        /// Character offset.
        offset: usize,
        /// Text.
        text: String,
    },
    /// [`Editor::delete_text`].
    DeleteText {
        /// Paragraph id.
        paragraph: String,
        /// Start.
        start: usize,
        /// End.
        end: usize,
    },
    /// [`Editor::replace_text`].
    ReplaceText {
        /// Paragraph id.
        paragraph: String,
        /// Start.
        start: usize,
        /// End.
        end: usize,
        /// Text.
        text: String,
    },
    /// [`Editor::replace_all`].
    ReplaceAll {
        /// Find.
        find: String,
        /// Replace.
        replace: String,
    },
    /// [`Editor::set_run_format`].
    SetRunFormat {
        /// Paragraph id.
        paragraph: String,
        /// Start.
        start: usize,
        /// End.
        end: usize,
        /// Format.
        format: RunFormat,
    },
    /// [`Editor::clear_formatting`].
    ClearFormatting {
        /// Paragraph id.
        paragraph: String,
        /// Start.
        start: usize,
        /// End.
        end: usize,
    },
    /// [`Editor::set_hyperlink`].
    SetHyperlink {
        /// Paragraph id.
        paragraph: String,
        /// Start.
        start: usize,
        /// End.
        end: usize,
        /// URL or `#bookmark`.
        target: String,
    },
    /// [`Editor::set_paragraph_style`].
    SetParagraphStyle {
        /// Paragraph id.
        paragraph: String,
        /// Style name.
        style: String,
    },
    /// [`Editor::set_alignment`].
    SetAlignment {
        /// Paragraph id.
        paragraph: String,
        /// Alignment.
        alignment: String,
    },
    /// [`Editor::set_page_break_before`].
    SetPageBreakBefore {
        /// Paragraph id.
        paragraph: String,
        /// On.
        on: bool,
    },
    /// [`Editor::insert_paragraph`].
    InsertParagraph {
        /// `"start"`, `"end"`, or omitted for the end.
        #[serde(default)]
        at: Option<String>,
        /// Insert before this block.
        #[serde(default)]
        before: Option<String>,
        /// Insert after this block.
        #[serde(default)]
        after: Option<String>,
        /// Text.
        #[serde(default)]
        text: String,
        /// Style name.
        #[serde(default)]
        style: Option<String>,
    },
    /// [`Editor::append_run`].
    AppendRun {
        /// Paragraph id.
        paragraph: String,
        /// Text.
        text: String,
        /// Format.
        #[serde(default)]
        format: RunFormat,
    },
    /// [`Editor::delete_block`].
    DeleteBlock {
        /// Block id.
        id: String,
    },
    /// [`Editor::insert_table`].
    InsertTable {
        /// `"start"` or `"end"`.
        #[serde(default)]
        at: Option<String>,
        /// Before this block.
        #[serde(default)]
        before: Option<String>,
        /// After this block.
        #[serde(default)]
        after: Option<String>,
        /// Rows.
        rows: usize,
        /// Columns.
        columns: usize,
        /// Cell text, row-major.
        #[serde(default)]
        cells: Vec<Vec<String>>,
    },
    /// [`Editor::set_cell_text`].
    SetCellText {
        /// Table id.
        table: String,
        /// Row.
        row: usize,
        /// Column.
        column: usize,
        /// Text.
        text: String,
    },
    /// [`Editor::add_row`].
    AddRow {
        /// Table id.
        table: String,
        /// Cell text.
        #[serde(default)]
        cells: Vec<String>,
    },
    /// [`Editor::delete_row`].
    DeleteRow {
        /// Table id.
        table: String,
        /// Row index.
        index: usize,
    },
    /// [`Editor::add_comment`].
    AddComment {
        /// Paragraph id.
        paragraph: String,
        /// Start.
        start: usize,
        /// End.
        end: usize,
        /// Text.
        text: String,
        /// Date, ISO 8601.
        #[serde(default)]
        date: Option<String>,
    },
    /// [`Editor::suggest_insert`].
    SuggestInsert {
        /// Paragraph id.
        paragraph: String,
        /// Offset.
        offset: usize,
        /// Text.
        text: String,
        /// Date.
        #[serde(default)]
        date: Option<String>,
    },
    /// [`Editor::suggest_delete`].
    SuggestDelete {
        /// Paragraph id.
        paragraph: String,
        /// Start.
        start: usize,
        /// End.
        end: usize,
        /// Date.
        #[serde(default)]
        date: Option<String>,
    },
    /// [`Editor::suggest_replace`].
    SuggestReplace {
        /// Paragraph id.
        paragraph: String,
        /// Start.
        start: usize,
        /// End.
        end: usize,
        /// Text.
        text: String,
        /// Date.
        #[serde(default)]
        date: Option<String>,
    },
    /// [`Editor::accept_all_revisions`].
    AcceptAllRevisions,
    /// [`Editor::reject_all_revisions`].
    RejectAllRevisions,
    /// [`Editor::set_properties`].
    SetProperties {
        /// The patch.
        properties: PropertiesPatch,
    },
}

fn position(at: Option<&str>, before: Option<&String>, after: Option<&String>) -> Result<Position> {
    Ok(match (at, before, after) {
        (_, Some(id), None) => Position::Before(id.clone()),
        (_, None, Some(id)) => Position::After(id.clone()),
        (Some("start"), None, None) => Position::Start,
        (Some("end") | None, None, None) => Position::End,
        _ => {
            return Err(Error::invalid(
                "a position is one of at: \"start\" | \"end\", before: id, after: id",
            ));
        }
    })
}

impl Editor<'_> {
    /// Runs one [`EditCommand`]; returns what the command returns (a new id,
    /// a count, a flag), as JSON.
    ///
    /// # Errors
    ///
    /// The command's own errors.
    pub fn run(&mut self, command: &EditCommand) -> Result<serde_json::Value> {
        use serde_json::Value;
        Ok(match command {
            EditCommand::InsertText {
                paragraph,
                offset,
                text,
            } => {
                self.insert_text(paragraph, *offset, text)?;
                Value::Null
            }
            EditCommand::DeleteText {
                paragraph,
                start,
                end,
            } => {
                self.delete_text(paragraph, *start, *end)?;
                Value::Null
            }
            EditCommand::ReplaceText {
                paragraph,
                start,
                end,
                text,
            } => {
                self.replace_text(paragraph, *start, *end, text)?;
                Value::Null
            }
            EditCommand::ReplaceAll { find, replace } => {
                Value::from(self.replace_all(find, replace)?)
            }
            EditCommand::SetRunFormat {
                paragraph,
                start,
                end,
                format,
            } => {
                self.set_run_format(paragraph, *start, *end, format)?;
                Value::Null
            }
            EditCommand::ClearFormatting {
                paragraph,
                start,
                end,
            } => {
                self.clear_formatting(paragraph, *start, *end)?;
                Value::Null
            }
            EditCommand::SetHyperlink {
                paragraph,
                start,
                end,
                target,
            } => {
                self.set_hyperlink(paragraph, *start, *end, target)?;
                Value::Null
            }
            EditCommand::SetParagraphStyle { paragraph, style } => {
                self.set_paragraph_style(paragraph, style)?;
                Value::Null
            }
            EditCommand::SetAlignment {
                paragraph,
                alignment,
            } => {
                self.set_alignment(paragraph, alignment)?;
                Value::Null
            }
            EditCommand::SetPageBreakBefore { paragraph, on } => {
                self.set_page_break_before(paragraph, *on)?;
                Value::Null
            }
            EditCommand::InsertParagraph {
                at,
                before,
                after,
                text,
                style,
            } => {
                let at = position(at.as_deref(), before.as_ref(), after.as_ref())?;
                Value::from(self.insert_paragraph(&at, text, style.as_deref())?)
            }
            EditCommand::AppendRun {
                paragraph,
                text,
                format,
            } => {
                self.append_run(paragraph, text, format)?;
                Value::Null
            }
            EditCommand::DeleteBlock { id } => {
                self.delete_block(id)?;
                Value::Null
            }
            EditCommand::InsertTable {
                at,
                before,
                after,
                rows,
                columns,
                cells,
            } => {
                let at = position(at.as_deref(), before.as_ref(), after.as_ref())?;
                Value::from(self.insert_table(&at, *rows, *columns, cells)?)
            }
            EditCommand::SetCellText {
                table,
                row,
                column,
                text,
            } => {
                self.set_cell_text(table, *row, *column, text)?;
                Value::Null
            }
            EditCommand::AddRow { table, cells } => Value::from(self.add_row(table, cells)?),
            EditCommand::DeleteRow { table, index } => {
                self.delete_row(table, *index)?;
                Value::Null
            }
            EditCommand::AddComment {
                paragraph,
                start,
                end,
                text,
                date,
            } => Value::from(self.add_comment(paragraph, *start, *end, text, date.as_deref())?),
            EditCommand::SuggestInsert {
                paragraph,
                offset,
                text,
                date,
            } => {
                self.suggest_insert(paragraph, *offset, text, date.as_deref())?;
                Value::Null
            }
            EditCommand::SuggestDelete {
                paragraph,
                start,
                end,
                date,
            } => {
                self.suggest_delete(paragraph, *start, *end, date.as_deref())?;
                Value::Null
            }
            EditCommand::SuggestReplace {
                paragraph,
                start,
                end,
                text,
                date,
            } => {
                self.suggest_replace(paragraph, *start, *end, text, date.as_deref())?;
                Value::Null
            }
            EditCommand::AcceptAllRevisions => Value::from(self.accept_all_revisions()?),
            EditCommand::RejectAllRevisions => Value::from(self.reject_all_revisions()?),
            EditCommand::SetProperties { properties } => {
                self.set_properties(properties)?;
                Value::Null
            }
        })
    }
}
