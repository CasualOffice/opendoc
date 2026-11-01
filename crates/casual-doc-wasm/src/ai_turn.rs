// SPDX-License-Identifier: Apache-2.0

//! The assistant as a live co-editor: one AI TURN writes into the document the way a person
//! types, while the person keeps editing elsewhere (`docs/183` §23, `docs/109` RM-52).
//!
//! # The pattern, named first (SKILL §8)
//!
//! **Streaming with batched commits under a scoped history group**, which is Google Docs'
//! "Help me write" and Notion AI's inline writer, built on the two mechanisms this engine
//! already has for a second writer: a transaction per batch through the one choke point
//! (`apply_group`, so access and protection are checked exactly as for the person), and
//! selective undo by transformation (`casual_doc_transaction::selective`). What is new is the
//! history scope: every commit of a turn joins ONE turn group (`Coalesce::Join`), which the
//! person's own typing never continues, which every history read sees once, and which is
//! reverted by rebasing over whatever the person did meanwhile. So the turn is one undo step,
//! Reject all is one step, and neither touches a character the person typed.
//!
//! # The insertion point is an anchor, not an offset (ADR-030 I3)
//!
//! The turn holds `(node, offset)` and the revision it was valid at. Before each write it is
//! mapped forward through every later commit that is not the turn's own, by the commits'
//! recorded position maps — the person typing in front of it in the same paragraph, splitting
//! it, deleting it. That is O(commits since the last write), never O(document). A paragraph
//! somebody removed ends the turn with `ai.turn-anchor-gone`, rather than writing somewhere
//! the assistant did not mean.
//!
//! # Suggestions or direct edits
//!
//! The host decides the EFFECTIVE mode (the most restrictive of the person's setting, the
//! document's state and their role) and opens the turn in it. In `suggest` every write is a
//! tracked revision authored "Casual AI" — one insertion revision for a whole streamed answer,
//! because each batch continues the previous batch's revision group while the anchor is where
//! the last batch ended. In `direct` the writes are ordinary edits. Either way the choke point
//! re-checks the replica's grant and the document's protection on every batch.
//!
//! # Cost
//!
//! A write is one lowering over the anchor's paragraph plus the incremental re-layout every
//! keystroke pays: O(paragraph), O(1) in document size. Accept all reads the revision list
//! once (O(document)), on the person's click only.

use std::str::FromStr;

use casual_doc_edit::refusal::marked;
use casual_doc_edit::{Operation, Pos};
use casual_doc_model::NodeId;
use casual_doc_model::v1::RevisionGroup;
use casual_doc_model::v1::{
    BlockNode, MarkRevision, MarkRevisionKind, Paragraph, ParagraphProperties,
};
use casual_doc_review::ReviewIds;
use casual_doc_review::body::build_review_revision_summaries;
use casual_doc_review::lower as review_lower;
use casual_doc_review::table_review;
use casual_doc_transaction::{Affinity, Coalesce, GroupId, Origin, Position, RevisionId};
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::{EditResult, WasmDocument, damage_of, to_js};

/// The undo-step name of an AI turn and of its decisions.
pub(crate) const AI_TURN_LABEL: &str = "Casual AI";

/// Whether a turn writes suggestions or ordinary edits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TurnMode {
    Direct,
    Suggest,
}

/// One live turn.
#[derive(Clone, Debug)]
pub(crate) struct AiTurn {
    id: u32,
    mode: TurnMode,
    /// The turn's undo groups, oldest first: one, unless the turn outran
    /// `MAX_COMMITS_PER_TURN`.
    groups: Vec<GroupId>,
    /// The group the next commit joins.
    group: GroupId,
    date: String,
    /// The insertion point, valid at revision `seen`.
    anchor: Option<Pos>,
    seen: RevisionId,
    /// Where the last write ended, and the insertion group it opened, so the next batch
    /// continues the same revision (suggest mode).
    continuing: Option<(Pos, RevisionGroup)>,
    commits: usize,
    open: bool,
    /// The last table the turn inserted.
    table: Option<NodeId>,
    /// The revision serials (`w:id`) the turn's own commits allocated, as half-open
    /// ranges, oldest first, adjacent ranges merged. This, not the date, is what makes a
    /// revision the turn's: `w:date` is second-granular, so two turns started in the
    /// same second share it, and matching on it let one turn's Accept all accept the
    /// other's suggestions (RM-52).
    serials: Vec<(u128, u128)>,
    /// The allocator's next serial when the turn last committed (or began).
    serial_mark: u128,
}

impl AiTurn {
    /// Whether `serial` was allocated by one of this turn's commits. O(ranges).
    fn owns_serial(&self, serial: &str) -> bool {
        serial.parse::<u128>().is_ok_and(|serial| {
            self.serials
                .iter()
                .any(|(start, end)| (*start..*end).contains(&serial))
        })
    }
}

impl WasmDocument {
    fn turn(&self, turn: u32) -> Result<&AiTurn, String> {
        self.ai_turn
            .as_ref()
            .filter(|held| held.id == turn)
            .ok_or_else(|| marked("ai.turn-gone", "That Casual AI turn is no longer open."))
    }

    fn turn_mut(&mut self, turn: u32) -> Result<&mut AiTurn, String> {
        self.ai_turn
            .as_mut()
            .filter(|held| held.id == turn)
            .ok_or_else(|| marked("ai.turn-gone", "That Casual AI turn is no longer open."))
    }

    fn writing(&self, turn: u32) -> Result<&AiTurn, String> {
        let held = self.turn(turn)?;
        if held.open {
            Ok(held)
        } else {
            Err(marked(
                "ai.turn-closed",
                "That Casual AI turn has finished writing.",
            ))
        }
    }

    pub(crate) fn ai_turn_begin_inner(&mut self, mode: &str, date: &str) -> Result<u32, String> {
        if self.layout.is_windowed() {
            return Err(crate::windowed_not_available("Casual AI's editing"));
        }
        let mode = match mode {
            "direct" => TurnMode::Direct,
            "suggest" => TurnMode::Suggest,
            other => {
                return Err(format!(
                    "Unknown turn mode `{}`; it is direct or suggest.",
                    casual_doc_ai::text::clip(other, 40)
                ));
            }
        };
        // Monotonic for the document's lifetime, NOT derived from the open turn: once a turn
        // was decided `ai_turn` is empty, and restarting at 1 handed the next turn the id a
        // card of the earlier one still held.
        let id = self.ai_turn_serial.checked_add(1).ok_or_else(|| {
            marked(
                "ai.turn-ids-exhausted",
                "Casual AI has used every turn id this document has.",
            )
        })?;
        self.ai_turn_serial = id;
        let group = self.log.open_turn_group();
        self.ai_turn = Some(AiTurn {
            id,
            mode,
            groups: vec![group],
            group,
            date: date.chars().take(64).collect(),
            anchor: None,
            seen: self.log.head(),
            continuing: None,
            commits: 0,
            open: true,
            table: None,
            serials: Vec::new(),
            serial_mark: self.revision_ids.next,
        });
        Ok(id)
    }

    pub(crate) fn ai_turn_set_anchor_inner(
        &mut self,
        turn: u32,
        node: &str,
        offset: u32,
    ) -> Result<(), String> {
        self.writing(turn)?;
        let node = NodeId::from_str(node).map_err(|_| "invalid node".to_owned())?;
        if casual_doc_edit::find_paragraph_any(&self.document, node).is_none() {
            return Err(marked(
                "ai.turn-anchor-gone",
                "The place Casual AI was writing is no longer in the document.",
            ));
        }
        let head = self.log.head();
        let held = self.turn_mut(turn)?;
        held.anchor = Some(Pos::new(node, offset));
        held.seen = head;
        held.continuing = None;
        Ok(())
    }

    /// The turn's anchor mapped to the current head over everybody else's commits.
    /// O(commits since the anchor was last valid).
    fn mapped_anchor(&self, turn: &AiTurn) -> Result<Pos, String> {
        let Some(anchor) = turn.anchor else {
            return Err(marked(
                "ai.turn-no-anchor",
                "Casual AI has no place to write yet.",
            ));
        };
        let mut position = Position::new(anchor.node, anchor.offset, Affinity::Before);
        for commit in self.log.commits() {
            if commit.revision() <= turn.seen || turn.groups.contains(&commit.group()) {
                continue;
            }
            let mapped = commit.position_map().map_tracked(position);
            if mapped.removed {
                return Err(marked(
                    "ai.turn-anchor-gone",
                    "The place Casual AI was writing was removed, so it stopped.",
                ));
            }
            position = mapped.position;
        }
        Ok(Pos::new(position.node, position.offset))
    }

    /// Applies `ops` as one commit of the turn. O(ops) plus the choke point's own work.
    fn turn_commit(&mut self, turn: u32, ops: &[Operation]) -> Result<(), String> {
        let group = self.turn(turn)?.group;
        self.apply_group(ops, AI_TURN_LABEL, Coalesce::Join(group), Origin::Edit)?;
        let landed = self
            .log
            .commits()
            .last()
            .map(casual_doc_transaction::Commit::group);
        let head = self.log.head();
        let next_serial = self.revision_ids.next;
        let held = self.turn_mut(turn)?;
        held.commits += 1;
        held.seen = head;
        // The serials this commit's lowering allocated. A person's suggestion allocated
        // between two of the turn's commits falls in the range too, so membership also
        // requires the AI's author (`turn_revision_items`).
        if next_serial > held.serial_mark {
            match held.serials.last_mut() {
                Some(last) if last.1 == held.serial_mark => last.1 = next_serial,
                _ => held.serials.push((held.serial_mark, next_serial)),
            }
        }
        held.serial_mark = next_serial;
        if let Some(landed) = landed
            && landed != held.group
        {
            // Past the turn cap: the turn continues in the fresh turn group the log opened.
            held.group = landed;
            held.groups.push(landed);
        }
        Ok(())
    }

    fn turn_author(&self) -> Option<String> {
        Some(casual_doc_ai::AI_AUTHOR.to_owned())
    }

    fn turn_result(&mut self, caret: Pos, ops: &[Operation]) -> EditResult {
        self.finish_edit_with(caret, &damage_of(ops))
    }

    /// One piece of text (no line break) at the mapped anchor.
    fn turn_write_piece(&mut self, turn: u32, text: &str) -> Result<Vec<Operation>, String> {
        let held = self.writing(turn)?.clone();
        let at = self.mapped_anchor(&held)?;
        let end = Pos::new(
            at.node,
            at.offset
                .checked_add(u32::try_from(text.len()).map_err(|_| "text too large")?)
                .ok_or("text offset overflow")?,
        );
        let (ops, group) = match held.mode {
            TurnMode::Direct => (
                vec![Operation::InsertText {
                    at,
                    text: text.to_owned(),
                }],
                None,
            ),
            TurnMode::Suggest => {
                let continuing = held
                    .continuing
                    .filter(|(ended, _)| *ended == at)
                    .map(|(_, group)| group);
                let author = self.turn_author();
                let mut lower = |continuing| {
                    review_lower::suggest_insert(
                        &self.document,
                        &mut ReviewIds {
                            nodes: &mut self.edit_ids,
                            revisions: &mut self.revision_ids,
                        },
                        &at.node.to_string(),
                        at.offset,
                        text,
                        review_lower::InsertStyle::Plain,
                        author.clone(),
                        Some(held.date.clone()),
                        continuing,
                    )
                };
                // A batch that cannot continue the previous revision (the person typed
                // into it, say) opens its own.
                let lowered = match lower(continuing) {
                    Ok(lowered) => lowered,
                    Err(_) if continuing.is_some() => lower(None)?,
                    Err(error) => return Err(error),
                };
                (lowered.operations, lowered.group)
            }
        };
        self.turn_commit(turn, &ops)?;
        let held = self.turn_mut(turn)?;
        held.anchor = Some(end);
        held.continuing = group.map(|group| (end, group));
        Ok(ops)
    }

    /// A paragraph break at the mapped anchor; the anchor moves to the new paragraph.
    fn turn_break(&mut self, turn: u32) -> Result<Vec<Operation>, String> {
        let held = self.writing(turn)?.clone();
        let at = self.mapped_anchor(&held)?;
        let (ops, caret) = match held.mode {
            TurnMode::Direct => {
                let new_id = self
                    .edit_ids
                    .next_id()
                    .map_err(|_| "id space exhausted".to_owned())?;
                (
                    vec![Operation::SplitParagraph {
                        at,
                        new_id,
                        properties: None,
                    }],
                    Pos::new(new_id, 0),
                )
            }
            TurnMode::Suggest => {
                let author = self.turn_author();
                let lowered = review_lower::suggest_split(
                    &self.document,
                    &mut ReviewIds {
                        nodes: &mut self.edit_ids,
                        revisions: &mut self.revision_ids,
                    },
                    &at.node.to_string(),
                    at.offset,
                    author,
                    Some(held.date.clone()),
                )?;
                (lowered.operations, lowered.caret)
            }
        };
        self.turn_commit(turn, &ops)?;
        let held = self.turn_mut(turn)?;
        held.anchor = Some(caret);
        held.continuing = None;
        Ok(ops)
    }

    pub(crate) fn ai_turn_write_inner(
        &mut self,
        turn: u32,
        text: &str,
    ) -> Result<EditResult, String> {
        self.writing(turn)?;
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut all = Vec::new();
        for (index, piece) in text.split('\n').enumerate() {
            if index > 0 {
                all.extend(self.turn_break(turn)?);
            }
            if !piece.is_empty() {
                all.extend(self.turn_write_piece(turn, piece)?);
            }
        }
        let caret = self
            .turn(turn)?
            .anchor
            .unwrap_or_else(|| Pos::new(self.document.id(), 0));
        Ok(self.turn_result(caret, &all))
    }

    pub(crate) fn ai_turn_replace_inner(
        &mut self,
        turn: u32,
        node: &str,
        start: u32,
        end: u32,
    ) -> Result<EditResult, String> {
        let held = self.writing(turn)?.clone();
        if start >= end {
            return Err("a replacement needs a range".to_owned());
        }
        let nid = NodeId::from_str(node).map_err(|_| "invalid node".to_owned())?;
        let (ops, anchor) = match held.mode {
            TurnMode::Direct => (
                vec![Operation::DeleteText {
                    range: casual_doc_edit::Range {
                        start: Pos::new(nid, start),
                        end: Pos::new(nid, end),
                    },
                }],
                Pos::new(nid, start),
            ),
            TurnMode::Suggest => {
                let author = self.turn_author();
                let lowered = review_lower::suggest_delete(
                    &self.document,
                    &mut ReviewIds {
                        nodes: &mut self.edit_ids,
                        revisions: &mut self.revision_ids,
                    },
                    node,
                    start,
                    end,
                    author,
                    Some(held.date.clone()),
                )?;
                // The lowering's own caret: where the editor's own Suggesting type-over
                // continues, in the editor's position space (the struck text occupies
                // none of it, so the replacement lands beside it).
                (lowered.operations, lowered.caret)
            }
        };
        self.turn_commit(turn, &ops)?;
        let held = self.turn_mut(turn)?;
        held.anchor = Some(anchor);
        held.continuing = None;
        Ok(self.turn_result(anchor, &ops))
    }

    pub(crate) fn ai_turn_delete_range_inner(
        &mut self,
        turn: u32,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> Result<EditResult, String> {
        let held = self.writing(turn)?.clone();
        let (start, end) = self.order_endpoints(start_node, start_offset, end_node, end_offset)?;
        if start == end {
            return Err("a deletion needs a range".to_owned());
        }
        let ops = match held.mode {
            TurnMode::Direct => self.selection_delete_ops_ordered(start, end)?,
            TurnMode::Suggest => {
                let author = self.turn_author();
                let ids = &mut ReviewIds {
                    nodes: &mut self.edit_ids,
                    revisions: &mut self.revision_ids,
                };
                if start.node == end.node {
                    review_lower::suggest_delete(
                        &self.document,
                        ids,
                        &start.node.to_string(),
                        start.offset,
                        end.offset,
                        author,
                        Some(held.date.clone()),
                    )?
                    .operations
                } else {
                    let nodes = self.paragraphs_in_selection(start, end);
                    review_lower::suggest_range_edit(
                        &self.document,
                        &mut ReviewIds {
                            nodes: &mut self.edit_ids,
                            revisions: &mut self.revision_ids,
                        },
                        start,
                        end,
                        &nodes,
                        None,
                        author,
                        Some(held.date.clone()),
                    )?
                    .operations
                }
            }
        };
        if ops.is_empty() {
            return Ok(self.finish_edit(start));
        }
        self.turn_commit(turn, &ops)?;
        let held = self.turn_mut(turn)?;
        held.anchor = Some(start);
        held.continuing = None;
        Ok(self.turn_result(start, &ops))
    }

    /// The body index just after the top-level block that is, or holds, `block`.
    fn index_after_top_level(&self, block: NodeId) -> Option<usize> {
        fn holds(node: &BlockNode, id: NodeId) -> bool {
            match node {
                BlockNode::Paragraph(paragraph) => paragraph.id == id,
                BlockNode::Table(table) => {
                    table.id == id
                        || table.rows.iter().any(|row| {
                            row.cells
                                .iter()
                                .any(|cell| cell.blocks.iter().any(|inner| holds(inner, id)))
                        })
                }
                BlockNode::Sdt(sdt) => sdt.blocks.iter().any(|inner| holds(inner, id)),
                BlockNode::AltChunk(_) => false,
            }
        }
        self.document
            .body()
            .iter()
            .position(|node| holds(node, block))
            .map(|index| index + 1)
    }

    pub(crate) fn ai_turn_paragraph_after_inner(
        &mut self,
        turn: u32,
        block: &str,
    ) -> Result<EditResult, String> {
        let held = self.writing(turn)?.clone();
        let block = NodeId::from_str(block).map_err(|_| "invalid block".to_owned())?;
        let index = self
            .index_after_top_level(block)
            .ok_or_else(|| "block not found".to_owned())?;
        let id = self
            .edit_ids
            .next_id()
            .map_err(|_| "id space exhausted".to_owned())?;
        let mut properties = ParagraphProperties::default();
        if held.mode == TurnMode::Suggest {
            properties.mark_revision = Some(Box::new(MarkRevision {
                kind: MarkRevisionKind::Insertion,
                author: self.turn_author(),
                date: Some(held.date.clone()),
                revision_id: Some(self.revision_ids.allocate()?),
            }));
        }
        let ops = vec![Operation::InsertBlocks {
            container: None,
            index: u32::try_from(index).map_err(|_| "the body is too long".to_owned())?,
            blocks: vec![BlockNode::Paragraph(Paragraph {
                id,
                properties: properties.into(),
                inlines: Vec::new(),
            })],
        }];
        self.turn_commit(turn, &ops)?;
        let anchor = Pos::new(id, 0);
        let held = self.turn_mut(turn)?;
        held.anchor = Some(anchor);
        held.continuing = None;
        Ok(self.turn_result(anchor, &ops))
    }

    pub(crate) fn ai_turn_insert_table_inner(
        &mut self,
        turn: u32,
        after: &str,
        rows: u32,
        columns: u32,
        header: bool,
    ) -> Result<EditResult, String> {
        let held = self.writing(turn)?.clone();
        let nid = NodeId::from_str(after).map_err(|_| "invalid node".to_owned())?;
        let (container, index, width) = self.table_insertion_point(nid);
        let ids = &mut self.edit_ids;
        let edit = casual_doc_command::table::insert_table_in(
            container,
            index,
            rows,
            columns,
            width,
            &mut || ids.next_id().ok(),
        )?;
        let mut ops = match held.mode {
            TurnMode::Direct => edit.operations,
            TurnMode::Suggest => {
                let author = self.turn_author();
                table_review::suggest_insert_table(
                    edit.operations,
                    edit.caret,
                    &mut ReviewIds {
                        nodes: &mut self.edit_ids,
                        revisions: &mut self.revision_ids,
                    },
                    &[],
                    header,
                    author,
                    Some(held.date.clone()),
                )?
                .operations
            }
        };
        if held.mode == TurnMode::Direct && header {
            casual_doc_command::table::mark_header_row(&mut ops);
        }
        let table = ops.iter().find_map(|op| match op {
            Operation::InsertTable { table, .. } => Some(table.id),
            _ => None,
        });
        self.turn_commit(turn, &ops)?;
        let caret = edit.caret.map_or(Pos::new(nid, 0), |p| Pos::new(p, 0));
        let held = self.turn_mut(turn)?;
        held.table = table;
        held.anchor = Some(caret);
        held.continuing = None;
        Ok(self.turn_result(caret, &ops))
    }

    pub(crate) fn ai_turn_table_cells_value(&self, turn: u32) -> Result<serde_json::Value, String> {
        let held = self.turn(turn)?;
        let Some(id) = held.table else {
            return Ok(json!({ "table": null, "cells": [] }));
        };
        let table = casual_doc_edit::find_table(&self.document, id)
            .ok_or_else(|| "The table Casual AI inserted is no longer there.".to_owned())?;
        let cells: Vec<Vec<Option<String>>> = table
            .rows
            .iter()
            .map(|row| {
                row.cells
                    .iter()
                    .map(|cell| {
                        cell.blocks.iter().find_map(|block| match block {
                            BlockNode::Paragraph(paragraph) => Some(paragraph.id.to_string()),
                            _ => None,
                        })
                    })
                    .collect()
            })
            .collect();
        Ok(json!({ "table": id.to_string(), "cells": cells }))
    }

    pub(crate) fn ai_turn_remove_table_inner(
        &mut self,
        turn: u32,
        table: &str,
    ) -> Result<EditResult, String> {
        let held = self.writing(turn)?.clone();
        let id = NodeId::from_str(table).map_err(|_| "invalid table".to_owned())?;
        let ops = match held.mode {
            TurnMode::Direct => {
                if casual_doc_edit::find_table(&self.document, id).is_none() {
                    return Err("table not found".to_owned());
                }
                casual_doc_command::table::delete_table(id)
            }
            TurnMode::Suggest => {
                let author = self.turn_author();
                table_review::suggest_delete_table(
                    &self.document,
                    &mut ReviewIds {
                        nodes: &mut self.edit_ids,
                        revisions: &mut self.revision_ids,
                    },
                    id,
                    author,
                    Some(held.date.clone()),
                )?
                .operations
            }
        };
        self.turn_commit(turn, &ops)?;
        let caret = self
            .turn(turn)?
            .anchor
            .unwrap_or_else(|| Pos::new(self.document.id(), 0));
        self.turn_mut(turn)?.continuing = None;
        Ok(self.turn_result(caret, &ops))
    }

    pub(crate) fn ai_turn_comment_inner(
        &mut self,
        turn: u32,
        node: &str,
        start: u32,
        end: u32,
        text: &str,
    ) -> Result<EditResult, String> {
        let held = self.writing(turn)?.clone();
        let author = self.turn_author();
        let lowered = review_lower::add_comment(
            &self.document,
            &mut ReviewIds {
                nodes: &mut self.edit_ids,
                revisions: &mut self.revision_ids,
            },
            node,
            start,
            node,
            end,
            text,
            author,
            Some("AI".to_owned()),
            Some(held.date),
        )?;
        self.turn_commit(turn, &lowered.operations)?;
        let caret = self.turn(turn)?.anchor.unwrap_or(lowered.caret);
        Ok(self.turn_result(caret, &lowered.operations))
    }

    /// The review items this turn authored: the turn's author, its date, and a revision
    /// serial one of its own commits allocated. O(document): the revision list.
    fn turn_revision_items(&self, held: &AiTurn) -> Vec<(String, Option<String>)> {
        let author = self.turn_author();
        build_review_revision_summaries(&self.document)
            .into_iter()
            .filter(|item| {
                item.author == author
                    && item.date.as_deref() == Some(held.date.as_str())
                    && item
                        .revision_id
                        .as_deref()
                        .is_some_and(|serial| held.owns_serial(serial))
            })
            .map(|item| (item.id, item.group_id))
            .collect()
    }

    pub(crate) fn ai_turn_end_value(&mut self, turn: u32) -> Result<serde_json::Value, String> {
        let held = {
            let held = self.turn_mut(turn)?;
            held.open = false;
            held.clone()
        };
        let revisions: Vec<String> = self
            .turn_revision_items(&held)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        Ok(json!({
            "turn": held.id,
            "mode": mode_name(held.mode),
            "commits": held.commits,
            "group": held.group.get(),
            "revisions": revisions,
        }))
    }

    pub(crate) fn ai_turn_state_value(&self, turn: u32) -> Result<serde_json::Value, String> {
        let held = self.turn(turn)?;
        let anchor = self
            .mapped_anchor(held)
            .ok()
            .map(|at| json!({ "node": at.node.to_string(), "offset": at.offset }));
        Ok(json!({
            "open": held.open,
            "mode": mode_name(held.mode),
            "group": held.group.get(),
            "commits": held.commits,
            "anchor": anchor,
        }))
    }

    pub(crate) fn ai_turn_anchor_value(&self, turn: u32) -> Result<serde_json::Value, String> {
        let held = self.turn(turn)?;
        let at = self.mapped_anchor(held)?;
        Ok(json!({ "node": at.node.to_string(), "offset": at.offset }))
    }

    /// Reverts every effect of the turn as ONE step, rebased over the person's interleaved
    /// edits. `forget`: the step and the turn leave Undo altogether (a safety rollback).
    pub(crate) fn ai_turn_revert_inner(
        &mut self,
        turn: u32,
        forget: bool,
    ) -> Result<EditResult, String> {
        let held = self.turn(turn)?.clone();
        let revert = self.log.open_turn_group();
        let mut all = Vec::new();
        let mut caret = None;
        for group in held.groups.iter().rev() {
            let ops = self.selective_history_step(*group)?;
            if ops.is_empty() {
                continue;
            }
            caret = Some(self.apply_group(
                &ops,
                AI_TURN_LABEL,
                Coalesce::Join(revert),
                Origin::Edit,
            )?);
            all.extend(ops);
        }
        if forget {
            for group in &held.groups {
                self.log.retire(*group);
            }
            self.log.retire(revert);
        }
        if let Some(open) = self.ai_turn.as_mut().filter(|open| open.id == turn) {
            open.open = false;
        }
        self.ai_turn = None;
        match caret {
            Some(caret) => Ok(self.turn_result(caret, &all)),
            None => Ok(self.finish_edit(Pos::new(self.document.id(), 0))),
        }
    }

    /// Accepts every revision the turn made, as ONE undo step. Direct mode: nothing to
    /// decide. O(document) for the revision list, once per decision.
    pub(crate) fn ai_turn_accept_inner(&mut self, turn: u32) -> Result<EditResult, String> {
        let held = self.turn(turn)?.clone();
        if held.mode == TurnMode::Direct {
            self.ai_turn = None;
            return Ok(self.finish_edit(Pos::new(self.document.id(), 0)));
        }
        let decision = self.log.open_turn_group();
        let mut all = Vec::new();
        let mut caret = None;
        let mut decided_groups: Vec<String> = Vec::new();
        // Each decision is lowered against the document as the previous one left it.
        for (id, group) in self.turn_revision_items(&held) {
            let lowered = match &group {
                Some(group) if decided_groups.contains(group) => continue,
                Some(group) => {
                    decided_groups.push(group.clone());
                    review_lower::decide_revision_group(&self.document, group, true)
                }
                None => review_lower::decide_revision(&self.document, &id, true),
            };
            let Ok(lowered) = lowered else {
                // Already decided with something else (a paragraph mark joined away).
                continue;
            };
            if lowered.operations.is_empty() {
                continue;
            }
            self.apply_group(
                &lowered.operations,
                AI_TURN_LABEL,
                Coalesce::Join(decision),
                Origin::Edit,
            )?;
            caret = Some(lowered.caret);
            all.extend(lowered.operations);
        }
        self.ai_turn = None;
        let caret = caret.unwrap_or_else(|| Pos::new(self.document.id(), 0));
        Ok(self.turn_result(caret, &all))
    }
}

const fn mode_name(mode: TurnMode) -> &'static str {
    match mode {
        TurnMode::Direct => "direct",
        TurnMode::Suggest => "suggest",
    }
}

#[wasm_bindgen]
impl WasmDocument {
    /// Opens a Casual AI turn (`docs/183` §23): `mode` is `"direct"` or `"suggest"` (the
    /// host's EFFECTIVE mode), `date` the suggestions' ISO date. Ends any open turn. Returns
    /// the turn id. O(1).
    #[wasm_bindgen(js_name = aiTurnBegin)]
    pub fn ai_turn_begin(&mut self, mode: &str, date: &str) -> Result<u32, JsValue> {
        self.ai_turn_begin_inner(mode, date).map_err(to_js)
    }

    /// Sets where the turn writes next (no edit). O(1) plus the paragraph lookup.
    #[wasm_bindgen(js_name = aiTurnSetAnchor)]
    pub fn ai_turn_set_anchor(
        &mut self,
        turn: u32,
        node: &str,
        offset: u32,
    ) -> Result<(), JsValue> {
        self.ai_turn_set_anchor_inner(turn, node, offset)
            .map_err(to_js)
    }

    /// The insertion point mapped to now: `{"node","offset"}` JSON. O(commits since).
    #[wasm_bindgen(js_name = aiTurnAnchor)]
    pub fn ai_turn_anchor(&self, turn: u32) -> Result<String, JsValue> {
        self.ai_turn_anchor_value(turn)
            .map(|value| value.to_string())
            .map_err(to_js)
    }

    /// Writes `text` at the turn's anchor (`\n` = paragraph break) and advances it.
    /// The result's caret is the AI's new insertion point, not the person's.
    #[wasm_bindgen(js_name = aiTurnWrite)]
    pub fn ai_turn_write(&mut self, turn: u32, text: &str) -> Result<EditResult, JsValue> {
        self.ai_turn_write_inner(turn, text).map_err(to_js)
    }

    /// Removes `[start, end)` of paragraph `node` before a replacement is written: a
    /// tracked deletion, the anchor where the editor's own Suggesting type-over continues
    /// (the replacement then shows beside the struck text), or a direct one (the anchor at
    /// `start`).
    #[wasm_bindgen(js_name = aiTurnReplace)]
    pub fn ai_turn_replace(
        &mut self,
        turn: u32,
        node: &str,
        start: u32,
        end: u32,
    ) -> Result<EditResult, JsValue> {
        self.ai_turn_replace_inner(turn, node, start, end)
            .map_err(to_js)
    }

    /// Deletes a range that may cross paragraphs; the anchor goes to its start.
    #[wasm_bindgen(js_name = aiTurnDeleteRange)]
    pub fn ai_turn_delete_range(
        &mut self,
        turn: u32,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> Result<EditResult, JsValue> {
        self.ai_turn_delete_range_inner(turn, start_node, start_offset, end_node, end_offset)
            .map_err(to_js)
    }

    /// Inserts an empty paragraph after the top-level block holding `block` (a paragraph or
    /// a table id); the anchor goes to it.
    #[wasm_bindgen(js_name = aiTurnParagraphAfter)]
    pub fn ai_turn_paragraph_after(
        &mut self,
        turn: u32,
        block: &str,
    ) -> Result<EditResult, JsValue> {
        self.ai_turn_paragraph_after_inner(turn, block)
            .map_err(to_js)
    }

    /// Inserts an empty `rows` x `columns` table after paragraph `after` (tracked in
    /// suggest mode); `aiTurnTableCells` names its cells.
    #[wasm_bindgen(js_name = aiTurnInsertTable)]
    pub fn ai_turn_insert_table(
        &mut self,
        turn: u32,
        after: &str,
        rows: u32,
        columns: u32,
        header: bool,
    ) -> Result<EditResult, JsValue> {
        self.ai_turn_insert_table_inner(turn, after, rows, columns, header)
            .map_err(to_js)
    }

    /// `{"table": id | null, "cells": [[paragraphId | null, …], …]}` for the turn's table.
    #[wasm_bindgen(js_name = aiTurnTableCells)]
    pub fn ai_turn_table_cells(&self, turn: u32) -> Result<String, JsValue> {
        self.ai_turn_table_cells_value(turn)
            .map(|value| value.to_string())
            .map_err(to_js)
    }

    /// Removes `table`: a tracked whole-table deletion (suggest) or `DeleteTable` (direct).
    #[wasm_bindgen(js_name = aiTurnRemoveTable)]
    pub fn ai_turn_remove_table(&mut self, turn: u32, table: &str) -> Result<EditResult, JsValue> {
        self.ai_turn_remove_table_inner(turn, table).map_err(to_js)
    }

    /// A comment authored "Casual AI" on `[start, end)` of paragraph `node`, in the turn.
    #[wasm_bindgen(js_name = aiTurnComment)]
    pub fn ai_turn_comment(
        &mut self,
        turn: u32,
        node: &str,
        start: u32,
        end: u32,
        text: &str,
    ) -> Result<EditResult, JsValue> {
        self.ai_turn_comment_inner(turn, node, start, end, text)
            .map_err(to_js)
    }

    /// Closes the turn's writing: `{"turn","mode","commits","group","revisions":[ids]}`.
    /// The turn stays decidable. O(document) for the revision list.
    #[wasm_bindgen(js_name = aiTurnEnd)]
    pub fn ai_turn_end(&mut self, turn: u32) -> Result<String, JsValue> {
        self.ai_turn_end_value(turn)
            .map(|value| value.to_string())
            .map_err(to_js)
    }

    /// `{"open","mode","group","commits","anchor": {"node","offset"} | null}`.
    #[wasm_bindgen(js_name = aiTurnState)]
    pub fn ai_turn_state(&self, turn: u32) -> Result<String, JsValue> {
        self.ai_turn_state_value(turn)
            .map(|value| value.to_string())
            .map_err(to_js)
    }

    /// Reverts the whole turn as one step, sparing the person's own edits. `forget`: a
    /// safety rollback that leaves no undo entry for the turn or the revert.
    #[wasm_bindgen(js_name = aiTurnRevert)]
    pub fn ai_turn_revert(&mut self, turn: u32, forget: bool) -> Result<EditResult, JsValue> {
        self.ai_turn_revert_inner(turn, forget).map_err(to_js)
    }

    /// Accepts every suggestion the turn made, as one undo step (direct: no edit).
    #[wasm_bindgen(js_name = aiTurnAccept)]
    pub fn ai_turn_accept(&mut self, turn: u32) -> Result<EditResult, JsValue> {
        self.ai_turn_accept_inner(turn).map_err(to_js)
    }
}

#[cfg(test)]
#[path = "ai_turn_tests.rs"]
mod tests;
