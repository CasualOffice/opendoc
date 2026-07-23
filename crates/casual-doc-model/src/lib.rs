//! Normalized document values and invariants used inside the OpenDoc runtime.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use unicode_segmentation::UnicodeSegmentation;

/// The normalized document schema implemented by this crate.
pub const SCHEMA_VERSION: u32 = 0;

/// Stable identity of a logical document node.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(u128);

impl NodeId {
    /// Creates a non-zero node ID.
    pub fn new(value: u128) -> Result<Self, ModelError> {
        if value == 0 {
            return Err(ModelError::ZeroNodeId);
        }
        Ok(Self(value))
    }

    /// Creates an ID from a namespace and a local counter.
    pub fn from_parts(namespace: u64, counter: u64) -> Result<Self, ModelError> {
        Self::new((u128::from(namespace) << 64) | u128::from(counter))
    }

    /// Returns the raw numeric representation.
    #[must_use]
    pub const fn as_u128(self) -> u128 {
        self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:032x}", self.0)
    }
}

impl FromStr for NodeId {
    type Err = ModelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 32
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ModelError::InvalidNodeId);
        }

        let parsed = u128::from_str_radix(value, 16).map_err(|_| ModelError::InvalidNodeId)?;
        Self::new(parsed)
    }
}

impl Serialize for NodeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for NodeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

/// Deterministic namespace-and-counter ID source.
#[derive(Clone, Debug)]
pub struct IdGenerator {
    namespace: u64,
    next_counter: u64,
}

impl IdGenerator {
    /// Creates an ID generator whose first local counter is one.
    #[must_use]
    pub const fn new(namespace: u64) -> Self {
        Self {
            namespace,
            next_counter: 1,
        }
    }

    /// Returns the next ID or an error if the counter is exhausted.
    pub fn next_id(&mut self) -> Result<NodeId, ModelError> {
        if self.next_counter == u64::MAX {
            return Err(ModelError::IdSpaceExhausted);
        }

        let id = NodeId::from_parts(self.namespace, self.next_counter)?;
        self.next_counter += 1;
        Ok(id)
    }
}

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
#[serde(rename_all = "camelCase")]
pub struct TextRun {
    text: String,
    marks: BTreeSet<Mark>,
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
#[serde(rename_all = "camelCase")]
pub struct Paragraph {
    id: NodeId,
    inlines: Vec<InlineNode>,
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
        let paragraph_len = self.grapheme_len();
        if grapheme_offset > paragraph_len {
            return Err(ModelError::InvalidGraphemeOffset {
                offset: grapheme_offset,
                length: paragraph_len,
            });
        }
        if text.is_empty() {
            return Err(ModelError::EmptyTextRun);
        }

        if self.inlines.is_empty() {
            self.inlines.push(InlineNode::Text(TextRun { text, marks }));
            return Ok(());
        }

        let mut traversed = 0;
        for index in 0..self.inlines.len() {
            let InlineNode::Text(run) = &self.inlines[index];
            let run_graphemes = run.text.graphemes(true).count();
            if grapheme_offset > traversed + run_graphemes {
                traversed += run_graphemes;
                continue;
            }

            let local_offset = grapheme_offset - traversed;
            let split_byte = grapheme_boundary_byte(&run.text, local_offset);
            let before = run.text[..split_byte].to_owned();
            let after = run.text[split_byte..].to_owned();
            let existing_marks = run.marks.clone();

            let mut replacement = Vec::with_capacity(3);
            if !before.is_empty() {
                replacement.push(InlineNode::Text(TextRun {
                    text: before,
                    marks: existing_marks.clone(),
                }));
            }
            replacement.push(InlineNode::Text(TextRun { text, marks }));
            if !after.is_empty() {
                replacement.push(InlineNode::Text(TextRun {
                    text: after,
                    marks: existing_marks,
                }));
            }

            self.inlines.splice(index..=index, replacement);
            self.normalize_runs();
            return Ok(());
        }

        Err(ModelError::InvalidGraphemeOffset {
            offset: grapheme_offset,
            length: paragraph_len,
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

/// Bounded opaque extension data reserved by schema v0.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionValue {
    media_type: String,
    data: Vec<u8>,
}

/// A normalized schema v0 document.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    schema_version: u32,
    document_id: NodeId,
    body: Vec<BlockNode>,
    extensions: BTreeMap<String, ExtensionValue>,
}

impl Document {
    /// Creates a valid blank document containing one empty paragraph.
    pub fn blank(document_id: NodeId, paragraph_id: NodeId) -> Result<Self, ModelError> {
        let document = Self {
            schema_version: SCHEMA_VERSION,
            document_id,
            body: vec![BlockNode::Paragraph(Paragraph::empty(paragraph_id))],
            extensions: BTreeMap::new(),
        };
        document.validate()?;
        Ok(document)
    }

    /// Returns the schema version.
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns the document ID.
    #[must_use]
    pub const fn id(&self) -> NodeId {
        self.document_id
    }

    /// Returns document body nodes.
    #[must_use]
    pub fn body(&self) -> &[BlockNode] {
        &self.body
    }

    /// Returns an immutable paragraph by ID.
    #[must_use]
    pub fn paragraph(&self, id: NodeId) -> Option<&Paragraph> {
        self.body.iter().find_map(|block| match block {
            BlockNode::Paragraph(paragraph) if paragraph.id == id => Some(paragraph),
            BlockNode::Paragraph(_) => None,
        })
    }

    /// Returns a mutable paragraph by ID for transaction application.
    #[must_use]
    pub fn paragraph_mut(&mut self, id: NodeId) -> Option<&mut Paragraph> {
        self.body.iter_mut().find_map(|block| match block {
            BlockNode::Paragraph(paragraph) if paragraph.id == id => Some(paragraph),
            BlockNode::Paragraph(_) => None,
        })
    }

    /// Validates all schema v0 invariants.
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(ModelError::UnsupportedSchemaVersion(self.schema_version));
        }
        if self.body.is_empty() {
            return Err(ModelError::EmptyDocumentBody);
        }

        let mut ids = BTreeSet::new();
        if !ids.insert(self.document_id) {
            return Err(ModelError::DuplicateNodeId(self.document_id));
        }

        for block in &self.body {
            match block {
                BlockNode::Paragraph(paragraph) => {
                    if !ids.insert(paragraph.id) {
                        return Err(ModelError::DuplicateNodeId(paragraph.id));
                    }
                    let mut previous_marks: Option<&BTreeSet<Mark>> = None;
                    for inline in &paragraph.inlines {
                        match inline {
                            InlineNode::Text(run) => {
                                if run.text.is_empty() {
                                    return Err(ModelError::EmptyTextRun);
                                }
                                if previous_marks == Some(&run.marks) {
                                    return Err(ModelError::AdjacentEquivalentTextRuns(
                                        paragraph.id,
                                    ));
                                }
                                previous_marks = Some(&run.marks);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// Normalized model construction or validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelError {
    /// A node ID was zero.
    ZeroNodeId,
    /// A serialized node ID did not use the required representation.
    InvalidNodeId,
    /// An ID namespace exhausted its counter.
    IdSpaceExhausted,
    /// A deserialized schema version is not supported.
    UnsupportedSchemaVersion(u32),
    /// A document contained no body blocks.
    EmptyDocumentBody,
    /// A node ID appeared more than once.
    DuplicateNodeId(NodeId),
    /// A text run was empty.
    EmptyTextRun,
    /// Equivalent neighboring text runs were not normalized.
    AdjacentEquivalentTextRuns(NodeId),
    /// A grapheme offset was outside a paragraph.
    InvalidGraphemeOffset {
        /// Requested boundary.
        offset: usize,
        /// Paragraph grapheme length.
        length: usize,
    },
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroNodeId => formatter.write_str("node ID must be non-zero"),
            Self::InvalidNodeId => formatter.write_str("node ID must be 32 lowercase hex digits"),
            Self::IdSpaceExhausted => formatter.write_str("node ID counter is exhausted"),
            Self::UnsupportedSchemaVersion(version) => {
                write!(formatter, "unsupported schema version {version}")
            }
            Self::EmptyDocumentBody => formatter.write_str("document body must not be empty"),
            Self::DuplicateNodeId(id) => write!(formatter, "duplicate node ID {id}"),
            Self::EmptyTextRun => formatter.write_str("text runs must not be empty"),
            Self::AdjacentEquivalentTextRuns(id) => {
                write!(formatter, "paragraph {id} has non-normalized text runs")
            }
            Self::InvalidGraphemeOffset { offset, length } => {
                write!(
                    formatter,
                    "grapheme offset {offset} exceeds length {length}"
                )
            }
        }
    }
}

impl Error for ModelError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_uses_fixed_lowercase_hex() {
        let id = NodeId::from_parts(1, 2).unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"00000000000000010000000000000002\"");
        assert_eq!(serde_json::from_str::<NodeId>(&json).unwrap(), id);
        assert!(
            "0000000000000001000000000000000A"
                .parse::<NodeId>()
                .is_err()
        );
    }

    #[test]
    fn blank_document_is_valid_and_deterministic() {
        let document = Document::blank(
            NodeId::from_parts(7, 1).unwrap(),
            NodeId::from_parts(7, 2).unwrap(),
        )
        .unwrap();

        assert_eq!(document.schema_version(), 0);
        assert_eq!(document.body().len(), 1);
        document.validate().unwrap();
        assert_eq!(
            serde_json::to_string(&document).unwrap(),
            "{\"schemaVersion\":0,\"documentId\":\"00000000000000070000000000000001\",\
             \"body\":[{\"type\":\"paragraph\",\"id\":\"00000000000000070000000000000002\",\
             \"inlines\":[]}],\"extensions\":{}}"
                .replace(' ', "")
        );
    }

    #[test]
    fn insertion_respects_grapheme_boundaries() {
        let id = NodeId::from_parts(1, 1).unwrap();
        let mut paragraph = Paragraph::empty(id);
        paragraph
            .insert_text(0, "A👨‍👩‍👧‍👦B".to_owned(), BTreeSet::new())
            .unwrap();
        paragraph
            .insert_text(2, "X".to_owned(), BTreeSet::new())
            .unwrap();

        assert_eq!(paragraph.grapheme_len(), 4);
        assert_eq!(paragraph.plain_text(), "A👨‍👩‍👧‍👦XB");
        assert_eq!(paragraph.inlines().len(), 1);
    }
}
