//! Immutable public document snapshot value objects.

use std::collections::BTreeSet;

use casual_doc_model as model;
use casual_doc_transaction as transaction;
use serde::{Deserialize, Serialize};

use crate::value::{Mark, NodeId, Revision};

/// Immutable document snapshot returned to hosts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSnapshot {
    /// Normalized schema version.
    pub schema_version: u32,
    /// Logical document ID.
    pub document_id: NodeId,
    /// Session revision represented by the snapshot.
    pub revision: Revision,
    /// Ordered body blocks.
    pub body: Vec<BlockSnapshot>,
}

/// Body block in a public snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockSnapshot {
    /// Paragraph block.
    Paragraph(ParagraphSnapshot),
}

/// Paragraph value in a public snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphSnapshot {
    /// Stable paragraph ID.
    pub id: NodeId,
    /// Ordered inline values.
    pub inlines: Vec<InlineSnapshot>,
}

/// Inline value in a public snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InlineSnapshot {
    /// Text with one mark set.
    Text {
        /// Text content.
        text: String,
        /// Deterministically ordered marks.
        marks: BTreeSet<Mark>,
    },
}

pub(crate) fn snapshot_from_internal(
    document: &model::Document,
    revision: transaction::RevisionId,
) -> DocumentSnapshot {
    DocumentSnapshot {
        schema_version: document.schema_version(),
        document_id: NodeId::from_internal(document.id()),
        revision: Revision(revision.get()),
        body: document
            .body()
            .iter()
            .map(|block| match block {
                model::BlockNode::Paragraph(paragraph) => {
                    BlockSnapshot::Paragraph(ParagraphSnapshot {
                        id: NodeId::from_internal(paragraph.id()),
                        inlines: paragraph
                            .inlines()
                            .iter()
                            .map(|inline| match inline {
                                model::InlineNode::Text(run) => InlineSnapshot::Text {
                                    text: run.text().to_owned(),
                                    marks: run
                                        .marks()
                                        .iter()
                                        .copied()
                                        .map(Mark::from_internal)
                                        .collect(),
                                },
                            })
                            .collect(),
                    })
                }
            })
            .collect(),
    }
}
