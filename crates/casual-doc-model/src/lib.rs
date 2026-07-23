//! Normalized document values and invariants used inside the OpenDoc runtime.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::str::FromStr;

use serde::de::{self, MapAccess, Visitor};
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRun {
    text: String,
    #[serde(deserialize_with = "deserialize_marks")]
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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

/// Bounded opaque extension data reserved by schema v0.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionValue {
    media_type: String,
    data: Vec<u8>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ExtensionMap(BTreeMap<String, ExtensionValue>);

impl Serialize for ExtensionMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ExtensionMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ExtensionMapVisitor;

        impl<'de> Visitor<'de> for ExtensionMapVisitor {
            type Value = ExtensionMap;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an object with unique extension keys")
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = access.next_entry()? {
                    if values.insert(key, value).is_some() {
                        return Err(de::Error::custom("duplicate extension key"));
                    }
                }
                Ok(ExtensionMap(values))
            }
        }

        deserializer.deserialize_map(ExtensionMapVisitor)
    }
}

/// A normalized schema v0 document.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Document {
    schema_version: u32,
    document_id: NodeId,
    body: Vec<BlockNode>,
    extensions: ExtensionMap,
}

impl Document {
    /// Creates a valid blank document containing one empty paragraph.
    pub fn blank(document_id: NodeId, paragraph_id: NodeId) -> Result<Self, ModelError> {
        let document = Self {
            schema_version: SCHEMA_VERSION,
            document_id,
            body: vec![BlockNode::Paragraph(Paragraph::empty(paragraph_id))],
            extensions: ExtensionMap::default(),
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

    /// Returns whether the document already contains a node ID.
    #[must_use]
    pub fn has_node_id(&self, id: NodeId) -> bool {
        self.contains_id(id)
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

    /// Splits a paragraph and inserts the new paragraph immediately after it.
    pub fn split_paragraph(
        &mut self,
        paragraph_id: NodeId,
        grapheme_offset: usize,
        new_id: NodeId,
    ) -> Result<(), ModelError> {
        if self.contains_id(new_id) {
            return Err(ModelError::DuplicateNodeId(new_id));
        }
        let index = self
            .paragraph_index(paragraph_id)
            .ok_or(ModelError::UnknownParagraph(paragraph_id))?;
        let trailing = match &mut self.body[index] {
            BlockNode::Paragraph(paragraph) => paragraph.split_off(grapheme_offset, new_id)?,
        };
        self.body.insert(index + 1, BlockNode::Paragraph(trailing));
        Ok(())
    }

    /// Joins two adjacent paragraphs and returns the join grapheme boundary.
    pub fn join_paragraphs(&mut self, first: NodeId, second: NodeId) -> Result<u32, ModelError> {
        let first_index = self
            .paragraph_index(first)
            .ok_or(ModelError::UnknownParagraph(first))?;
        let second_index = self
            .paragraph_index(second)
            .ok_or(ModelError::UnknownParagraph(second))?;
        if second_index != first_index + 1 {
            return Err(ModelError::ParagraphsNotAdjacent { first, second });
        }

        let boundary = u32::try_from(
            self.paragraph(first)
                .ok_or(ModelError::UnknownParagraph(first))?
                .grapheme_len(),
        )
        .map_err(|_| ModelError::GraphemeCountOverflow(first))?;
        let BlockNode::Paragraph(trailing) = self.body.remove(second_index);
        match &mut self.body[first_index] {
            BlockNode::Paragraph(paragraph) => paragraph.append_paragraph(trailing),
        }
        Ok(boundary)
    }

    fn contains_id(&self, id: NodeId) -> bool {
        self.document_id == id || self.paragraph(id).is_some()
    }

    fn paragraph_index(&self, id: NodeId) -> Option<usize> {
        self.body.iter().position(|block| match block {
            BlockNode::Paragraph(paragraph) => paragraph.id == id,
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

    /// Parses one strict, bounded normalized schema v0 JSON document.
    pub fn from_json(bytes: &[u8], limits: SnapshotLimits) -> Result<Self, SnapshotError> {
        limits.validate()?;
        enforce_limit("input_json_bytes", bytes.len(), limits.max_input_bytes)?;
        let document: Self =
            serde_json::from_slice(bytes).map_err(|_| SnapshotError::MalformedJson)?;
        document.validate().map_err(SnapshotError::InvalidModel)?;
        document.validate_snapshot_limits(limits)?;
        Ok(document)
    }

    /// Serializes a valid normalized document to deterministic compact JSON.
    pub fn to_json(&self) -> Result<Vec<u8>, SnapshotError> {
        self.validate().map_err(SnapshotError::InvalidModel)?;
        serde_json::to_vec(self).map_err(|_| SnapshotError::Serialization)
    }

    fn validate_snapshot_limits(&self, limits: SnapshotLimits) -> Result<(), SnapshotError> {
        enforce_limit("body_blocks", self.body.len(), limits.max_blocks)?;

        let mut scalar_values = 0_usize;
        for block in &self.body {
            match block {
                BlockNode::Paragraph(paragraph) => {
                    for inline in &paragraph.inlines {
                        match inline {
                            InlineNode::Text(run) => {
                                enforce_limit(
                                    "text_run_bytes",
                                    run.text.len(),
                                    limits.max_text_run_bytes,
                                )?;
                                scalar_values = scalar_values
                                    .checked_add(run.text.chars().count())
                                    .ok_or(SnapshotError::LimitExceeded {
                                        limit: "unicode_scalar_values",
                                        observed: usize::MAX,
                                        allowed: limits.max_unicode_scalar_values,
                                    })?;
                            }
                        }
                    }
                }
            }
        }
        enforce_limit(
            "unicode_scalar_values",
            scalar_values,
            limits.max_unicode_scalar_values,
        )?;
        enforce_limit(
            "extension_entries",
            self.extensions.0.len(),
            limits.max_extension_entries,
        )?;
        let extension_bytes =
            self.extensions
                .0
                .values()
                .try_fold(0_usize, |total, extension| {
                    total
                        .checked_add(extension.media_type.len())
                        .and_then(|value| value.checked_add(extension.data.len()))
                        .ok_or(SnapshotError::LimitExceeded {
                            limit: "extension_payload_bytes",
                            observed: usize::MAX,
                            allowed: limits.max_extension_bytes,
                        })
                })?;
        enforce_limit(
            "extension_payload_bytes",
            extension_bytes,
            limits.max_extension_bytes,
        )
    }
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

fn enforce_limit(
    limit: &'static str,
    observed: usize,
    allowed: usize,
) -> Result<(), SnapshotError> {
    if observed > allowed {
        return Err(SnapshotError::LimitExceeded {
            limit,
            observed,
            allowed,
        });
    }
    Ok(())
}

/// Configurable normalized snapshot limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotLimits {
    /// Maximum input JSON bytes checked before parsing.
    pub max_input_bytes: usize,
    /// Maximum body blocks.
    pub max_blocks: usize,
    /// Maximum Unicode scalar values across text runs.
    pub max_unicode_scalar_values: usize,
    /// Maximum UTF-8 bytes in one text run.
    pub max_text_run_bytes: usize,
    /// Maximum extension map entries.
    pub max_extension_entries: usize,
    /// Maximum aggregate extension media-type and payload bytes.
    pub max_extension_bytes: usize,
}

impl SnapshotLimits {
    const HARD_MAX_INPUT_BYTES: usize = 256 * 1024 * 1024;
    const HARD_MAX_BLOCKS: usize = 8_000_000;
    const HARD_MAX_UNICODE_SCALAR_VALUES: usize = 200_000_000;
    const HARD_MAX_TEXT_RUN_BYTES: usize = 64 * 1024 * 1024;
    const HARD_MAX_EXTENSION_ENTRIES: usize = 500_000;
    const HARD_MAX_EXTENSION_BYTES: usize = 256 * 1024 * 1024;

    fn validate(self) -> Result<(), SnapshotError> {
        for (name, value, hard_ceiling) in [
            (
                "input_json_bytes",
                self.max_input_bytes,
                Self::HARD_MAX_INPUT_BYTES,
            ),
            ("body_blocks", self.max_blocks, Self::HARD_MAX_BLOCKS),
            (
                "unicode_scalar_values",
                self.max_unicode_scalar_values,
                Self::HARD_MAX_UNICODE_SCALAR_VALUES,
            ),
            (
                "text_run_bytes",
                self.max_text_run_bytes,
                Self::HARD_MAX_TEXT_RUN_BYTES,
            ),
            (
                "extension_entries",
                self.max_extension_entries,
                Self::HARD_MAX_EXTENSION_ENTRIES,
            ),
            (
                "extension_payload_bytes",
                self.max_extension_bytes,
                Self::HARD_MAX_EXTENSION_BYTES,
            ),
        ] {
            if value > hard_ceiling {
                return Err(SnapshotError::InvalidLimitConfiguration {
                    limit: name,
                    value,
                    hard_ceiling,
                });
            }
        }
        Ok(())
    }
}

impl Default for SnapshotLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024 * 1024,
            max_blocks: 2_000_000,
            max_unicode_scalar_values: 50_000_000,
            max_text_run_bytes: 16 * 1024 * 1024,
            max_extension_entries: 100_000,
            max_extension_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Normalized snapshot parsing, limit, or serialization failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    /// A configured limit exceeded the runtime hard ceiling.
    InvalidLimitConfiguration {
        /// Stable limit name.
        limit: &'static str,
        /// Requested value.
        value: usize,
        /// Maximum permitted value.
        hard_ceiling: usize,
    },
    /// Input exceeded a configured limit.
    LimitExceeded {
        /// Stable limit name.
        limit: &'static str,
        /// Observed value.
        observed: usize,
        /// Configured allowed value.
        allowed: usize,
    },
    /// JSON syntax or strict schema decoding failed.
    MalformedJson,
    /// Parsed content violated normalized model invariants.
    InvalidModel(ModelError),
    /// A valid committed document could not be serialized.
    Serialization,
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimitConfiguration {
                limit,
                value,
                hard_ceiling,
            } => write!(
                formatter,
                "limit {limit} value {value} exceeds hard ceiling {hard_ceiling}"
            ),
            Self::LimitExceeded {
                limit,
                observed,
                allowed,
            } => write!(
                formatter,
                "limit {limit} observed {observed} exceeds allowed {allowed}"
            ),
            Self::MalformedJson => formatter.write_str("normalized JSON is malformed"),
            Self::InvalidModel(error) => write!(formatter, "normalized model is invalid: {error}"),
            Self::Serialization => formatter.write_str("normalized JSON serialization failed"),
        }
    }
}

impl Error for SnapshotError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidModel(error) => Some(error),
            _ => None,
        }
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
    /// A grapheme range was empty or reversed.
    InvalidGraphemeRange {
        /// Start boundary.
        start: usize,
        /// End boundary.
        end: usize,
    },
    /// A paragraph ID did not resolve.
    UnknownParagraph(NodeId),
    /// Two paragraphs were not adjacent in the required order.
    ParagraphsNotAdjacent {
        /// First requested paragraph.
        first: NodeId,
        /// Second requested paragraph.
        second: NodeId,
    },
    /// A paragraph length exceeded public position representation.
    GraphemeCountOverflow(NodeId),
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
            Self::InvalidGraphemeRange { start, end } => {
                write!(formatter, "grapheme range {start}..{end} is invalid")
            }
            Self::UnknownParagraph(id) => write!(formatter, "paragraph {id} does not exist"),
            Self::ParagraphsNotAdjacent { first, second } => {
                write!(
                    formatter,
                    "paragraphs {first} and {second} are not adjacent"
                )
            }
            Self::GraphemeCountOverflow(id) => {
                write!(
                    formatter,
                    "paragraph {id} exceeds addressable grapheme count"
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

    #[test]
    fn normalized_json_round_trip_is_byte_deterministic() {
        let document = Document::blank(
            NodeId::from_parts(3, 1).unwrap(),
            NodeId::from_parts(3, 2).unwrap(),
        )
        .unwrap();
        let first = document.to_json().unwrap();
        let loaded = Document::from_json(&first, SnapshotLimits::default()).unwrap();
        let second = loaded.to_json().unwrap();

        assert_eq!(first, second);
        assert_eq!(loaded, document);
    }

    #[test]
    fn strict_json_rejects_unknown_and_duplicate_values() {
        let unknown = br#"{
            "schemaVersion":0,
            "documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","inlines":[]}],
            "extensions":{},
            "future":true
        }"#;
        let duplicate_mark = br#"{
            "schemaVersion":0,
            "documentId":"00000000000000030000000000000001",
            "body":[{
                "type":"paragraph",
                "id":"00000000000000030000000000000002",
                "inlines":[{"type":"text","text":"x","marks":["bold","bold"]}]
            }],
            "extensions":{}
        }"#;
        let duplicate_extension = br#"{
            "schemaVersion":0,
            "documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","inlines":[]}],
            "extensions":{
                "same":{"mediaType":"application/octet-stream","data":[1]},
                "same":{"mediaType":"application/octet-stream","data":[2]}
            }
        }"#;

        for invalid in [unknown.as_slice(), duplicate_mark, duplicate_extension] {
            assert_eq!(
                Document::from_json(invalid, SnapshotLimits::default()),
                Err(SnapshotError::MalformedJson)
            );
        }
    }

    #[test]
    fn snapshot_limits_reject_before_and_after_parse() {
        let json = br#"{
            "schemaVersion":0,
            "documentId":"00000000000000030000000000000001",
            "body":[{
                "type":"paragraph",
                "id":"00000000000000030000000000000002",
                "inlines":[{"type":"text","text":"secret","marks":[]}]
            }],
            "extensions":{}
        }"#;
        let byte_limits = SnapshotLimits {
            max_input_bytes: json.len() - 1,
            ..SnapshotLimits::default()
        };
        assert!(matches!(
            Document::from_json(json, byte_limits),
            Err(SnapshotError::LimitExceeded {
                limit: "input_json_bytes",
                ..
            })
        ));

        let text_limits = SnapshotLimits {
            max_unicode_scalar_values: 5,
            ..SnapshotLimits::default()
        };
        let error = Document::from_json(json, text_limits).unwrap_err();
        assert!(matches!(
            error,
            SnapshotError::LimitExceeded {
                limit: "unicode_scalar_values",
                ..
            }
        ));
        assert!(!error.to_string().contains("secret"));
    }

    #[test]
    fn every_snapshot_limit_has_a_stable_boundary_name() {
        let text_json = br#"{
            "schemaVersion":0,
            "documentId":"00000000000000030000000000000001",
            "body":[{
                "type":"paragraph",
                "id":"00000000000000030000000000000002",
                "inlines":[{"type":"text","text":"abcdef","marks":[]}]
            }],
            "extensions":{}
        }"#;
        let cases = [
            (
                SnapshotLimits {
                    max_blocks: 0,
                    ..SnapshotLimits::default()
                },
                "body_blocks",
            ),
            (
                SnapshotLimits {
                    max_unicode_scalar_values: 5,
                    ..SnapshotLimits::default()
                },
                "unicode_scalar_values",
            ),
            (
                SnapshotLimits {
                    max_text_run_bytes: 5,
                    ..SnapshotLimits::default()
                },
                "text_run_bytes",
            ),
        ];
        for (limits, expected_name) in cases {
            assert!(matches!(
                Document::from_json(text_json, limits),
                Err(SnapshotError::LimitExceeded { limit, .. }) if limit == expected_name
            ));
        }

        let extension_json = br#"{
            "schemaVersion":0,
            "documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","inlines":[]}],
            "extensions":{"x":{"mediaType":"x","data":[1,2]}}
        }"#;
        assert!(matches!(
            Document::from_json(
                extension_json,
                SnapshotLimits {
                    max_extension_entries: 0,
                    ..SnapshotLimits::default()
                }
            ),
            Err(SnapshotError::LimitExceeded {
                limit: "extension_entries",
                ..
            })
        ));
        assert!(matches!(
            Document::from_json(
                extension_json,
                SnapshotLimits {
                    max_extension_bytes: 2,
                    ..SnapshotLimits::default()
                }
            ),
            Err(SnapshotError::LimitExceeded {
                limit: "extension_payload_bytes",
                ..
            })
        ));

        assert!(
            Document::from_json(
                text_json,
                SnapshotLimits {
                    max_input_bytes: text_json.len(),
                    max_blocks: 1,
                    max_unicode_scalar_values: 6,
                    max_text_run_bytes: 6,
                    ..SnapshotLimits::default()
                }
            )
            .is_ok()
        );
    }

    #[test]
    fn hard_ceiling_is_not_host_bypassable() {
        let limits = SnapshotLimits {
            max_input_bytes: SnapshotLimits::HARD_MAX_INPUT_BYTES + 1,
            ..SnapshotLimits::default()
        };
        assert!(matches!(
            Document::from_json(b"{}", limits),
            Err(SnapshotError::InvalidLimitConfiguration {
                limit: "input_json_bytes",
                ..
            })
        ));
    }

    #[test]
    fn duplicate_node_ids_fail_invariant_validation() {
        let json = br#"{
            "schemaVersion":0,
            "documentId":"00000000000000030000000000000001",
            "body":[{
                "type":"paragraph",
                "id":"00000000000000030000000000000001",
                "inlines":[]
            }],
            "extensions":{}
        }"#;
        assert!(matches!(
            Document::from_json(json, SnapshotLimits::default()),
            Err(SnapshotError::InvalidModel(ModelError::DuplicateNodeId(_)))
        ));
    }
}
