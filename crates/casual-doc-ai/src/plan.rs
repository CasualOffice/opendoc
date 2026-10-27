// SPDX-License-Identifier: Apache-2.0

//! Edit plans: a closed, versioned vocabulary compiled to the closed op set
//! (`docs/178` §4.3).
//!
//! A plan applies all or nothing as **one** undo step: the session's state is
//! cloned (copy-on-write), every edit is resolved, lowered and committed to the
//! clone through the session's one choke point, and the clone replaces the
//! session only when every edit succeeded. `preview` runs the very same code
//! and throws the clone away, so a preview cannot disagree with an apply.
//!
//! Suggestions are lowered by `casual_doc_review::lower`, the code the editor's
//! own Suggesting mode calls, so an agent's tracked change is a person's
//! tracked change. Direct edits are plain operations. Either way the engine
//! judges every operation against the session's capabilities.

use std::collections::BTreeSet;

use casual_doc_edit::{FormatDelta, Operation, Pos, Range};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, HighlightColor, Paragraph, RgbColor, StyleId, StyleKind};
use casual_doc_review::ReviewIds;
use casual_doc_review::body::build_review_revision_summaries;
use casual_doc_review::lower::{self, InsertStyle, Lowered};
use casual_doc_transaction::{Coalesce, Origin};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::anchor::{Anchor, Side, paragraph, resolve};
use crate::error::{AiError, AiResult};
use crate::profile::Profile;
use crate::read::node_id;
use crate::session::{ChangeRecord, Session, State};
use crate::structure::find_table;
use crate::text::{clip, paragraph_text, runs};

/// The vocabulary's version. A new edit kind is an entry in the capability
/// registry (`casual_doc_command::COMMANDS`, `docs/181`), with its lowering,
/// or not at all.
pub const PLAN_VERSION: u32 = 1;

/// Whether edits become tracked suggestions or land directly.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Tracked changes authored by the session's identity. The default.
    #[default]
    Suggest,
    /// Untracked edits; only under a profile that grants `edit`.
    Direct,
}

/// A plan as an agent sends it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditPlan {
    /// The revision the plan was written against.
    pub base_revision: u64,
    /// The edits, applied in order.
    pub edits: Vec<Value>,
    /// Suggest (default) or direct.
    #[serde(default)]
    pub mode: Mode,
    /// A retry of a committed plan with the same key returns the original
    /// change instead of applying twice.
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// One new paragraph for `insert_paragraphs`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewParagraph {
    /// Its text.
    #[serde(default)]
    pub text: String,
    /// A paragraph style, by name (for example `Heading 2`) or id.
    #[serde(default)]
    pub style: Option<String>,
}

/// Character formatting an agent may set.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextFormat {
    /// Bold on or off.
    #[serde(default)]
    pub bold: Option<bool>,
    /// Italic on or off.
    #[serde(default)]
    pub italic: Option<bool>,
    /// Underline on or off.
    #[serde(default)]
    pub underline: Option<bool>,
    /// Strikethrough on or off.
    #[serde(default)]
    pub strike: Option<bool>,
    /// Size in points (half-point precision).
    #[serde(default)]
    pub size: Option<f32>,
    /// Colour, `#RRGGBB`.
    #[serde(default)]
    pub color: Option<String>,
    /// Highlight name (`yellow`, `green`, `cyan`, `magenta`, `blue`, `red`, …, `none`).
    #[serde(default)]
    pub highlight: Option<String>,
    /// Font family.
    #[serde(default)]
    pub font: Option<String>,
}

/// Accept or reject.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    /// Keep the change.
    Accept,
    /// Undo the change.
    Reject,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Edit {
    InsertText {
        at: Anchor,
        text: String,
    },
    ReplaceText {
        target: Anchor,
        text: String,
    },
    DeleteText {
        target: Anchor,
    },
    InsertParagraphs {
        after: Anchor,
        paragraphs: Vec<NewParagraph>,
    },
    ApplyParagraphStyle {
        target: Anchor,
        style: String,
    },
    SetTextFormat {
        target: Anchor,
        format: TextFormat,
    },
    SetCellText {
        table: String,
        row: u32,
        column: u32,
        text: String,
    },
    TableInsertRow {
        table: String,
        index: u32,
    },
    TableDeleteRow {
        table: String,
        index: u32,
    },
    Comment {
        target: Anchor,
        text: String,
        #[serde(default)]
        reply_to: Option<String>,
    },
    ResolveComment {
        id: String,
        #[serde(default)]
        resolved: Option<bool>,
    },
    Review {
        decision: Decision,
        #[serde(default)]
        revisions: Vec<String>,
        #[serde(default)]
        all: bool,
    },
}

impl Edit {
    fn kind(&self) -> &'static str {
        match self {
            Self::InsertText { .. } => "insert_text",
            Self::ReplaceText { .. } => "replace_text",
            Self::DeleteText { .. } => "delete_text",
            Self::InsertParagraphs { .. } => "insert_paragraphs",
            Self::ApplyParagraphStyle { .. } => "apply_paragraph_style",
            Self::SetTextFormat { .. } => "set_text_format",
            Self::SetCellText { .. } => "set_cell_text",
            Self::TableInsertRow { .. } => "table_insert_row",
            Self::TableDeleteRow { .. } => "table_delete_row",
            Self::Comment { .. } => "comment",
            Self::ResolveComment { .. } => "resolve_comment",
            Self::Review { .. } => "review",
        }
    }

    /// The profile this edit needs in `mode` (`docs/178` §4.3's table).
    fn needs(&self, mode: Mode) -> Profile {
        match self {
            Self::Comment { .. } | Self::ResolveComment { .. } => Profile::Comment,
            Self::Review { .. } => Profile::Review,
            Self::TableInsertRow { .. } | Self::TableDeleteRow { .. } => Profile::Edit,
            _ => match mode {
                Mode::Suggest => Profile::Suggest,
                Mode::Direct => Profile::Edit,
            },
        }
    }

    fn inserted_bytes(&self) -> usize {
        match self {
            Self::InsertText { text, .. }
            | Self::ReplaceText { text, .. }
            | Self::SetCellText { text, .. }
            | Self::Comment { text, .. } => text.len(),
            Self::InsertParagraphs { paragraphs, .. } => paragraphs
                .iter()
                .map(|paragraph| paragraph.text.len())
                .sum(),
            _ => 0,
        }
    }
}

/// Parses and bounds a plan. **O(edits)**.
fn parse(session: &Session, plan: &EditPlan) -> AiResult<Vec<Edit>> {
    let limits = &session.config().limits;
    if plan.edits.is_empty() {
        return Err(AiError::invalid("A plan needs at least one edit."));
    }
    if plan.edits.len() > limits.max_plan_edits {
        return Err(AiError::new(
            "over-limit",
            format!(
                "A plan holds at most {} edits; split it into several plans.",
                limits.max_plan_edits
            ),
        ));
    }
    let mut edits = Vec::with_capacity(plan.edits.len());
    let mut bytes = 0usize;
    for (index, value) in plan.edits.iter().enumerate() {
        let kind = value.get("op").and_then(Value::as_str).unwrap_or("");
        // The registry is the vocabulary: anything it does not name is
        // `unknown-edit`, never ignored.
        if casual_doc_command::find(kind).is_none() {
            return Err(AiError::new(
                "unknown-edit",
                format!(
                    "Edit {index} has op `{}`, which is not in the edit vocabulary (version \
                     {PLAN_VERSION}).",
                    clip(kind, 40)
                ),
            )
            .with_details(json!({ "index": index, "known": casual_doc_command::ids() })));
        }
        let edit: Edit = serde_json::from_value(value.clone()).map_err(|error| {
            AiError::invalid(format!("Edit {index} (`{kind}`) is malformed: {error}."))
                .with_details(json!({ "index": index }))
        })?;
        bytes = bytes.saturating_add(edit.inserted_bytes());
        edits.push(edit);
    }
    if bytes > limits.max_plan_text_bytes {
        return Err(AiError::new(
            "over-limit",
            "A plan inserts at most 1 MiB of text; split it into several plans.",
        ));
    }
    Ok(edits)
}

/// One edit's outcome.
#[derive(Clone, Debug)]
struct Outcome {
    index: usize,
    kind: &'static str,
    nodes: Vec<NodeId>,
    operations: usize,
    error: Option<AiError>,
}

fn color(value: &str) -> AiResult<RgbColor> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AiError::invalid("A colour is `#RRGGBB`."));
    }
    let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).unwrap_or(0);
    Ok(RgbColor {
        r: channel(0),
        g: channel(2),
        b: channel(4),
    })
}

fn highlight(value: &str) -> AiResult<HighlightColor> {
    Ok(match value.to_ascii_lowercase().as_str() {
        "none" => HighlightColor::None,
        "black" => HighlightColor::Black,
        "blue" => HighlightColor::Blue,
        "cyan" => HighlightColor::Cyan,
        "green" => HighlightColor::Green,
        "magenta" => HighlightColor::Magenta,
        "red" => HighlightColor::Red,
        "yellow" => HighlightColor::Yellow,
        "white" => HighlightColor::White,
        "darkblue" => HighlightColor::DarkBlue,
        "darkcyan" => HighlightColor::DarkCyan,
        "darkgray" => HighlightColor::DarkGray,
        "darkgreen" => HighlightColor::DarkGreen,
        "darkmagenta" => HighlightColor::DarkMagenta,
        "darkred" => HighlightColor::DarkRed,
        "darkyellow" => HighlightColor::DarkYellow,
        "lightgray" => HighlightColor::LightGray,
        _ => {
            return Err(AiError::invalid(
                "That highlight colour is not one Word has.",
            ));
        }
    })
}

fn delta(format: &TextFormat) -> AiResult<FormatDelta> {
    let delta = FormatDelta {
        bold: format.bold,
        italic: format.italic,
        underline: format.underline,
        strike: format.strike,
        size_half_points: match format.size {
            Some(points) if (1.0..=1638.0).contains(&points) => Some((points * 2.0).round() as u32),
            Some(_) => return Err(AiError::invalid("A size is between 1 and 1638 points.")),
            None => None,
        },
        color: format.color.as_deref().map(color).transpose()?,
        highlight: format.highlight.as_deref().map(highlight).transpose()?,
        font: format.font.clone(),
        ..FormatDelta::default()
    };
    if delta == FormatDelta::default() {
        return Err(AiError::invalid("`format` sets nothing."));
    }
    Ok(delta)
}

fn style_id(state: &State, style: &str) -> AiResult<StyleId> {
    let styles = &state.document.definitions().styles;
    let by_id = style.parse::<NodeId>().ok().map(StyleId::new);
    for (id, definition) in styles.iter() {
        if definition.kind != StyleKind::Paragraph {
            continue;
        }
        if Some(*id) == by_id
            || definition
                .name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(style))
        {
            return Ok(*id);
        }
    }
    Err(AiError::invalid(format!(
        "No paragraph style is named `{}`; get_styles lists them.",
        clip(style, 80)
    )))
}

/// Refuses `kind` in suggest mode when the registry says it cannot be
/// tracked, with the registry's own reason. O(commands).
fn refuse_untracked(kind: &str, mode: Mode) -> AiResult<()> {
    match casual_doc_command::find(kind).map(|command| command.trackable) {
        Some(casual_doc_command::Trackable::No(reason)) if mode != Mode::Direct => {
            Err(AiError::new("not-trackable", reason))
        }
        _ => Ok(()),
    }
}

/// Applies `edit` to `state`, committing each lowered step with `coalesce`
/// (the first step of a plan opens the undo group, the rest continue it).
fn apply_edit(
    session: &Session,
    state: &mut State,
    edit: &Edit,
    mode: Mode,
    first: &mut bool,
    nodes: &mut Vec<NodeId>,
) -> AiResult<usize> {
    let capabilities = session.config().profile.capabilities();
    let author = Some(session.config().author.clone());
    let date = Some(session.date());
    let mut operations = 0usize;
    let kind = edit.kind();
    let declared = casual_doc_command::find(kind).map_or(&[][..], |command| command.emits);
    let mut commit = |state: &mut State, ops: Vec<Operation>| -> AiResult<()> {
        // `docs/181` §3: a command emits only what the registry says it does,
        // so the registry's coverage claims are true of the code. O(ops).
        if let Some(op) = ops
            .iter()
            .find(|op| !declared.contains(&casual_doc_command::coverage::operation_name(op)))
        {
            return Err(AiError::engine(format!(
                "`{kind}` lowered to `{}`, which the capability registry does not declare for \
                 it.",
                casual_doc_command::coverage::operation_name(op)
            )));
        }
        let coalesce = if *first {
            Coalesce::New
        } else {
            Coalesce::Continue
        };
        operations += ops.len();
        state.commit(ops, capabilities, coalesce, Origin::Edit)?;
        *first = false;
        Ok(())
    };
    macro_rules! ids {
        ($state:expr) => {
            &mut ReviewIds {
                nodes: &mut $state.ids,
                revisions: &mut $state.revisions,
            }
        };
    }
    let lowered = |result: Result<Lowered, String>| result.map_err(AiError::engine);
    match edit {
        Edit::InsertText { at, text } => {
            if text.is_empty() {
                return Err(AiError::invalid("`text` is empty."));
            }
            let target = resolve(&state.document, at)?;
            let offset = target.point(at.side.unwrap_or(Side::After));
            nodes.push(target.node);
            let ops = match mode {
                Mode::Suggest => {
                    lowered(lower::suggest_insert(
                        &state.document,
                        ids!(state),
                        &target.node.to_string(),
                        offset,
                        text,
                        InsertStyle::Plain,
                        author,
                        date,
                        None,
                    ))?
                    .operations
                }
                Mode::Direct => vec![Operation::InsertText {
                    at: Pos::new(target.node, offset),
                    text: text.clone(),
                }],
            };
            commit(state, ops)?;
        }
        Edit::ReplaceText { target, text } => {
            let resolved = resolve(&state.document, target)?;
            nodes.push(resolved.node);
            if resolved.start == resolved.end {
                return Err(AiError::invalid("The target is empty; use insert_text."));
            }
            if text.is_empty() {
                return Err(AiError::invalid("`text` is empty; use delete_text."));
            }
            let ops = match mode {
                Mode::Suggest => {
                    lowered(lower::suggest_replace(
                        &state.document,
                        ids!(state),
                        &resolved.node.to_string(),
                        resolved.start,
                        resolved.end,
                        text,
                        author,
                        date,
                    ))?
                    .operations
                }
                Mode::Direct => vec![
                    Operation::DeleteText {
                        range: Range {
                            start: Pos::new(resolved.node, resolved.start),
                            end: Pos::new(resolved.node, resolved.end),
                        },
                    },
                    Operation::InsertText {
                        at: Pos::new(resolved.node, resolved.start),
                        text: text.clone(),
                    },
                ],
            };
            commit(state, ops)?;
        }
        Edit::DeleteText { target } => {
            let resolved = resolve(&state.document, target)?;
            nodes.push(resolved.node);
            if resolved.start == resolved.end {
                return Err(AiError::invalid("The target is empty."));
            }
            let ops = match mode {
                Mode::Suggest => {
                    lowered(lower::suggest_delete(
                        &state.document,
                        ids!(state),
                        &resolved.node.to_string(),
                        resolved.start,
                        resolved.end,
                        author,
                        date,
                    ))?
                    .operations
                }
                Mode::Direct => vec![Operation::DeleteText {
                    range: Range {
                        start: Pos::new(resolved.node, resolved.start),
                        end: Pos::new(resolved.node, resolved.end),
                    },
                }],
            };
            commit(state, ops)?;
        }
        Edit::InsertParagraphs { after, paragraphs } => {
            if paragraphs.is_empty() {
                return Err(AiError::invalid("`paragraphs` is empty."));
            }
            let mut current = paragraph(&state.document, &after.node)?.id;
            for new in paragraphs {
                let style = new
                    .style
                    .as_deref()
                    .map(|style| style_id(state, style))
                    .transpose()?;
                let end = find(state, current).map(|p| paragraph_text(p).len() as u32)?;
                let created = match mode {
                    Mode::Suggest => {
                        let split = lowered(lower::suggest_split(
                            &state.document,
                            ids!(state),
                            &current.to_string(),
                            end,
                            author.clone(),
                            date.clone(),
                        ))?;
                        let created = split.caret.node;
                        commit(state, split.operations)?;
                        created
                    }
                    Mode::Direct => {
                        let new_id = state.ids.next_id().map_err(|_| {
                            AiError::new("over-limit", "The id space is exhausted.")
                        })?;
                        commit(
                            state,
                            vec![Operation::SplitParagraph {
                                at: Pos::new(current, end),
                                new_id,
                                properties: None,
                            }],
                        )?;
                        new_id
                    }
                };
                nodes.push(created);
                if let Some(style) = style {
                    let currently = find(state, created)?.properties.get().clone();
                    let mut next = currently.clone();
                    next.style_ref = Some(style);
                    let next = match mode {
                        Mode::Suggest => lower::track_paragraph_change(
                            &mut state.revisions,
                            author.clone(),
                            date.clone(),
                            &currently,
                            next,
                        )
                        .map_err(AiError::engine)?,
                        Mode::Direct => next,
                    };
                    if next != currently {
                        commit(
                            state,
                            vec![Operation::SetParagraphProperties {
                                node: created,
                                properties: Box::new(next),
                            }],
                        )?;
                    }
                }
                if !new.text.is_empty() {
                    let ops = match mode {
                        Mode::Suggest => {
                            lowered(lower::suggest_insert(
                                &state.document,
                                ids!(state),
                                &created.to_string(),
                                0,
                                &new.text,
                                InsertStyle::Plain,
                                author.clone(),
                                date.clone(),
                                None,
                            ))?
                            .operations
                        }
                        Mode::Direct => vec![Operation::InsertText {
                            at: Pos::new(created, 0),
                            text: new.text.clone(),
                        }],
                    };
                    commit(state, ops)?;
                }
                current = created;
            }
        }
        Edit::ApplyParagraphStyle { target, style } => {
            let style = style_id(state, style)?;
            let found = paragraph(&state.document, &target.node)?;
            let node = found.id;
            nodes.push(node);
            let currently = found.properties.get().clone();
            let mut next = currently.clone();
            next.style_ref = Some(style);
            let next = match mode {
                Mode::Suggest => lower::track_paragraph_change(
                    &mut state.revisions,
                    author,
                    date,
                    &currently,
                    next,
                )
                .map_err(AiError::engine)?,
                Mode::Direct => next,
            };
            if next == currently {
                return Err(AiError::new(
                    "no-change",
                    "The paragraph already has that style.",
                ));
            }
            commit(
                state,
                vec![Operation::SetParagraphProperties {
                    node,
                    properties: Box::new(next),
                }],
            )?;
        }
        Edit::SetTextFormat { target, format } => {
            let delta = delta(format)?;
            let resolved = resolve(&state.document, target)?;
            nodes.push(resolved.node);
            if resolved.start == resolved.end {
                return Err(AiError::invalid("The target is empty."));
            }
            let ops = match mode {
                Mode::Suggest => {
                    lowered(lower::suggest_format(
                        &state.document,
                        ids!(state),
                        &resolved.node.to_string(),
                        resolved.start,
                        resolved.end,
                        delta,
                        author,
                        date,
                    ))?
                    .operations
                }
                Mode::Direct => vec![Operation::FormatText {
                    range: Range {
                        start: Pos::new(resolved.node, resolved.start),
                        end: Pos::new(resolved.node, resolved.end),
                    },
                    delta,
                }],
            };
            commit(state, ops)?;
        }
        Edit::SetCellText {
            table,
            row,
            column,
            text,
        } => {
            let cell_paragraph = {
                let table = find_table(&state.document, node_id(table)?).ok_or_else(|| {
                    AiError::new("anchor-not-found", "No table has that id at this revision.")
                })?;
                let cell = table
                    .rows
                    .get(*row as usize)
                    .and_then(|r| r.cells.get(*column as usize))
                    .ok_or_else(|| AiError::invalid("That table has no such cell."))?;
                cell.blocks
                    .iter()
                    .find_map(|block| match block {
                        BlockNode::Paragraph(paragraph) => Some(paragraph.id),
                        _ => None,
                    })
                    .ok_or_else(|| AiError::invalid("That cell holds no paragraph."))?
            };
            let anchor = Anchor {
                node: cell_paragraph.to_string(),
                ..Anchor::default()
            };
            let inner = if paragraph_text(find(state, cell_paragraph)?).is_empty() {
                Edit::InsertText {
                    at: anchor,
                    text: text.clone(),
                }
            } else if text.is_empty() {
                Edit::DeleteText { target: anchor }
            } else {
                Edit::ReplaceText {
                    target: anchor,
                    text: text.clone(),
                }
            };
            return apply_edit(session, state, &inner, mode, first, nodes)
                .map(|ops| ops + operations);
        }
        Edit::TableInsertRow { table, index } => {
            refuse_untracked(kind, mode)?;
            let found = find_table(&state.document, node_id(table)?)
                .ok_or_else(|| {
                    AiError::new("anchor-not-found", "No table has that id at this revision.")
                })?
                .clone();
            // The editor's own lowering: the merged-cell rule of `docs/171`
            // and the InsertRow/ReplaceTable choice (`docs/181` §0).
            let ids = &mut state.ids;
            let edit =
                casual_doc_command::table::insert_row_at(&found, *index as usize, &mut || {
                    ids.next_id().ok()
                })
                .map_err(AiError::invalid)?;
            commit(state, edit.operations)?;
        }
        Edit::TableDeleteRow { table, index } => {
            refuse_untracked(kind, mode)?;
            let found = find_table(&state.document, node_id(table)?)
                .ok_or_else(|| {
                    AiError::new("anchor-not-found", "No table has that id at this revision.")
                })?
                .clone();
            if *index as usize >= found.rows.len() {
                return Err(AiError::invalid(
                    "The row index is past the end of the table.",
                ));
            }
            let edit = casual_doc_command::table::delete_row(&found, *index as usize)
                .map_err(AiError::invalid)?;
            commit(state, edit.operations)?;
        }
        Edit::Comment {
            target,
            text,
            reply_to,
        } => {
            if text.is_empty() {
                return Err(AiError::invalid("`text` is empty."));
            }
            let initials = Some(session.initials());
            let result = match reply_to {
                Some(parent) => lower::reply_to_comment(
                    &state.document,
                    ids!(state),
                    parent,
                    text,
                    author,
                    initials,
                    date,
                ),
                None => {
                    let resolved = resolve(&state.document, target)?;
                    nodes.push(resolved.node);
                    if resolved.start == resolved.end {
                        return Err(AiError::invalid("A comment needs a non-empty target."));
                    }
                    let node = resolved.node.to_string();
                    lower::add_comment(
                        &state.document,
                        ids!(state),
                        &node,
                        resolved.start,
                        &node,
                        resolved.end,
                        text,
                        author,
                        initials,
                        date,
                    )
                }
            };
            commit(state, lowered(result)?.operations)?;
        }
        Edit::ResolveComment { id, resolved } => {
            let result = lower::set_comment_resolved(&state.document, id, resolved.unwrap_or(true));
            commit(state, lowered(result)?.operations)?;
        }
        Edit::Review {
            decision,
            revisions,
            all,
        } => {
            let accept = *decision == Decision::Accept;
            if *all {
                let result = lower::decide_all_revisions(&state.document, accept);
                commit(state, lowered(result)?.operations)?;
            } else {
                if revisions.is_empty() {
                    return Err(AiError::invalid(
                        "Name the revisions to decide, or set `all`.",
                    ));
                }
                // An editor-authored group (a replacement, a typing run) and a
                // tracked move are decided whole, as the editor decides them.
                let summaries = build_review_revision_summaries(&state.document);
                let mut done = BTreeSet::new();
                for id in revisions {
                    let summary = summaries.iter().find(|s| s.id == *id);
                    let (key, result) = match summary {
                        Some(s) if s.group_id.is_some() => {
                            let group = s.group_id.clone().unwrap_or_default();
                            (
                                format!("g{group}"),
                                Some((group.clone(), None::<(String, String)>)),
                            )
                        }
                        Some(s) if s.move_pair.is_some() => {
                            let pair = s.move_pair.clone().expect("checked");
                            (
                                format!("m{}", pair.from_start),
                                Some((String::new(), Some((pair.from_start, pair.to_start)))),
                            )
                        }
                        _ => (id.clone(), None),
                    };
                    if !done.insert(key) {
                        continue;
                    }
                    let lowered_result = match result {
                        Some((group, None)) => {
                            lower::decide_revision_group(&state.document, &group, accept)
                        }
                        Some((_, Some((from, to)))) => {
                            lower::decide_move_pair(&state.document, &from, &to, accept)
                        }
                        None => lower::decide_revision(&state.document, id, accept),
                    };
                    commit(state, lowered(lowered_result)?.operations)?;
                }
            }
        }
    }
    Ok(operations)
}

fn find(state: &State, node: NodeId) -> AiResult<&Paragraph> {
    casual_doc_edit::find_paragraph_any(&state.document, node).ok_or_else(|| {
        AiError::new(
            "anchor-not-found",
            "No paragraph has that id at this revision.",
        )
    })
}

/// Runs a plan on a clone of the session's state. Returns the clone and the
/// per-edit outcomes; the session itself is untouched.
fn run(session: &Session, plan: &EditPlan) -> AiResult<(State, Vec<Outcome>)> {
    if plan.base_revision != session.revision() {
        return Err(AiError::new(
            "stale-revision",
            "The document changed since this plan was written; read it again and re-plan.",
        )
        .with_details(json!({ "currentRevision": session.revision() })));
    }
    let edits = parse(session, plan)?;
    let mut state = session.state.clone();
    let mut outcomes = Vec::with_capacity(edits.len());
    let mut first = true;
    for (index, edit) in edits.iter().enumerate() {
        let mut nodes = Vec::new();
        // The adapter's declaration: an edit the profile does not grant is
        // refused here with the tool-level code. The ENGINE check inside
        // `State::commit` is what actually holds the line.
        let needed = edit.needs(plan.mode);
        let result = if session.config().profile.grants(needed) {
            apply_edit(session, &mut state, edit, plan.mode, &mut first, &mut nodes)
        } else {
            Err(AiError::new(
                "capability-withheld",
                format!(
                    "This session's profile is `{}`; `{}` in {} mode needs `{}`.",
                    session.config().profile.name(),
                    edit.kind(),
                    match plan.mode {
                        Mode::Suggest => "suggest",
                        Mode::Direct => "direct",
                    },
                    needed.name()
                ),
            )
            .with_details(json!({ "enforcedBy": "server" })))
        };
        let (operations, error) = match result {
            Ok(operations) => (operations, None),
            Err(error) => (0, Some(error)),
        };
        outcomes.push(Outcome {
            index,
            kind: edit.kind(),
            nodes,
            operations,
            error,
        });
    }
    Ok((state, outcomes))
}

fn redline(before: &Session, after: &State, outcomes: &[Outcome]) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for node in outcomes.iter().flat_map(|outcome| outcome.nodes.iter()) {
        if !seen.insert(*node) || out.len() >= 50 {
            continue;
        }
        let was = casual_doc_edit::find_paragraph_any(before.document(), *node)
            .map(|p| clip(&paragraph_text(p), 1_000).to_owned());
        let now = casual_doc_edit::find_paragraph_any(&after.document, *node);
        out.push(json!({
            "node": node.to_string(),
            "before": was,
            "after": now.map(|p| clip(&paragraph_text(p), 1_000).to_owned()),
            "afterRuns": now.map(|p| runs(p, 100)),
        }));
    }
    out
}

fn outcomes_json(outcomes: &[Outcome]) -> Vec<Value> {
    outcomes
        .iter()
        .map(|outcome| {
            json!({
                "index": outcome.index,
                "op": outcome.kind,
                "nodes": outcome.nodes.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "operations": outcome.operations,
                "refusal": outcome.error,
            })
        })
        .collect()
}

/// Dry-runs a plan: the same resolution, capability checks, lowering and
/// apply as [`apply`], on a copy-on-write clone, committing nothing.
/// **O(document)** for the clone plus O(affected blocks).
///
/// # Errors
///
/// `stale-revision`, `unknown-edit`, `invalid-argument`, `over-limit` for the
/// plan as a whole; per-edit refusals are reported in the result.
pub fn preview(session: &Session, plan: &EditPlan) -> AiResult<Value> {
    let (state, outcomes) = run(session, plan)?;
    let refused = outcomes
        .iter()
        .filter(|outcome| outcome.error.is_some())
        .count();
    Ok(json!({
        "baseRevision": session.revision(),
        "mode": match plan.mode { Mode::Suggest => "suggest", Mode::Direct => "direct" },
        "author": session.config().author,
        "wouldApply": refused == 0,
        "refusals": refused,
        "operations": outcomes.iter().map(|o| o.operations).sum::<usize>(),
        "edits": outcomes_json(&outcomes),
        "redline": redline(session, &state, &outcomes),
        "reportImpact": "not computed here; call compatibility_report after applying",
    }))
}

/// Applies a plan all or nothing, as one undo step, and returns its change id.
///
/// # Errors
///
/// As [`preview`], plus `plan-refused` carrying every per-edit refusal when
/// any edit was refused (nothing is applied).
pub fn apply(session: &mut Session, plan: &EditPlan) -> AiResult<Value> {
    if let Some(key) = plan.idempotency_key.as_deref()
        && let Some(previous) = session
            .changes
            .iter()
            .find(|change| change.idempotency_key.as_deref() == Some(key))
    {
        return Ok(json!({
            "changeId": previous.id,
            "revision": previous.revision_after,
            "replayed": true,
        }));
    }
    let (state, outcomes) = run(session, plan)?;
    if outcomes.iter().any(|outcome| outcome.error.is_some()) {
        let refusals: Vec<Value> = outcomes
            .iter()
            .filter_map(|outcome| {
                outcome.error.as_ref().map(
                    |error| json!({ "index": outcome.index, "op": outcome.kind, "refusal": error }),
                )
            })
            .collect();
        let first = outcomes
            .iter()
            .find_map(|outcome| outcome.error.as_ref())
            .map_or("", |error| error.code);
        return Err(AiError::new(
            "plan-refused",
            "No edit was applied: at least one edit was refused.",
        )
        .with_details(json!({ "firstCode": first, "refusals": refusals })));
    }
    let group = state
        .log
        .undo_target()
        .ok_or_else(|| AiError::new("no-change", "The plan changed nothing."))?;
    let mode = plan.mode;
    let redline = redline(session, &state, &outcomes);
    let before = session.revision();
    session.state = state;
    let change_id = format!("change-{}", group.get());
    session.changes.push(ChangeRecord {
        id: change_id.clone(),
        group,
        idempotency_key: plan.idempotency_key.clone(),
        revision_after: session.revision(),
    });
    Ok(json!({
        "changeId": change_id,
        "baseRevision": before,
        "revision": session.revision(),
        "mode": match mode { Mode::Suggest => "suggest", Mode::Direct => "direct" },
        "author": session.config().author,
        "operations": outcomes.iter().map(|o| o.operations).sum::<usize>(),
        "edits": outcomes_json(&outcomes),
        "redline": redline,
    }))
}
