//! Inline and block body content for the normalized schema v0 model.

use std::collections::BTreeSet;

use serde::de;
use serde::{Deserialize, Deserializer, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::{ModelError, NodeId};

/// Inline formatting represented in deterministic enum order.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mark {
    /// Bold text.
    Bold,
    /// Italic text.
    Italic,
    /// Underlined text.
    Underline,
    /// Struck-through text.
    Strike,
}

/// A contiguous text span sharing one mark set.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRun {
    pub(crate) text: String,
    #[serde(deserialize_with = "deserialize_marks")]
    pub(crate) marks: BTreeSet<Mark>,
}

impl TextRun {
    /// Creates a non-empty text run.
    pub fn new(text: impl Into<String>, marks: BTreeSet<Mark>) -> Result<Self, ModelError> {
        let text = text.into();
        if text.is_empty() {
            return Err(ModelError::EmptyTextRun);
        }
        Ok(Self { text, marks })
    }

    /// Returns the run text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the run marks.
    #[must_use]
    pub const fn marks(&self) -> &BTreeSet<Mark> {
        &self.marks
    }
}

/// Inline content supported by normalized schema v0.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InlineNode {
    /// A text run.
    Text(TextRun),
}

/// A paragraph in the normalized document body.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Paragraph {
    pub(crate) id: NodeId,
    pub(crate) inlines: Vec<InlineNode>,
}

impl Paragraph {
    /// Creates an empty paragraph.
    #[must_use]
    pub const fn empty(id: NodeId) -> Self {
        Self {
            id,
            inlines: Vec::new(),
        }
    }

    /// Returns the paragraph ID.
    #[must_use]
    pub const fn id(&self) -> NodeId {
        self.id
    }

    /// Returns the paragraph inline nodes.
    #[must_use]
    pub fn inlines(&self) -> &[InlineNode] {
        &self.inlines
    }

    /// Returns paragraph text with inline boundaries removed.
    #[must_use]
    pub fn plain_text(&self) -> String {
        self.inlines
            .iter()
            .map(|inline| match inline {
                InlineNode::Text(run) => run.text(),
            })
            .collect()
    }

    /// Returns the number of extended grapheme clusters in the paragraph.
    #[must_use]
    pub fn grapheme_len(&self) -> usize {
        self.inlines
            .iter()
            .map(|inline| match inline {
                InlineNode::Text(run) => run.text().graphemes(true).count(),
            })
            .sum()
    }

    /// Inserts text at an extended-grapheme boundary and normalizes text runs.
    pub fn insert_text(
        &mut self,
        grapheme_offset: usize,
        text: String,
        marks: BTreeSet<Mark>,
    ) -> Result<(), ModelError> {
        self.insert_runs(grapheme_offset, vec![TextRun::new(text, marks)?])
    }

    /// Inserts marked runs at an extended-grapheme boundary.
    pub fn insert_runs(
        &mut self,
        grapheme_offset: usize,
        runs: Vec<TextRun>,
    ) -> Result<(), ModelError> {
        if runs.is_empty() {
            return Err(ModelError::EmptyTextRun);
        }
        self.split_inline_boundary(grapheme_offset)?;
        let index = self.boundary_index(grapheme_offset)?;
        self.inlines
            .splice(index..index, runs.into_iter().map(InlineNode::Text));
        self.normalize_runs();
        Ok(())
    }

    /// Deletes a non-empty grapheme range and returns its exact marked runs.
    pub fn delete_range(&mut self, start: usize, end: usize) -> Result<Vec<TextRun>, ModelError> {
        if start >= end {
            return Err(ModelError::InvalidGraphemeRange { start, end });
        }
        let paragraph_len = self.grapheme_len();
        if end > paragraph_len {
            return Err(ModelError::InvalidGraphemeOffset {
                offset: end,
                length: paragraph_len,
            });
        }
        self.split_inline_boundary(start)?;
        self.split_inline_boundary(end)?;
        let start_index = self.boundary_index(start)?;
        let end_index = self.boundary_index(end)?;
        let removed = self
            .inlines
            .drain(start_index..end_index)
            .map(|inline| match inline {
                InlineNode::Text(run) => run,
            })
            .collect();
        self.normalize_runs();
        Ok(removed)
    }

    /// Splits this paragraph and returns the new trailing paragraph.
    pub fn split_off(
        &mut self,
        grapheme_offset: usize,
        new_id: NodeId,
    ) -> Result<Self, ModelError> {
        self.split_inline_boundary(grapheme_offset)?;
        let index = self.boundary_index(grapheme_offset)?;
        let inlines = self.inlines.drain(index..).collect();
        self.normalize_runs();
        let mut trailing = Self {
            id: new_id,
            inlines,
        };
        trailing.normalize_runs();
        Ok(trailing)
    }

    /// Appends another paragraph's inline content and normalizes the boundary.
    pub fn append_paragraph(&mut self, mut other: Self) {
        self.inlines.append(&mut other.inlines);
        self.normalize_runs();
    }

    fn split_inline_boundary(&mut self, grapheme_offset: usize) -> Result<(), ModelError> {
        let paragraph_len = self.grapheme_len();
        if grapheme_offset > paragraph_len {
            return Err(ModelError::InvalidGraphemeOffset {
                offset: grapheme_offset,
                length: paragraph_len,
            });
        }

        let mut traversed = 0;
        for index in 0..self.inlines.len() {
            if grapheme_offset == traversed {
                return Ok(());
            }
            let InlineNode::Text(run) = &self.inlines[index];
            let run_graphemes = run.text.graphemes(true).count();
            let run_end = traversed + run_graphemes;
            if grapheme_offset < run_end {
                let local_offset = grapheme_offset - traversed;
                let split_byte = grapheme_boundary_byte(&run.text, local_offset);
                let before = run.text[..split_byte].to_owned();
                let after = run.text[split_byte..].to_owned();
                let marks = run.marks.clone();
                self.inlines.splice(
                    index..=index,
                    [
                        InlineNode::Text(TextRun {
                            text: before,
                            marks: marks.clone(),
                        }),
                        InlineNode::Text(TextRun { text: after, marks }),
                    ],
                );
                return Ok(());
            }
            traversed = run_end;
        }
        Ok(())
    }

    fn boundary_index(&self, grapheme_offset: usize) -> Result<usize, ModelError> {
        let mut traversed = 0;
        for (index, inline) in self.inlines.iter().enumerate() {
            if grapheme_offset == traversed {
                return Ok(index);
            }
            let InlineNode::Text(run) = inline;
            traversed += run.text.graphemes(true).count();
        }
        if grapheme_offset == traversed {
            return Ok(self.inlines.len());
        }
        Err(ModelError::InvalidGraphemeOffset {
            offset: grapheme_offset,
            length: traversed,
        })
    }

    fn normalize_runs(&mut self) {
        let mut normalized: Vec<InlineNode> = Vec::with_capacity(self.inlines.len());
        for inline in self.inlines.drain(..) {
            match inline {
                InlineNode::Text(run) if run.text.is_empty() => {}
                InlineNode::Text(run) => {
                    if let Some(InlineNode::Text(previous)) = normalized.last_mut() {
                        if previous.marks == run.marks {
                            previous.text.push_str(&run.text);
                            continue;
                        }
                    }
                    normalized.push(InlineNode::Text(run));
                }
            }
        }
        self.inlines = normalized;
    }
}

fn grapheme_boundary_byte(text: &str, grapheme_offset: usize) -> usize {
    if grapheme_offset == text.graphemes(true).count() {
        return text.len();
    }
    text.grapheme_indices(true)
        .nth(grapheme_offset)
        .map_or(text.len(), |(index, _)| index)
}

/// A body-level normalized node.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockNode {
    /// A paragraph block.
    Paragraph(Paragraph),
}

fn deserialize_marks<'de, D>(deserializer: D) -> Result<BTreeSet<Mark>, D::Error>
where
    D: Deserializer<'de>,
{
    let marks = Vec::<Mark>::deserialize(deserializer)?;
    let mut unique = BTreeSet::new();
    for mark in marks {
        if !unique.insert(mark) {
            return Err(de::Error::custom("duplicate text mark"));
        }
    }
    Ok(unique)
}
