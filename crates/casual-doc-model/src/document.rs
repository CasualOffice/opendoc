//! The normalized schema v0 document and its invariants.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::body::{BlockNode, InlineNode, Mark, Paragraph};
use crate::extension::ExtensionMap;
use crate::{ModelError, NodeId, SCHEMA_VERSION, SnapshotError, SnapshotLimits, enforce_limit};

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

    /// Returns whether the reserved v0 extension map is empty.
    #[must_use]
    pub fn extensions_is_empty(&self) -> bool {
        self.extensions.0.is_empty()
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
