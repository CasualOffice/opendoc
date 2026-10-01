// SPDX-License-Identifier: Apache-2.0

//! TP1 as a property, not as examples.
//!
//! The convergence law is
//!
//! ```text
//! apply(apply(S, a), transform(b, a, Later)) == apply(apply(S, b), transform(a, b, Earlier))
//! ```
//!
//! and it is checked over the cross product of a generated operation set rather than over
//! cases someone thought of. Transform functions fail on the pair nobody considered — that
//! is how OT ships broken — and the op set being closed with a deterministic `apply` is
//! exactly what makes exhaustive generation possible.
//!
//! # What "identical" means here, and the one thing it quotients out
//!
//! `casual_doc_edit::apply` **mints** identities when an edit has to split a run: a bold
//! applied to the middle of a run produces a tail run whose id comes from `RunIds`, not
//! from the operation. Two replicas running two orders therefore mint different *values*
//! for the same logical run, and no transform can fix that — the identity is not on the
//! wire.
//!
//! So the comparison canonicalises **run** identities by document-order rank and is exact
//! on everything else: every block id, every object id, every property, every byte of
//! text, and the order of all of it. Nothing that an operation *carries* is quotiented, and
//! a run relabelled into a different position still fails, because the rank is the
//! position. Doc 150 §2.3 records the finding this exposes: making convergence
//! byte-identical would mean operations carrying the identities they cause to be minted,
//! which is an op-set change and therefore a finding, not a licence.

use casual_doc_edit::{FormatDelta, Operation, Pos, Range as EditRange};
use casual_doc_model::v1::{
    BlockNode, Definitions, GridColumn, InlineNode, Paragraph, ParagraphProperties, Run,
    RunProperties, Table, TableCell, TableCellProperties, TableProperties, TableRow,
    TableRowProperties,
};
use casual_doc_model::{IdGenerator, NodeId};

use crate::{Coalesce, RevisionLog, Transaction, TransactionId};

use super::*;

/// The ids the seed hands out, so a candidate operation can name them.
struct Seed {
    document: Document,
    /// Body paragraphs, in body order.
    paragraphs: Vec<NodeId>,
    /// The body table.
    table: NodeId,
    /// The table's first cell.
    cell: NodeId,
    /// A generator for the fresh ids candidate operations carry.
    ids: IdGenerator,
}

fn paragraph(id: NodeId, run: NodeId, text: &str) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id,
        properties: ParagraphProperties::default().into(),
        inlines: vec![InlineNode::Run(Run {
            id: run,
            properties: RunProperties::default().into(),
            text: text.to_owned(),
        })],
    })
}

/// A small document with enough text, structure and nesting that a divergence shows up in
/// the comparison rather than in an empty body.
fn seed() -> Seed {
    let mut ids = IdGenerator::new(1);
    let document_id = ids.next_id().expect("id");
    let mut blocks = Vec::new();
    let mut paragraphs = Vec::new();
    // Every paragraph holds one run of the same length, so offsets, ties and containments
    // all land on the same numbers across paragraphs rather than by luck.
    for _ in 0..5 {
        let id = ids.next_id().expect("id");
        let run = ids.next_id().expect("id");
        paragraphs.push(id);
        blocks.push(paragraph(id, run, "abcdefgh"));
    }
    let table = ids.next_id().expect("id");
    let mut rows = Vec::new();
    let mut first_cell = None;
    for _ in 0..3 {
        let mut cells = Vec::new();
        for _ in 0..2 {
            let cell = ids.next_id().expect("id");
            if first_cell.is_none() {
                first_cell = Some(cell);
            }
            let inner = ids.next_id().expect("id");
            let inner_run = ids.next_id().expect("id");
            cells.push(TableCell {
                id: cell,
                properties: TableCellProperties::default(),
                blocks: vec![paragraph(inner, inner_run, "wxyz")],
            });
        }
        rows.push(TableRow {
            id: ids.next_id().expect("id"),
            properties: TableRowProperties::default(),
            cells,
        });
    }
    blocks.push(BlockNode::Table(Box::new(Table {
        id: table,
        grid: vec![
            GridColumn {
                width_twips: Some(1000),
            },
            GridColumn {
                width_twips: Some(1000),
            },
        ],
        grid_change: None,
        properties: TableProperties::default(),
        rows,
    })));
    // A trailing paragraph so the table is never the last block, which keeps the
    // body-band arithmetic exercised on both sides of it.
    let tail = ids.next_id().expect("id");
    let tail_run = ids.next_id().expect("id");
    paragraphs.push(tail);
    blocks.push(paragraph(tail, tail_run, "abcdefgh"));

    let document = Document::new(document_id, blocks, Definitions::default()).expect("document");
    Seed {
        document,
        paragraphs,
        table,
        cell: first_cell.expect("a cell"),
        ids,
    }
}

fn fresh_paragraph(ids: &mut IdGenerator, text: &str) -> BlockNode {
    let id = ids.next_id().expect("id");
    let run = ids.next_id().expect("id");
    paragraph(id, run, text)
}

fn fresh_cells(ids: &mut IdGenerator, rows: usize) -> Vec<TableCell> {
    (0..rows)
        .map(|_| TableCell {
            id: ids.next_id().expect("id"),
            properties: TableCellProperties::default(),
            blocks: vec![fresh_paragraph(ids, "new")],
        })
        .collect()
}

/// The generated operations, clustered on the same paragraphs, offsets, containers and
/// indices so that ties, containments and overlaps are all hit on purpose.
#[allow(clippy::too_many_lines)]
fn candidates(seed: &mut Seed) -> Vec<Operation> {
    let mut ops = Vec::new();
    let paragraphs = seed.paragraphs.clone();
    let ids = &mut seed.ids;

    for node in [paragraphs[0], paragraphs[1]] {
        for offset in [0_u32, 3, 8] {
            ops.push(Operation::InsertText {
                at: Pos::new(node, offset),
                text: "XY".to_owned(),
            });
            ops.push(Operation::SplitParagraph {
                at: Pos::new(node, offset),
                new_id: ids.next_id().expect("id"),
                properties: None,
            });
        }
        for (start, end) in [(0_u32, 3_u32), (2, 6), (3, 8), (0, 8)] {
            ops.push(Operation::DeleteText {
                range: EditRange {
                    start: Pos::new(node, start),
                    end: Pos::new(node, end),
                },
            });
            ops.push(Operation::FormatText {
                range: EditRange {
                    start: Pos::new(node, start),
                    end: Pos::new(node, end),
                },
                delta: FormatDelta {
                    bold: Some(true),
                    ..FormatDelta::default()
                },
            });
        }
        ops.push(Operation::ClearFormatting {
            range: EditRange {
                start: Pos::new(node, 2),
                end: Pos::new(node, 6),
            },
        });
        ops.push(Operation::SetParagraphProperties {
            node,
            properties: Box::new(ParagraphProperties::default()),
        });
    }
    ops.push(Operation::JoinParagraphs {
        first: paragraphs[0],
        second: paragraphs[1],
        properties: None,
    });
    ops.push(Operation::JoinParagraphs {
        first: paragraphs[1],
        second: paragraphs[2],
        properties: None,
    });
    ops.push(Operation::SetInlines {
        node: paragraphs[1],
        inlines: vec![InlineNode::Run(Run {
            id: ids.next_id().expect("id"),
            properties: RunProperties::default().into(),
            text: "rewritten".to_owned(),
        })],
    });

    // Body bands: insertions at, before and after the deletions, and deletions that
    // contain, abut and overlap each other.
    for index in [0_u32, 1, 3, 5] {
        ops.push(Operation::InsertBlocks {
            container: None,
            index,
            blocks: vec![fresh_paragraph(ids, "pasted")],
        });
    }
    for (index, count) in [(0_u32, 1_u32), (1, 2), (2, 1), (3, 2)] {
        ops.push(Operation::DeleteBlocks {
            container: None,
            index,
            count,
        });
    }
    // A band in a nested container, so "the same index in a different container" is
    // exercised rather than assumed.
    ops.push(Operation::InsertBlocks {
        container: Some(seed.cell),
        index: 0,
        blocks: vec![fresh_paragraph(ids, "nested")],
    });

    // Table structure. `InsertColumn` refuses an irregular table, so the grid stays
    // regular wherever these apply, which is why the index arithmetic is total.
    for index in [0_u32, 1, 3] {
        ops.push(Operation::InsertRow {
            table: seed.table,
            index,
            row: Box::new(TableRow {
                id: ids.next_id().expect("id"),
                properties: TableRowProperties::default(),
                cells: fresh_cells(ids, 2),
            }),
        });
    }
    for index in [0_u32, 1, 2] {
        ops.push(Operation::DeleteRow {
            table: seed.table,
            index,
        });
    }
    for index in [0_u32, 1, 2] {
        ops.push(Operation::InsertColumn {
            table: seed.table,
            index,
            width: Some(900),
            cells: fresh_cells(ids, 3),
        });
    }
    ops.push(Operation::DeleteColumn {
        table: seed.table,
        index: 0,
    });
    ops.push(Operation::DeleteColumn {
        table: seed.table,
        index: 1,
    });
    ops.push(Operation::SetTableProperties {
        table: seed.table,
        properties: Box::new(TableProperties::default()),
    });
    ops.push(Operation::SetTableCellProperties {
        cell: seed.cell,
        properties: Box::new(TableCellProperties::default()),
    });
    ops
}

/// The fresh identities an operation carries.
///
/// Two clients cannot mint the same fresh id, so a pair that shares one is not a
/// concurrency the wire can produce and is skipped rather than answered.
fn carried_ids(operation: &Operation) -> Vec<NodeId> {
    let mut out = Vec::new();
    match operation {
        Operation::SplitParagraph { new_id, .. } => out.push(*new_id),
        Operation::InsertBlocks { blocks, .. } => {
            for block in blocks {
                super::collect_block(block, &mut out);
            }
        }
        Operation::InsertRow { row, .. } => {
            out.push(row.id);
            for cell in &row.cells {
                out.push(cell.id);
                for block in &cell.blocks {
                    super::collect_block(block, &mut out);
                }
            }
        }
        Operation::InsertColumn { cells, .. } => {
            for cell in cells {
                out.push(cell.id);
                for block in &cell.blocks {
                    super::collect_block(block, &mut out);
                }
            }
        }
        Operation::SetInlines { inlines, .. } => {
            for inline in inlines {
                super::collect_inline(inline, &mut out);
            }
        }
        _ => {}
    }
    out
}

/// A generator far above the seed's own ids, so a minted run identity can never collide
/// with one an operation carries.
fn replica_ids() -> IdGenerator {
    IdGenerator::new(1_000_000)
}

/// Replaces every **run** identity with its document-order rank.
///
/// See the module docs: run ids are the only identities `apply` mints, and no operation
/// addresses one. Everything else is compared exactly.
fn canonicalise(document: &mut Document) {
    let mut next = 1_u128;
    canonicalise_blocks(document.body_mut(), &mut next);
}

fn canonicalise_blocks(blocks: &mut [BlockNode], next: &mut u128) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Paragraph(paragraph) => canonicalise_inlines(&mut paragraph.inlines, next),
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        canonicalise_blocks(&mut cell.blocks, next);
                    }
                }
            }
            BlockNode::Sdt(sdt) => canonicalise_blocks(&mut sdt.blocks, next),
            BlockNode::AltChunk(_) => {}
        }
    }
}

fn canonicalise_inlines(inlines: &mut [InlineNode], next: &mut u128) {
    for inline in inlines.iter_mut() {
        match inline {
            InlineNode::Run(run) => {
                run.id = NodeId::new(*next).expect("non-zero");
                *next += 1;
            }
            InlineNode::Hyperlink(node) => canonicalise_inlines(&mut node.inlines, next),
            InlineNode::Field(node) => canonicalise_inlines(&mut node.inlines, next),
            InlineNode::Revision(node) => canonicalise_inlines(&mut node.inlines, next),
            InlineNode::Sdt(node) => canonicalise_inlines(&mut node.inlines, next),
            InlineNode::TextBox(node) => canonicalise_blocks(&mut node.blocks, next),
            _ => {}
        }
    }
}

/// Applies one operation, returning its inverse.
fn apply_one(
    document: &mut Document,
    ids: &mut IdGenerator,
    operation: &Operation,
) -> Result<Operation, casual_doc_edit::EditError> {
    casual_doc_edit::apply(
        document,
        casual_doc_edit::Mint::reserve(ids, 1).expect("an identity space"),
        operation,
    )
}

/// One side of the diamond: apply `first`, rebase `second` past it, apply what survives.
///
/// `Err` carries the refusal; `Ok(None)` means the rebase applied but `apply` refused the
/// rebased operation, which is counted rather than waved through.
fn diamond_side(
    base: &Document,
    first: &Operation,
    second: &Operation,
    side: Side,
) -> Result<Result<Document, casual_doc_edit::EditError>, TransformError> {
    let mut document = base.clone();
    let mut ids = replica_ids();
    let inverse =
        apply_one(&mut document, &mut ids, first).expect("a candidate applies to the seed");
    let placement = BlockIndex::of(base);
    let rebased = transform_placed(second, Change::new(first, &inverse), side, &placement)?;
    for operation in rebased.into_operations() {
        if let Err(error) = apply_one(&mut document, &mut ids, &operation) {
            return Ok(Err(error));
        }
    }
    canonicalise(&mut document);
    Ok(Ok(document))
}

/// Pairs whose rebase is correct and which the MODEL then refuses, because applying both
/// would empty a container it requires to be non-empty. See the assertion that pins the
/// shape inside the property.
/// Two, and both are the same pair in its two orderings: `DeleteColumn { index: 0 }`
/// against `DeleteColumn { index: 1 }` on the two-column table. Each rebases to
/// `DeleteColumn { index: 0 }` correctly, and the model then refuses to remove a table's
/// only column.
const MODEL_INVARIANT_REFUSALS: usize = 2;

#[test]
fn tp1_holds_for_every_supported_pair() {
    let mut seed = seed();
    let base = seed.document.clone();
    let operations = candidates(&mut seed);

    let mut checked = 0_usize;
    let mut refused = 0_usize;
    let mut apply_failures = 0_usize;

    for a in &operations {
        // Only operations that actually apply to the shared state are concurrency the wire
        // can produce.
        let mut probe = base.clone();
        if apply_one(&mut probe, &mut replica_ids(), a).is_err() {
            continue;
        }
        let a_ids = carried_ids(a);
        for b in &operations {
            let mut probe = base.clone();
            if apply_one(&mut probe, &mut replica_ids(), b).is_err() {
                continue;
            }
            if carried_ids(b).iter().any(|id| a_ids.contains(id)) {
                continue;
            }
            // The two replicas must agree on ONE order before they can agree on a result,
            // so fix it here: `a` is ordered before `b`. Each side then transforms with the
            // role that order gives it — the two halves of the same diamond.
            let (Ok(left), Ok(right)) = (
                diamond_side(&base, a, b, Side::Later),
                diamond_side(&base, b, a, Side::Earlier),
            ) else {
                refused += 1;
                continue;
            };
            // A rebase that the MODEL refuses is a different outcome from a divergence, and
            // it is part of the contract rather than a hole in it: `transform` guarantees
            // the rebased operation expresses the right intention, not that the engine's
            // own invariants will accept it. Two people deleting the two columns of a
            // two-column table concurrently is the whole of this shape — the second delete
            // rebases correctly and is then refused because a table keeps at least one
            // column. The caller must treat that exactly as it treats `Unsupported`.
            let (left, right) = match (left, right) {
                (Ok(left), Ok(right)) => (left, right),
                (Err(error), _) | (_, Err(error)) => {
                    assert_eq!(
                        error,
                        casual_doc_edit::EditError::Unsupported,
                        "a rebased operation was refused for a reason other than a model \
                         invariant\n  a = {a:?}\n  b = {b:?}"
                    );
                    apply_failures += 1;
                    continue;
                }
            };
            assert_eq!(
                left, right,
                "TP1 violated\n  a = {a:?}\n  b = {b:?}\n  left  = {left:?}\n  right = {right:?}"
            );
            checked += 1;
        }
    }

    // Guards against the property passing because everything was skipped. The refusal
    // surface is real and enumerated (doc 150 §6), so refusals are counted, not waved
    // through, and the answered pairs must heavily outnumber them.
    assert!(
        checked > 3_000,
        "only {checked} pairs checked — the generator stopped covering the op set"
    );
    assert!(
        refused * 4 < checked,
        "{refused} refused against {checked} answered — too much is being refused"
    );
    // Pinned exactly, not bounded: a pair that starts or stops being refused by the model
    // is a behaviour change and neither is a thing to accept quietly. Every one of these
    // is a structural removal that would empty a container the model requires to be
    // non-empty; the assertion in the loop pins the shape, this pins the size.
    assert_eq!(
        apply_failures, MODEL_INVARIANT_REFUSALS,
        "the set of pairs the model refuses changed"
    );
}

#[test]
fn the_refusal_surface_is_exactly_these_cases() {
    // The sibling engine went three weeks with an undocumented refusal because every test
    // asked whether one pair behaved and none asked what the whole surface *was*. This
    // pins it two ways.
    //
    // First: a refusal may not be written as a bare literal. Every refusal names a
    // constant and every constant is in `REFUSAL_REASONS`, so a new one cannot reach a
    // caller without appearing in the list doc 150 §6 documents. CRLF-normalised, because
    // two Windows-only CI failures came from literals not matching a CRLF checkout.
    // All three files, because the classification and effect submodules refuse too and a
    // guard that scans only the parent would have gone blind the moment the module was
    // split — which is exactly what happened to it.
    let source = [
        include_str!("transform.rs"),
        include_str!("transform/classify.rs"),
        include_str!("transform/effect.rs"),
    ]
    .join("\n")
    .replace("\r\n", "\n");
    let bare = source.matches("Err(refuse(\"").count()
        + source.matches("return Err(\"").count()
        + source.matches(".ok_or(\"").count();
    assert_eq!(
        bare, 0,
        "a refusal is written as a bare string literal instead of a `REFUSAL_REASONS` \
         constant, so it can reach a caller without being documented"
    );
    // The guard has to be able to see the thing it forbids.
    let planted = format!("{source}\nreturn Err(\"invented\");\n");
    assert_eq!(planted.matches("return Err(\"").count(), 1);

    // Second: nothing outside the list reaches the surface in practice, and the list is
    // not aspirational — the generator actually reaches the placement refusals.
    let mut seed = seed();
    let base = seed.document.clone();
    let operations = candidates(&mut seed);
    let mut seen: Vec<&'static str> = Vec::new();
    for a in &operations {
        let mut probe = base.clone();
        let Ok(inverse) = apply_one(&mut probe, &mut replica_ids(), a) else {
            continue;
        };
        for b in &operations {
            // Deliberately with NO placement, which is the surface a session that cannot
            // resolve the base state sees.
            for side in [Side::Later, Side::Earlier] {
                if let Err(TransformError::Unsupported { reason, .. }) =
                    transform(b, Change::new(a, &inverse), side)
                    && !seen.contains(&reason)
                {
                    seen.push(reason);
                }
            }
        }
    }
    for reason in &seen {
        assert!(
            REFUSAL_REASONS.contains(reason),
            "an unrecorded refusal reached the surface: {reason}"
        );
    }
    // The two the generator is built to reach without a placement.
    assert!(
        seen.contains(&REFUSAL_REASONS[4]) || seen.contains(&REFUSAL_REASONS[5]),
        "the placement refusals were not reached: {seen:?}"
    );
}

#[test]
fn a_placement_answers_the_pairs_a_bare_transform_refuses() {
    let mut seed = seed();
    let base = seed.document.clone();
    let paragraphs = seed.paragraphs.clone();
    let split = Operation::SplitParagraph {
        at: Pos::new(paragraphs[1], 4),
        new_id: seed.ids.next_id().expect("id"),
        properties: None,
    };
    let paste = Operation::InsertBlocks {
        container: None,
        index: 0,
        blocks: vec![fresh_paragraph(&mut seed.ids, "pasted")],
    };
    let mut document = base.clone();
    let inverse = apply_one(&mut document, &mut replica_ids(), &split).expect("split applies");

    assert!(
        matches!(
            transform(&paste, Change::new(&split, &inverse), Side::Later),
            Err(TransformError::Unsupported { .. })
        ),
        "without a placement the pair has no answer"
    );
    let placement = BlockIndex::of(&base);
    let rebased = transform_placed(
        &paste,
        Change::new(&split, &inverse),
        Side::Later,
        &placement,
    )
    .expect("a placement answers it");
    // The paste is before the split, so it does not move.
    assert_eq!(rebased, Rebase::Keep(paste));
}

#[test]
fn concurrent_insertions_at_one_boundary_order_by_the_settled_order() {
    let seed = seed();
    let node = seed.paragraphs[0];
    let earlier = Operation::InsertText {
        at: Pos::new(node, 4),
        text: "A".to_owned(),
    };
    let later = Operation::InsertText {
        at: Pos::new(node, 4),
        text: "B".to_owned(),
    };
    let mut left = seed.document.clone();
    let inverse = apply_one(&mut left, &mut replica_ids(), &earlier).expect("applies");
    let rebased = transform(&later, Change::new(&earlier, &inverse), Side::Later)
        .expect("a text tie has an answer");
    let Rebase::Keep(Operation::InsertText { at, .. }) = rebased else {
        panic!("expected a moved insertion, got {rebased:?}");
    };
    assert_eq!(
        at.offset, 5,
        "the later operation moves off the contested boundary"
    );

    let mut right = seed.document.clone();
    let inverse = apply_one(&mut right, &mut replica_ids(), &later).expect("applies");
    let rebased = transform(&earlier, Change::new(&later, &inverse), Side::Earlier)
        .expect("a text tie has an answer");
    let Rebase::Keep(Operation::InsertText { at, .. }) = rebased else {
        panic!("expected a held insertion, got {rebased:?}");
    };
    assert_eq!(
        at.offset, 4,
        "the earlier operation holds the boundary it asked for"
    );
}

#[test]
fn formatting_applies_to_whatever_survived_the_concurrent_deletion() {
    let seed = seed();
    let node = seed.paragraphs[0];
    let delete = Operation::DeleteText {
        range: EditRange {
            start: Pos::new(node, 2),
            end: Pos::new(node, 5),
        },
    };
    let format = Operation::FormatText {
        range: EditRange {
            start: Pos::new(node, 1),
            end: Pos::new(node, 7),
        },
        delta: FormatDelta {
            bold: Some(true),
            ..FormatDelta::default()
        },
    };
    let mut document = seed.document.clone();
    let inverse = apply_one(&mut document, &mut replica_ids(), &delete).expect("applies");
    let rebased =
        transform(&format, Change::new(&delete, &inverse), Side::Later).expect("has an answer");
    let Rebase::Keep(Operation::FormatText { range, .. }) = rebased else {
        panic!("expected the surviving range, got {rebased:?}");
    };
    assert_eq!((range.start.offset, range.end.offset), (1, 4));

    // And when nothing survives it is `Satisfied`, not a tombstone: the text this
    // operation meant to embolden is gone, so nothing was lost by *this* operation.
    let inner = Operation::FormatText {
        range: EditRange {
            start: Pos::new(node, 3),
            end: Pos::new(node, 4),
        },
        delta: FormatDelta {
            bold: Some(true),
            ..FormatDelta::default()
        },
    };
    assert_eq!(
        transform(&inner, Change::new(&delete, &inverse), Side::Later).expect("has an answer"),
        Rebase::Satisfied
    );
}

#[test]
fn a_concurrent_split_divides_a_range_into_two_operations() {
    let mut seed = seed();
    let node = seed.paragraphs[0];
    let new_id = seed.ids.next_id().expect("id");
    let split = Operation::SplitParagraph {
        at: Pos::new(node, 4),
        new_id,
        properties: None,
    };
    let delete = Operation::DeleteText {
        range: EditRange {
            start: Pos::new(node, 2),
            end: Pos::new(node, 6),
        },
    };
    let mut document = seed.document.clone();
    let inverse = apply_one(&mut document, &mut replica_ids(), &split).expect("applies");
    let rebased =
        transform(&delete, Change::new(&split, &inverse), Side::Later).expect("has an answer");
    let Rebase::KeepMany(parts) = rebased else {
        panic!("expected a divided range, got {rebased:?}");
    };
    let [first, second] = <[Operation; 2]>::try_from(parts).expect("exactly two");
    assert_eq!(
        first,
        Operation::DeleteText {
            range: EditRange {
                start: Pos::new(node, 2),
                end: Pos::new(node, 4),
            },
        }
    );
    assert_eq!(
        second,
        Operation::DeleteText {
            range: EditRange {
                start: Pos::new(new_id, 0),
                end: Pos::new(new_id, 2),
            },
        }
    );

    // U1: the same shape is refused for a hyperlink, which carries one fresh id and would
    // need two wrappers.
    let link = Operation::SetHyperlink {
        range: EditRange {
            start: Pos::new(node, 2),
            end: Pos::new(node, 6),
        },
        id: seed.ids.next_id().expect("id"),
        target: None,
        tooltip: None,
    };
    assert!(matches!(
        transform(&link, Change::new(&split, &inverse), Side::Later),
        Err(TransformError::Unsupported { .. })
    ));
}

#[test]
fn a_removed_anchor_is_a_tombstone_and_not_a_silent_no_op() {
    let seed = seed();
    let doomed = seed.paragraphs[1];
    let delete = Operation::DeleteBlocks {
        container: None,
        index: 1,
        count: 1,
    };
    let write = Operation::SetParagraphProperties {
        node: doomed,
        properties: Box::new(ParagraphProperties::default()),
    };
    let mut document = seed.document.clone();
    let inverse = apply_one(&mut document, &mut replica_ids(), &delete).expect("applies");
    let rebased =
        transform(&write, Change::new(&delete, &inverse), Side::Later).expect("has an answer");
    assert_eq!(
        rebased,
        Rebase::Tombstoned(Tombstone {
            operation: "SetParagraphProperties",
            against: "DeleteBlocks",
        }),
        "a dropped operation must be reportable, not a silent no-op"
    );
    assert!(rebased.into_operations().is_empty());
}

#[test]
fn an_anchor_nested_inside_removed_content_is_also_a_tombstone() {
    let seed = seed();
    let cell = seed.cell;
    // Removing the first row takes its cells and their paragraphs with it. The identities
    // are only in the inverse, which is why transform is defined over the pair.
    let delete = Operation::DeleteRow {
        table: seed.table,
        index: 0,
    };
    let write = Operation::SetTableCellProperties {
        cell,
        properties: Box::new(TableCellProperties::default()),
    };
    let mut document = seed.document.clone();
    let inverse = apply_one(&mut document, &mut replica_ids(), &delete).expect("applies");
    assert!(matches!(
        transform(&write, Change::new(&delete, &inverse), Side::Later),
        Ok(Rebase::Tombstoned(_))
    ));
}

#[test]
fn contention_yields_only_the_aspects_the_later_write_overwrites() {
    let mut seed = seed();
    let node = seed.paragraphs[0];
    let split = Operation::SplitParagraph {
        at: Pos::new(node, 4),
        new_id: seed.ids.next_id().expect("id"),
        properties: None,
    };
    let earlier = Operation::SetParagraphProperties {
        node,
        properties: Box::new(ParagraphProperties::default()),
    };
    let later = Operation::SetParagraphProperties {
        node,
        properties: Box::new(ParagraphProperties::default()),
    };
    let mut document = seed.document.clone();
    let inverse = apply_one(&mut document, &mut replica_ids(), &later).expect("applies");
    assert_eq!(
        transform(&earlier, Change::new(&later, &inverse), Side::Earlier).expect("has an answer"),
        Rebase::Satisfied,
        "the earlier write of the same aspect yields"
    );
    // A different aspect of the same node is untouched; so is an unrelated operation.
    let mut document = seed.document.clone();
    let inverse = apply_one(&mut document, &mut replica_ids(), &split).expect("applies");
    assert!(matches!(
        transform(&earlier, Change::new(&split, &inverse), Side::Earlier),
        Ok(Rebase::Keep(_))
    ));
}

#[test]
fn operations_in_different_paragraphs_are_left_alone() {
    // The property that makes the 55x55 matrix mostly identity: an operation addressed to
    // a node survives a neighbour's edit with no arithmetic at all.
    let seed = seed();
    let typing = Operation::InsertText {
        at: Pos::new(seed.paragraphs[0], 3),
        text: "hello".to_owned(),
    };
    let elsewhere = Operation::InsertText {
        at: Pos::new(seed.paragraphs[3], 3),
        text: "world".to_owned(),
    };
    let mut document = seed.document.clone();
    let inverse = apply_one(&mut document, &mut replica_ids(), &typing).expect("applies");
    assert_eq!(
        transform(&elsewhere, Change::new(&typing, &inverse), Side::Later).expect("has an answer"),
        Rebase::Keep(elsewhere),
        "a different paragraph cannot move"
    );
}

#[test]
fn a_commit_without_inverses_cannot_be_a_concurrent_change() {
    let mut seed = seed();
    let mut log = RevisionLog::default();
    let mut ids = replica_ids();
    let node = seed.paragraphs[0];
    let transaction = Transaction::reserve(
        TransactionId::new(1),
        log.head(),
        "Typing",
        &mut ids,
        vec![Operation::InsertText {
            at: Pos::new(node, 0),
            text: "a".to_owned(),
        }],
    )
    .expect("identity spaces");
    log.apply(&mut seed.document, transaction).expect("applies");
    assert!(
        log.commits().last().expect("a commit").changes().is_some(),
        "an ordinary commit can serve as a concurrent change"
    );

    let transaction = Transaction::reserve(
        TransactionId::new(2),
        log.head(),
        "Typing",
        &mut ids,
        vec![Operation::InsertText {
            at: Pos::new(node, 1),
            text: "b".to_owned(),
        }],
    )
    .expect("identity spaces")
    .coalescing(Coalesce::ContinueKeepingFirstInverse);
    log.apply(&mut seed.document, transaction).expect("applies");
    assert!(
        log.commits().last().expect("a commit").changes().is_none(),
        "a commit that kept no inverse cannot say what it destroyed (doc 150 §10 Q2)"
    );
}

#[test]
fn transform_cost_does_not_scale_with_the_document() {
    // SKILL §8: guard complexity by DOUBLING, never by a clock. A timing threshold cannot
    // tell a slow constant from a quadratic and is flaky under load; this counts work.
    fn work(blocks: usize) -> (usize, usize) {
        let mut ids = IdGenerator::new(1);
        let document_id = ids.next_id().expect("id");
        let mut body = Vec::new();
        for _ in 0..blocks {
            body.push(fresh_paragraph(&mut ids, "abcdefgh"));
        }
        let mut document =
            Document::new(document_id, body, Definitions::default()).expect("document");
        // The concurrent change removes a fixed four blocks whatever the document holds.
        let delete = Operation::DeleteBlocks {
            container: None,
            index: 0,
            count: 4,
        };
        let inverse = apply_one(&mut document, &mut replica_ids(), &delete).expect("applies");
        let removed = super::removed_by(Change::new(&delete, &inverse));
        let index = BlockIndex::of(&document);
        (removed.nodes.len(), index.positions.len())
    }

    let (small_transform, small_index) = work(64);
    let (large_transform, large_index) = work(128);
    assert_eq!(
        small_transform, large_transform,
        "transform read {small_transform} nodes at n and {large_transform} at 2n — it is \
         scaling with the document, which breaks `107` B2"
    );
    // The same measurement, proving it can see growth: `BlockIndex::of` IS O(document), and
    // a guard that cannot detect scaling is not a guard.
    assert!(
        large_index >= small_index * 2 - 4,
        "the measurement cannot see growth: index went {small_index} -> {large_index}"
    );
}

/// Every `.rs` file under `root`, sorted, with CRLF normalised.
///
/// CRLF-normalised because two Windows-only CI failures came from literals containing `\n`
/// not matching a CRLF checkout (doc 147 §5).
fn sources(root: &std::path::Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let entries = std::fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("{} is not readable: {error}", directory.display()));
        for entry in entries {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("{} is not readable: {error}", path.display()));
                found.push((
                    path.file_name()
                        .expect("a file name")
                        .to_string_lossy()
                        .into_owned(),
                    text.replace("\r\n", "\n"),
                ));
            }
        }
    }
    assert!(
        !found.is_empty(),
        "no sources found under {} — a guard that reads nothing proves nothing",
        root.display()
    );
    found.sort();
    found
}

#[test]
fn the_keystroke_path_runs_no_transform() {
    // `107` exit gate 7 and the sibling's own commitment: OT is dormant at one editor, so a
    // lone editor pays nothing for a feature it is not using.
    //
    // WHY THIS IS NOT THE GUARD IT REPLACES. `single_user_editing_does_not_call_transform`
    // scanned `lib.rs` and only `lib.rs`. That asserted a *circumstance* — that one file
    // happened to hold no call — and it went blind the moment a sibling module in this crate
    // called `transform`, which `session.rs` now does on purpose. Re-run unchanged it would
    // have stayed green while saying nothing at all about the keystroke path.
    //
    // So this asserts the guarantee, in two halves:
    //
    //   1. inside this crate, `session` is the ONLY module that reaches transform's entry
    //      points, and no file on the keystroke path names `session`;
    //   2. the live editor (`casual-doc-wasm`) does not reach the collaboration modules at
    //      all, so nothing a lone editor executes can run a line of them even by accident.
    const ENTRY_POINTS: [&str; 3] = ["transform(", "transform_placed(", "BlockIndex::of("];
    // `transform` is allowed to call itself and to be tested; `session` is the one consumer
    // ADR-047 sanctions, and it is unreachable without a host having joined a room.
    const MAY_TRANSFORM: [&str; 6] = [
        "transform.rs",
        "classify.rs",
        "effect.rs",
        "transform_tests.rs",
        "session.rs",
        "session_tests.rs",
    ];
    // The collaboration modules themselves, which may of course name each other.
    const COLLABORATION: [&str; 3] = ["protocol.rs", "wire.rs", "session.rs"];

    let engine = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    for (name, text) in sources(&engine) {
        if !MAY_TRANSFORM.contains(&name.as_str()) {
            for entry in ENTRY_POINTS {
                if text.contains(entry) {
                    offenders.push(format!("{name} reaches `{entry}`"));
                }
            }
        }
        // The other half of the same rule: a file on the keystroke path must not reach the
        // module that does transform, or the keystroke path acquires one a call deeper.
        // Test files are excluded because a test is not a keystroke path — and because THIS
        // file has to name the forbidden strings in order to forbid them, which is the
        // shape of self-reference every source-scanning guard hits sooner or later.
        if !COLLABORATION.contains(&name.as_str()) && !name.ends_with("_tests.rs") {
            for collab in ["session::", "ClientSession", "ServerSession"] {
                if text.contains(collab) {
                    offenders.push(format!("{name} reaches `{collab}`"));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the keystroke path now reaches the collaboration machinery: {offenders:?}. \
         Single-user editing must contact nothing and run no transform."
    );

    // The guard is only worth having if it can see the thing it forbids.
    let planted = "let _ = transform::transform(a, b, side);\n".replace("\r\n", "\n");
    assert!(
        ENTRY_POINTS.iter().any(|entry| planted.contains(entry)),
        "the scan cannot see a call it is supposed to forbid"
    );
}

#[test]
fn the_live_editor_has_no_collaboration_dependency() {
    // The second half of `107` exit gate 7, and the half a source scan of THIS crate can
    // never see: `casual-doc-wasm` is the live editing path, and it must not reach the
    // collaboration modules at all. A lone editor then cannot execute one line of them, which
    // is a stronger statement than "the keystroke path does not call transform" — it is
    // "collaboration is something a session acquires, not a mode the engine is built in".
    //
    // Doc 152 §9 states the scope this pins: `casual-doc-wasm` is deliberately unchanged by
    // that increment, so this guard is what stops the next one changing it by accident.
    const FORBIDDEN: [&str; 6] = [
        "casual_doc_transaction::session",
        "casual_doc_transaction::protocol",
        "casual_doc_transaction::wire",
        "ClientSession",
        "ServerSession",
        "WireOperation",
    ];

    let editor = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("casual-doc-wasm")
        .join("src");
    let mut offenders = Vec::new();
    for (name, text) in sources(&editor) {
        for forbidden in FORBIDDEN {
            if text.contains(forbidden) {
                offenders.push(format!("{name} reaches `{forbidden}`"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the live editor now reaches the collaboration modules: {offenders:?}"
    );
    let planted = "use casual_doc_transaction::session::ClientSession;";
    assert!(
        FORBIDDEN.iter().any(|item| planted.contains(item)),
        "the scan cannot see a dependency it is supposed to forbid"
    );
}
