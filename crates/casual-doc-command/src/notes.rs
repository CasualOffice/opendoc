// SPDX-License-Identifier: Apache-2.0

//! Footnote and endnote lowerings the editor and agents share (`docs/181`
//! §4.2, step 6).

use casual_doc_edit::table_structure::MintId;
use casual_doc_edit::{Operation, Pos};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, InlineNode, NoteId, NoteKind, Paragraph, ParagraphProperties, Run, RunProperties,
};
// Their own lines (parallel lanes, rustfmt Preserve): note options and conversion.
use casual_doc_edit::inline_text_len;
use casual_doc_model::v1::{Document, NoteProperties, NoteReference, SectionId, StyleId};

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

/// Which sections a note-options change reaches — Word's "Apply changes to".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoteScope {
    /// The one section given.
    Section(SectionId),
    /// Every section, and the document's own defaults in settings.
    Document,
}

/// Word's Footnote and Endnote dialog, Apply: `properties` for `kind`, written to
/// one section or to every section and the settings default, as one step. An
/// imported document's per-section options are overwritten the way Word
/// overwrites them. O(sections).
#[must_use]
pub fn set_note_properties(
    document: &Document,
    scope: NoteScope,
    kind: NoteKind,
    properties: &NoteProperties,
) -> Vec<Operation> {
    let op = |section| Operation::SetNoteProperties {
        section,
        kind,
        properties: Box::new(properties.clone()),
    };
    match scope {
        NoteScope::Section(section) => vec![op(Some(section))],
        NoteScope::Document => std::iter::once(op(None))
            .chain(
                document
                    .definitions()
                    .sections
                    .iter()
                    .map(|section| op(Some(section.id))),
            )
            .collect(),
    }
}

/// Which notes a conversion turns into the other kind — Word's and ONLYOFFICE's
/// three Convert Notes choices (`DE.Views.Links.mniConvertNote`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoteConversion {
    /// Every footnote becomes an endnote.
    FootnotesToEndnotes,
    /// Every endnote becomes a footnote.
    EndnotesToFootnotes,
    /// Both at once.
    Swap,
}

impl NoteConversion {
    /// What a reference of `kind` becomes, or `None` when this conversion leaves it.
    #[must_use]
    pub const fn target(self, kind: NoteKind) -> Option<NoteKind> {
        match (self, kind) {
            (Self::FootnotesToEndnotes | Self::Swap, NoteKind::Footnote) => Some(NoteKind::Endnote),
            (Self::EndnotesToFootnotes | Self::Swap, NoteKind::Endnote) => Some(NoteKind::Footnote),
            _ => None,
        }
    }
}

/// A selection to convert within, as `(paragraph, offset)` endpoints; `None`
/// converts the whole document.
pub type NoteRange = Option<((NodeId, u32), (NodeId, u32))>;

/// A lowered conversion.
#[derive(Clone, Debug, Default)]
pub struct NoteConversionEdit {
    /// One `RemoveNote` and one `InsertNote` per converted note, in document
    /// order, as one step.
    pub operations: Vec<Operation>,
    /// How many notes were converted.
    pub converted: usize,
    /// How many matching references were LEFT because they sit inside a wrapper
    /// (a hyperlink, a tracked change) rather than at a paragraph's top level,
    /// where `RemoveNote` finds them. Reported so the host can say so.
    pub nested: usize,
}

/// Every top-level note reference in `blocks`, in document order, as
/// `(paragraph order, paragraph, offset, reference)`; nested ones are counted
/// by kind in `nested`. `order` numbers paragraphs as they are met. O(blocks).
fn references_in(
    blocks: &[BlockNode],
    order: &mut usize,
    out: &mut Vec<(usize, NodeId, u32, NoteReference)>,
    nested: &mut Vec<NoteKind>,
) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let mut offset = 0u32;
                for inline in &paragraph.inlines {
                    match inline {
                        InlineNode::NoteReference(reference) => {
                            out.push((*order, paragraph.id, offset, *reference));
                        }
                        other => nested_references(other, nested),
                    }
                    offset += inline_text_len(inline);
                }
                *order += 1;
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        references_in(&cell.blocks, order, out, nested);
                    }
                }
            }
            BlockNode::Sdt(sdt) => references_in(&sdt.blocks, order, out, nested),
            BlockNode::AltChunk(_) => {}
        }
    }
}

/// Note references inside a wrapper inline, which a conversion cannot reach.
fn nested_references(inline: &InlineNode, nested: &mut Vec<NoteKind>) {
    let children: &[InlineNode] = match inline {
        InlineNode::Hyperlink(link) => &link.inlines,
        InlineNode::Revision(revision) => &revision.inlines,
        _ => return,
    };
    for child in children {
        match child {
            InlineNode::NoteReference(reference) => nested.push(reference.kind),
            other => nested_references(other, nested),
        }
    }
}

/// The position of paragraph `node` in document order. O(blocks).
fn paragraph_index(blocks: &[BlockNode], node: NodeId, order: &mut usize) -> Option<usize> {
    for block in blocks {
        let found = match block {
            BlockNode::Paragraph(paragraph) => {
                let here = *order;
                *order += 1;
                (paragraph.id == node).then_some(here)
            }
            BlockNode::Table(table) => table.rows.iter().find_map(|row| {
                row.cells
                    .iter()
                    .find_map(|cell| paragraph_index(&cell.blocks, node, order))
            }),
            BlockNode::Sdt(sdt) => paragraph_index(&sdt.blocks, node, order),
            BlockNode::AltChunk(_) => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// The paragraph styles a converted note's paragraphs move between: Word's
/// "footnote text" and "endnote text", matched by NAME because style ids are
/// minted per import. `(from, to)` for a note of `kind`, or `None` when the
/// document does not define both. O(styles).
fn note_text_styles(document: &Document, kind: NoteKind) -> Option<(StyleId, StyleId)> {
    let named = |name: &str| {
        document
            .definitions()
            .styles
            .iter()
            .find(|(_, style)| {
                style
                    .name
                    .as_deref()
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
            })
            .map(|(id, _)| *id)
    };
    let footnote = named("footnote text")?;
    let endnote = named("endnote text")?;
    Some(match kind {
        NoteKind::Footnote => (footnote, endnote),
        NoteKind::Endnote => (endnote, footnote),
    })
}

/// A note body re-kinded: its number mark says the new kind, and a paragraph in
/// the old kind's text style takes the new kind's.
fn converted_blocks(
    blocks: &[BlockNode],
    target: NoteKind,
    styles: Option<(StyleId, StyleId)>,
) -> Vec<BlockNode> {
    blocks
        .iter()
        .map(|block| match block {
            BlockNode::Paragraph(paragraph) => {
                let mut paragraph = paragraph.clone();
                if let Some((from, to)) = styles
                    && paragraph.properties.style_ref == Some(from)
                {
                    paragraph.properties.make_mut().style_ref = Some(to);
                }
                for inline in &mut paragraph.inlines {
                    if let InlineNode::NoteNumberMark(mark) = inline {
                        mark.kind = target;
                    }
                }
                BlockNode::Paragraph(paragraph)
            }
            other => other.clone(),
        })
        .collect()
}

/// Word's Convert Notes, as ONE step: every footnote to an endnote, every
/// endnote to a footnote, or both — over the whole document, or only the
/// references inside `range`. For each note its reference and definition are
/// removed and a note of the other kind is inserted at the same place holding
/// the same blocks, its number mark and text style re-kinded. Each pair is net
/// zero in paragraph length, so every later offset stays valid; the numbering is
/// derived at render, so both kinds renumber themselves.
///
/// O(blocks + the converted notes' content).
///
/// # Errors
/// When ids run out.
pub fn convert_notes(
    document: &Document,
    conversion: NoteConversion,
    range: NoteRange,
    ids: MintId<'_>,
) -> Result<NoteConversionEdit, String> {
    let mut references = Vec::new();
    let mut nested = Vec::new();
    references_in(document.body(), &mut 0, &mut references, &mut nested);
    let within = match range {
        None => None,
        Some(((start_node, start), (end_node, end))) => {
            let at = |node| paragraph_index(document.body(), node, &mut 0);
            let (Some(a), Some(b)) = (at(start_node), at(end_node)) else {
                return Ok(NoteConversionEdit::default());
            };
            let (first, last) = if (a, start) <= (b, end) {
                ((a, start), (b, end))
            } else {
                ((b, end), (a, start))
            };
            Some((first, last))
        }
    };
    let mut edit = NoteConversionEdit {
        nested: nested
            .into_iter()
            .filter(|kind| conversion.target(*kind).is_some())
            .count(),
        ..NoteConversionEdit::default()
    };
    let definitions = document.definitions();
    // Resolved once, not per note: each is a scan of the style table.
    let footnote_styles = note_text_styles(document, NoteKind::Footnote);
    let endnote_styles = note_text_styles(document, NoteKind::Endnote);
    for (order, paragraph, offset, reference) in references {
        let Some(target) = conversion.target(reference.kind) else {
            continue;
        };
        // A reference is zero-width (`inline_text_len`): it is inside the
        // selection when it sits at or after the start and before the end.
        if let Some((first, last)) = within
            && ((order, offset) < first || (order, offset) >= last)
        {
            continue;
        }
        let note = match reference.kind {
            NoteKind::Footnote => definitions.footnotes.get(&reference.note),
            NoteKind::Endnote => definitions.endnotes.get(&reference.note),
        };
        let Some(note) = note else {
            continue;
        };
        let blocks = converted_blocks(
            &note.blocks,
            target,
            match reference.kind {
                NoteKind::Footnote => footnote_styles,
                NoteKind::Endnote => endnote_styles,
            },
        );
        edit.operations.push(Operation::RemoveNote {
            kind: reference.kind,
            note: reference.note,
            reference_id: reference.id,
        });
        edit.operations.push(Operation::InsertNote {
            kind: target,
            note: NoteId::new(next(ids)?),
            at: Pos::new(paragraph, offset),
            reference_id: reference.id,
            blocks,
        });
        edit.converted += 1;
    }
    Ok(edit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u64) -> NodeId {
        NodeId::from_parts(n, 91).unwrap()
    }

    fn run(n: u64, text: &str) -> InlineNode {
        InlineNode::Run(Run {
            id: id(n),
            properties: RunProperties::default().into(),
            text: text.to_owned(),
        })
    }

    fn reference(n: u64, kind: NoteKind, note: u64) -> InlineNode {
        InlineNode::NoteReference(NoteReference {
            id: id(n),
            kind,
            note: NoteId::new(id(note)),
        })
    }

    fn body(n: u64, text: &str) -> Vec<BlockNode> {
        vec![BlockNode::Paragraph(Paragraph {
            id: id(n),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(n + 1, text)],
        })]
    }

    /// "One[f1] two[e1] three[f2]" — two footnotes and an endnote.
    fn document() -> Document {
        let mut definitions = casual_doc_model::v1::Definitions::default();
        definitions.footnotes.insert(
            NoteId::new(id(50)),
            casual_doc_model::v1::Note {
                blocks: body(900, "first"),
            },
        );
        definitions.footnotes.insert(
            NoteId::new(id(51)),
            casual_doc_model::v1::Note {
                blocks: body(910, "second"),
            },
        );
        definitions.endnotes.insert(
            NoteId::new(id(60)),
            casual_doc_model::v1::Note {
                blocks: body(920, "end"),
            },
        );
        Document::new(
            id(1),
            vec![BlockNode::Paragraph(Paragraph {
                id: id(10),
                properties: ParagraphProperties::default().into(),
                inlines: vec![
                    run(11, "One"),
                    reference(12, NoteKind::Footnote, 50),
                    run(13, " two"),
                    reference(14, NoteKind::Endnote, 60),
                    run(15, " three"),
                    reference(16, NoteKind::Footnote, 51),
                ],
            })],
            definitions,
        )
        .expect("valid")
    }

    fn mint() -> impl FnMut() -> Option<NodeId> {
        let mut next = 5_000;
        move || {
            next += 1;
            NodeId::from_parts(next, 91).ok()
        }
    }

    /// Every footnote becomes an endnote at the same offset with the same body,
    /// the endnote is left alone, and the pairs are in document order.
    #[test]
    fn footnotes_become_endnotes_where_they_stood() {
        let document = document();
        let mut ids = mint();
        let edit = convert_notes(
            &document,
            NoteConversion::FootnotesToEndnotes,
            None,
            &mut ids,
        )
        .expect("lowers");
        assert_eq!(edit.converted, 2);
        let inserts: Vec<(NoteKind, u32)> = edit
            .operations
            .iter()
            .filter_map(|op| match op {
                Operation::InsertNote { kind, at, .. } => Some((*kind, at.offset)),
                _ => None,
            })
            .collect();
        // A reference is zero-width in the offset space (`inline_text_len`), so
        // "One" puts the first at 3 and "One two three" the second at 13.
        assert_eq!(
            inserts,
            vec![(NoteKind::Endnote, 3), (NoteKind::Endnote, 13)]
        );
        assert!(edit.operations.iter().all(|op| !matches!(
            op,
            Operation::RemoveNote {
                kind: NoteKind::Endnote,
                ..
            }
        )));
    }

    /// Swap converts both kinds; a selection converts only what it covers.
    /// Mutation: drop the `within` filter -> the selected conversion converts
    /// all three.
    #[test]
    fn swap_converts_both_and_a_selection_only_its_own() {
        let document = document();
        let mut ids = mint();
        let all = convert_notes(&document, NoteConversion::Swap, None, &mut ids).expect("lowers");
        assert_eq!(all.converted, 3);
        // From inside " two" to the end of " three": the endnote at 7 and the
        // footnote at 13, not the footnote at 3.
        let selected = convert_notes(
            &document,
            NoteConversion::Swap,
            Some(((id(10), 5), (id(10), 16))),
            &mut ids,
        )
        .expect("lowers");
        assert_eq!(selected.converted, 2);
    }

    #[test]
    fn whole_document_options_reach_the_settings_default_and_every_section() {
        let document = document();
        let ops = set_note_properties(
            &document,
            NoteScope::Document,
            NoteKind::Footnote,
            &NoteProperties::default(),
        );
        assert_eq!(ops.len(), 1 + document.definitions().sections.len());
        assert!(matches!(
            ops[0],
            Operation::SetNoteProperties { section: None, .. }
        ));
    }
}
