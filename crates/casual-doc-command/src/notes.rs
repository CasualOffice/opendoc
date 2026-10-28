// SPDX-License-Identifier: Apache-2.0

//! Footnote and endnote lowerings the editor and agents share (`docs/181`
//! §4.2, step 6).

use casual_doc_edit::table_structure::MintId;
use casual_doc_edit::{Operation, Pos};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, InlineNode, NoteId, NoteKind, Paragraph, ParagraphProperties, Run, RunProperties,
};

fn next(ids: MintId<'_>) -> Result<NodeId, String> {
    ids().ok_or_else(|| "id space exhausted".to_owned())
}

/// A lowered note insertion.
#[derive(Clone, Debug)]
pub struct NoteEdit {
    /// The operations, one step.
    pub operations: Vec<Operation>,
    /// The note body's first paragraph.
    pub body: NodeId,
}

/// Inserts a footnote or endnote whose reference mark sits at `at`, with a
/// body of one paragraph: empty (the editor's gesture, the caret goes there to
/// type) or holding `text` (an agent writes the note in the same edit). The
/// note id, the reference id and the body paragraph id are minted in that
/// order, then the text run's. The displayed number is derived at render.
/// O(1).
///
/// # Errors
/// When ids run out.
pub fn insert_note(
    kind: NoteKind,
    at: Pos,
    text: Option<&str>,
    ids: MintId<'_>,
) -> Result<NoteEdit, String> {
    let note = NoteId::new(next(ids)?);
    let reference_id = next(ids)?;
    let body = next(ids)?;
    let inlines = match text.filter(|text| !text.is_empty()) {
        Some(text) => vec![InlineNode::Run(Run {
            id: next(ids)?,
            properties: RunProperties::default().into(),
            text: text.to_owned(),
        })],
        None => Vec::new(),
    };
    Ok(NoteEdit {
        operations: vec![Operation::InsertNote {
            kind,
            note,
            at,
            reference_id,
            blocks: vec![BlockNode::Paragraph(Paragraph {
                id: body,
                properties: ParagraphProperties::default().into(),
                inlines,
            })],
        }],
        body,
    })
}
