//! Deterministic total v0-to-v1 migration.

use std::collections::BTreeSet;
use std::fmt;

use super::*;
use crate::{
    BlockNode as V0BlockNode, Document as V0Document, IdGenerator, InlineNode as V0InlineNode,
    Mark, ModelError, NodeId, SnapshotError,
};

/// A deterministic v0-to-v1 migration failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MigrationError {
    /// The v0 source failed its own invariants.
    InvalidSource(ModelError),
    /// Migration produced an invalid v1 document (an internal defect, e.g. a
    /// source run whose grapheme count exceeds `u32::MAX`).
    ProducedInvalidV1(ModelError),
    /// The synthesized id space was exhausted.
    IdSpaceExhausted,
    /// The v0 source populated the extension map, which v1 cannot represent.
    UnsupportedSourceExtensions,
}

impl fmt::Display for MigrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSource(error) => write!(formatter, "v0 source is invalid: {error}"),
            Self::ProducedInvalidV1(error) => {
                write!(
                    formatter,
                    "migration produced an invalid v1 document: {error}"
                )
            }
            Self::IdSpaceExhausted => formatter.write_str("synthesized node id space is exhausted"),
            Self::UnsupportedSourceExtensions => {
                formatter.write_str("v0 source extension map cannot migrate to v1")
            }
        }
    }
}

impl std::error::Error for MigrationError {}

impl From<MigrationError> for SnapshotError {
    fn from(error: MigrationError) -> Self {
        match error {
            MigrationError::InvalidSource(model) | MigrationError::ProducedInvalidV1(model) => {
                Self::InvalidModel(model)
            }
            MigrationError::IdSpaceExhausted => Self::LimitExceeded {
                limit: "synthesized_node_ids",
                observed: usize::MAX,
                allowed: usize::MAX,
            },
            MigrationError::UnsupportedSourceExtensions => {
                Self::InvalidModel(ModelError::UnsupportedV0Extensions)
            }
        }
    }
}

impl Document {
    /// Deterministically migrates a valid v0 document into v1.
    ///
    /// Migration is total and lossless over the empty-extensions v0 profile:
    /// paragraph and document ids are preserved verbatim; synthesized run ids
    /// come from `ids` in canonical document order (skipping preserved ids).
    /// A v0 source with a populated extension map is rejected — never silently
    /// dropped. Output bytes are identical for a fixed `(source, ids seed)`.
    /// Totality is conditioned on each v0 run's grapheme count fitting `u32`;
    /// an over-long run yields `ProducedInvalidV1` rather than a wrong document.
    pub fn from_v0(source: &V0Document, ids: &mut IdGenerator) -> Result<Self, MigrationError> {
        source.validate().map_err(MigrationError::InvalidSource)?;
        if !source.extensions_is_empty() {
            return Err(MigrationError::UnsupportedSourceExtensions);
        }

        let mut used: BTreeSet<NodeId> = BTreeSet::new();
        used.insert(source.id());
        for block in source.body() {
            let V0BlockNode::Paragraph(paragraph) = block;
            used.insert(paragraph.id());
        }

        let mut body = Vec::with_capacity(source.body().len());
        for block in source.body() {
            let V0BlockNode::Paragraph(paragraph) = block;
            let mut inlines = Vec::with_capacity(paragraph.inlines().len());
            for inline in paragraph.inlines() {
                let V0InlineNode::Text(run) = inline;
                let id = alloc_non_colliding(ids, &mut used)?;
                inlines.push(InlineNode::Run(Run {
                    id,
                    properties: run_properties_from_marks(run.marks()),
                    text: run.text().to_owned(),
                }));
            }
            body.push(BlockNode::Paragraph(Paragraph {
                id: paragraph.id(),
                properties: ParagraphProperties::default(),
                inlines,
            }));
        }

        Document::new(source.id(), body, Definitions::default())
            .map_err(MigrationError::ProducedInvalidV1)
    }
}

fn alloc_non_colliding(
    ids: &mut IdGenerator,
    used: &mut BTreeSet<NodeId>,
) -> Result<NodeId, MigrationError> {
    loop {
        let candidate = ids
            .next_id()
            .map_err(|_| MigrationError::IdSpaceExhausted)?;
        if used.insert(candidate) {
            return Ok(candidate);
        }
    }
}

fn run_properties_from_marks(marks: &BTreeSet<Mark>) -> RunProperties {
    let flag = |mark: Mark| marks.contains(&mark).then_some(true);
    RunProperties {
        style_ref: None,
        bold: flag(Mark::Bold),
        italic: flag(Mark::Italic),
        underline: flag(Mark::Underline),
        strike: flag(Mark::Strike),
        color: None,
        size_half_points: None,
        font_ref: None,
    }
}
