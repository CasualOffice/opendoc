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

use casual_doc_command::edit::{AlignmentName, CellAlignment, CellRef, LineSpacingRule, ListKind};
use casual_doc_command::edit::{
    BreakName, LineNumberRestartName, OrientationName, PageKind, PageNumberFormat, WatermarkAngle,
};
use casual_doc_command::paragraph::ParagraphChange;
// Link to Previous, Remove and page-number positions (`docs/109` HF-192, RM-23),
// on their own line per the parallel-lane import rule.
use casual_doc_command::edit::{Band, PageNumberSide};
use casual_doc_command::table::TableEdit;
use casual_doc_edit::RunningRegion;
use casual_doc_edit::{Operation, Pos, Range};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Alignment, BlockNode, NumberingRef, Paragraph, ParagraphProperties, StyleId, StyleKind, Table,
};
use casual_doc_model::v1::{
    BreakKind, HeaderFooterKind, LineNumberRestart, LineNumbering, NoteKind, NumberFormat,
    PageOrientation, Rgba, SectionId, WatermarkLayout,
};
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
use crate::text::{clip, paragraph_text, redline_runs};

/// The vocabulary's version. A new edit kind is an entry in the capability
/// registry (`casual_doc_command::COMMANDS`, `docs/181`), with its lowering,
/// or not at all.
pub const PLAN_VERSION: u32 = 1;

// The vocabulary's types live in the capability registry (`docs/181` §2.1).
pub use casual_doc_command::edit::{Decision, Edit, Mode, NewParagraph, TextFormat};

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

/// The profile `edit` needs in `mode` (`docs/178` §4.3's table).
fn needs(edit: &Edit, mode: Mode) -> Profile {
    match edit {
        Edit::Comment { .. } | Edit::ResolveComment { .. } => Profile::Comment,
        Edit::Review { .. } => Profile::Review,
        // Not trackable (the registry says why): only a direct edit, so `edit`.
        Edit::TableInsertRow { .. }
        | Edit::TableDeleteRow { .. }
        | Edit::InsertTable { .. }
        | Edit::TableInsertColumn { .. }
        | Edit::TableDeleteColumn { .. }
        | Edit::TableMergeCells { .. }
        | Edit::TableSplitCell { .. }
        | Edit::TableDelete { .. }
        | Edit::TableSetCellFormat { .. }
        | Edit::ClearFormatting { .. }
        | Edit::RestartNumbering { .. }
        | Edit::ContinueNumbering { .. }
        | Edit::SetHyperlink { .. }
        | Edit::RemoveHyperlink { .. }
        | Edit::CreateBookmark { .. }
        | Edit::RenameBookmark { .. }
        | Edit::DeleteBookmark { .. }
        | Edit::InsertField { .. }
        | Edit::InsertImage { .. }
        | Edit::SetAltText { .. }
        | Edit::ResizeObject { .. }
        | Edit::SetWrap { .. }
        | Edit::CropImage { .. }
        | Edit::DeleteObject { .. }
        | Edit::SetShapeFill { .. }
        | Edit::SetShapeOutline { .. }
        | Edit::InsertFootnote { .. }
        | Edit::InsertEndnote { .. }
        | Edit::InsertBreak { .. }
        | Edit::InsertSectionBreak { .. }
        | Edit::SetPageSetup { .. }
        | Edit::SetLineNumbering { .. }
        | Edit::SetPageNumbering { .. }
        | Edit::SetWatermark { .. }
        | Edit::InsertHeader { .. }
        | Edit::InsertFooter { .. }
        | Edit::SetDifferentFirstPage { .. }
        | Edit::SetDifferentOddEven { .. }
        | Edit::LinkToPrevious { .. }
        | Edit::RemoveHeaderFooter { .. }
        | Edit::InsertPageNumber { .. }
        | Edit::InsertChart { .. }
        | Edit::InsertTableOfContents { .. }
        | Edit::UpdateTableOfContents { .. }
        | Edit::InsertCaption { .. }
        | Edit::UpdateCaptionNumbers { .. }
        | Edit::InsertCrossReference { .. } => Profile::Edit,
        Edit::InsertText { .. }
        | Edit::ReplaceText { .. }
        | Edit::DeleteText { .. }
        | Edit::InsertParagraphs { .. }
        | Edit::ApplyParagraphStyle { .. }
        | Edit::SetTextFormat { .. }
        | Edit::RewriteBlock { .. }
        | Edit::SetAlignment { .. }
        | Edit::SetLineSpacing { .. }
        | Edit::SetParagraphSpacing { .. }
        | Edit::SetIndent { .. }
        | Edit::SetList { .. }
        | Edit::ChangeListLevel { .. }
        | Edit::SetCellText { .. } => match mode {
            Mode::Suggest => Profile::Suggest,
            Mode::Direct => Profile::Edit,
        },
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
pub(crate) struct Outcome {
    index: usize,
    kind: &'static str,
    nodes: Vec<NodeId>,
    operations: usize,
    pub(crate) error: Option<AiError>,
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

/// A shared lowering's refusal as an agent reads it: the editor's sentence,
/// without the host's `refused: ` marker and routing code. O(message).
fn refusal(message: &str) -> AiError {
    let (text, _) = casual_doc_edit::refusal::split(message);
    AiError::invalid(text.strip_prefix("refused: ").unwrap_or(text).to_owned())
}

/// A clone of the table `id` names. O(document).
fn table_of(state: &State, id: &str) -> AiResult<Table> {
    find_table(&state.document, node_id(id)?)
        .cloned()
        .ok_or_else(|| AiError::new("anchor-not-found", "No table has that id at this revision."))
}

/// `cell` as physical (row, cell) indexes of `table`, if it is one. O(1).
fn cell_of(table: &Table, cell: CellRef) -> AiResult<(usize, usize)> {
    let (row, column) = (cell.row as usize, cell.column as usize);
    table
        .rows
        .get(row)
        .and_then(|r| r.cells.get(column))
        .map(|_| (row, column))
        .ok_or_else(|| AiError::invalid("That table has no such cell."))
}

/// Every paragraph on every surface in document order, as the editor orders
/// them for its list commands. O(document).
fn ordered_paragraphs(document: &casual_doc_model::v1::Document) -> Vec<NodeId> {
    let mut out = Vec::new();
    for blocks in casual_doc_edit::surface_block_lists(document) {
        casual_doc_review::body::visit_paragraphs(blocks, &mut |paragraph| out.push(paragraph.id));
    }
    out
}

/// The tracker for a paragraph-property lowering in `mode`: a `w:pPrChange`
/// by the session's author when suggesting (the editor's own
/// `track_paragraph_change`), nothing when direct.
fn paragraph_track<'a>(
    revisions: &'a mut casual_doc_review::body::RevisionIdAllocator,
    mode: Mode,
    author: Option<String>,
    date: Option<String>,
) -> impl FnMut(&ParagraphProperties, ParagraphProperties) -> Result<ParagraphProperties, String> + 'a
{
    move |current, next| match mode {
        Mode::Suggest => {
            lower::track_paragraph_change(revisions, author.clone(), date.clone(), current, next)
        }
        Mode::Direct => Ok(next),
    }
}

/// Lowers a paragraph-property change on the paragraph `node` names through
/// the one paragraph lowering the editor uses, after `leading` operations.
/// Refuses a change that changes nothing. O(paragraph lookup).
fn paragraph_change(
    state: &mut State,
    node: &str,
    mode: Mode,
    (author, date): (Option<String>, Option<String>),
    leading: Vec<Operation>,
    nodes: &mut Vec<NodeId>,
    change: impl Fn(&mut ParagraphProperties),
) -> AiResult<Vec<Operation>> {
    let found = paragraph(&state.document, node)?;
    let id = found.id;
    let current = found.properties.get().clone();
    nodes.push(id);
    let mut track = paragraph_track(&mut state.revisions, mode, author, date);
    let ops = casual_doc_command::paragraph::set_properties(
        leading,
        vec![(id, current.clone())],
        change,
        &mut track,
    )
    .map_err(|message| refusal(&message))?;
    let unchanged = matches!(
        ops.last(),
        Some(Operation::SetParagraphProperties { properties, .. }) if **properties == current
    );
    if unchanged {
        return Err(AiError::new(
            "no-change",
            "The paragraph already has that formatting.",
        ));
    }
    Ok(ops)
}

/// The sections `ordinal` names (0-based, body order), or every section when
/// it is `None`. O(sections).
fn sections_of(state: &State, ordinal: Option<u32>) -> AiResult<Vec<SectionId>> {
    let sections = &state.document.definitions().sections;
    if sections.is_empty() {
        return Err(AiError::invalid(
            "This document declares no page setup, so it has no section to change.",
        ));
    }
    match ordinal {
        None => Ok(sections.iter().map(|boundary| boundary.id).collect()),
        Some(ordinal) => casual_doc_command::sections::section_at(
            &state.document,
            ordinal as usize,
        )
        .map(|boundary| vec![boundary.id])
        .ok_or_else(|| {
            AiError::invalid(format!(
                "There is no section {ordinal}; the document has {} (get_outline lists them).",
                sections.len()
            ))
        }),
    }
}

/// A `#RRGGBB` colour as an opaque model colour. O(1).
fn rgba(value: &str) -> AiResult<Rgba> {
    casual_doc_command::format::parse_color(value)
        .map(casual_doc_command::objects::opaque)
        .ok_or_else(|| AiError::invalid("A colour is `#RRGGBB`."))
}

/// Word's bookmark-name rule, which the editor's dialog leaves to the person
/// and an agent must meet: a letter first, then letters, digits or `_`, at
/// most 40 characters. O(name).
fn bookmark_name(name: &str) -> AiResult<()> {
    let mut chars = name.chars();
    let first = chars.next().is_some_and(char::is_alphabetic);
    if first && name.chars().count() <= 40 && chars.all(|c| c.is_alphanumeric() || c == '_') {
        Ok(())
    } else {
        Err(AiError::invalid(
            "A bookmark name starts with a letter and holds only letters, digits and `_`, at \
             most 40 characters.",
        ))
    }
}

/// The bookmark called `name`. O(bookmarks).
fn bookmark_called(state: &State, name: &str) -> AiResult<casual_doc_model::v1::BookmarkId> {
    casual_doc_command::links::bookmark_named(&state.document, name).ok_or_else(|| {
        AiError::new(
            "anchor-not-found",
            format!(
                "No bookmark is named `{}`; get_outline lists them.",
                clip(name, 80)
            ),
        )
    })
}

/// A size in points as EMU, or the refusal naming `label`. O(1).
fn emu(points: f64, label: &str) -> AiResult<i64> {
    casual_doc_command::objects::points_to_emu(points)
        .ok_or_else(|| AiError::invalid(format!("`{label}` is a positive size in points.")))
}

/// The pages a header or footer is for, as the model's kind. O(1).
const fn band_region(band: Band) -> RunningRegion {
    match band {
        Band::Header => RunningRegion::Header,
        Band::Footer => RunningRegion::Footer,
    }
}

const fn number_align(side: PageNumberSide) -> casual_doc_command::sections::PageNumberAlign {
    match side {
        PageNumberSide::Left => casual_doc_command::sections::PageNumberAlign::Left,
        PageNumberSide::Center => casual_doc_command::sections::PageNumberAlign::Center,
        PageNumberSide::Right => casual_doc_command::sections::PageNumberAlign::Right,
    }
}

fn page_kind(pages: PageKind) -> HeaderFooterKind {
    match pages {
        PageKind::Default => HeaderFooterKind::Default,
        PageKind::First => HeaderFooterKind::First,
        PageKind::Even => HeaderFooterKind::Even,
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
            // The one character-format lowering the editor's toolbar uses
            // (`docs/181` §4.2 step 2).
            let delta = casual_doc_command::format::delta(format).map_err(AiError::invalid)?;
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
                Mode::Direct => casual_doc_command::format::format_ranges(
                    &[(resolved.node, resolved.start, resolved.end)],
                    &delta,
                ),
            };
            commit(state, ops)?;
        }
        Edit::ClearFormatting { target } => {
            refuse_untracked(kind, mode)?;
            let resolved = resolve(&state.document, target)?;
            nodes.push(resolved.node);
            if resolved.start == resolved.end {
                return Err(AiError::invalid("The target is empty."));
            }
            commit(
                state,
                casual_doc_command::format::clear_ranges(&[(
                    resolved.node,
                    resolved.start,
                    resolved.end,
                )]),
            )?;
        }
        Edit::RewriteBlock { target, markdown } => {
            let whole = Anchor {
                node: target.node.clone(),
                ..Anchor::default()
            };
            // Refuses a paragraph whose offsets the edit path cannot address.
            let resolved = resolve(&state.document, &whole)?;
            let node = resolved.node;
            nodes.push(node);
            let rewrite = casual_doc_command::rewrite::parse(markdown, &node.to_string())
                .map_err(AiError::invalid)?;
            let current = paragraph_text(find(state, node)?);
            // `docs/181` §4.3: a word-level diff (casual-doc-diff's, the one
            // aligner the comparison feature uses), lowered edit by edit
            // through the ordinary text lowerings, right to left so each
            // edit's offsets are still the original text's.
            let diff = casual_doc_diff::inline::diff_text(&current, &rewrite.text);
            let mut changed = false;
            for change in diff.edits.iter().rev() {
                let (start, end) = change.left;
                let new = &rewrite.text[change.right.0 as usize..change.right.1 as usize];
                let at = node.to_string();
                let ops = match (mode, change.removes(), change.adds()) {
                    (Mode::Suggest, true, true) => {
                        lowered(lower::suggest_replace(
                            &state.document,
                            ids!(state),
                            &at,
                            start,
                            end,
                            new,
                            author.clone(),
                            date.clone(),
                        ))?
                        .operations
                    }
                    (Mode::Suggest, true, false) => {
                        lowered(lower::suggest_delete(
                            &state.document,
                            ids!(state),
                            &at,
                            start,
                            end,
                            author.clone(),
                            date.clone(),
                        ))?
                        .operations
                    }
                    (Mode::Suggest, false, true) => {
                        lowered(lower::suggest_insert(
                            &state.document,
                            ids!(state),
                            &at,
                            start,
                            new,
                            InsertStyle::Plain,
                            author.clone(),
                            date.clone(),
                            None,
                        ))?
                        .operations
                    }
                    (Mode::Direct, removes, adds) => {
                        let mut ops = Vec::new();
                        if removes {
                            ops.push(Operation::DeleteText {
                                range: Range {
                                    start: Pos::new(node, start),
                                    end: Pos::new(node, end),
                                },
                            });
                        }
                        if adds {
                            ops.push(Operation::InsertText {
                                at: Pos::new(node, start),
                                text: new.to_owned(),
                            });
                        }
                        ops
                    }
                    (Mode::Suggest, false, false) => Vec::new(),
                };
                if !ops.is_empty() {
                    commit(state, ops)?;
                    changed = true;
                }
            }
            if let Some(level) = rewrite.heading {
                let style = style_id(state, &format!("Heading {level}"))?;
                let has = find(state, node)?.properties.get().style_ref == Some(style);
                if !has {
                    let ops = paragraph_change(
                        state,
                        &target.node,
                        mode,
                        (author, date),
                        Vec::new(),
                        &mut Vec::new(),
                        |p| ParagraphChange::Style(Some(style)).apply(p),
                    )?;
                    commit(state, ops)?;
                    changed = true;
                }
            }
            if !changed {
                return Err(AiError::new(
                    "no-change",
                    "The rewrite is the paragraph's current content.",
                ));
            }
        }
        Edit::SetAlignment { target, alignment } => {
            let value = match alignment {
                AlignmentName::Start => Alignment::Start,
                AlignmentName::Center => Alignment::Center,
                AlignmentName::End => Alignment::End,
                AlignmentName::Justify => Alignment::Justify,
            };
            let ops = paragraph_change(
                state,
                &target.node,
                mode,
                (author, date),
                Vec::new(),
                nodes,
                |p| ParagraphChange::Alignment(value).apply(p),
            )?;
            commit(state, ops)?;
        }
        Edit::SetLineSpacing {
            target,
            rule,
            value,
        } => {
            if !(0..=31_680).contains(value) {
                return Err(AiError::invalid(
                    "Line spacing is 0 to 31680 (percent, or twips).",
                ));
            }
            let change = match rule {
                LineSpacingRule::Multiple => ParagraphChange::LineSpacingMultiple(
                    u16::try_from(*value).map_err(|_| AiError::invalid("Too large a multiple."))?,
                ),
                LineSpacingRule::Exact => ParagraphChange::LineSpacingFixed {
                    twips: *value,
                    at_least: false,
                },
                LineSpacingRule::AtLeast => ParagraphChange::LineSpacingFixed {
                    twips: *value,
                    at_least: true,
                },
            };
            let ops = paragraph_change(
                state,
                &target.node,
                mode,
                (author, date),
                Vec::new(),
                nodes,
                |p| change.apply(p),
            )?;
            commit(state, ops)?;
        }
        Edit::SetParagraphSpacing {
            target,
            before,
            after,
        } => {
            if before.is_none() && after.is_none() {
                return Err(AiError::invalid("Set `before`, `after` or both."));
            }
            let changes = [
                before.map(ParagraphChange::SpaceBefore),
                after.map(ParagraphChange::SpaceAfter),
            ];
            let ops = paragraph_change(
                state,
                &target.node,
                mode,
                (author, date),
                Vec::new(),
                nodes,
                |p| changes.iter().flatten().for_each(|change| change.apply(p)),
            )?;
            commit(state, ops)?;
        }
        Edit::SetIndent {
            target,
            start,
            end,
            first_line,
        } => {
            if start.is_none() && end.is_none() && first_line.is_none() {
                return Err(AiError::invalid(
                    "Set `start`, `end`, `first_line` or some.",
                ));
            }
            let changes = [
                start.map(ParagraphChange::StartIndent),
                end.map(ParagraphChange::EndIndent),
                first_line.map(ParagraphChange::FirstLineIndent),
            ];
            let ops = paragraph_change(
                state,
                &target.node,
                mode,
                (author, date),
                Vec::new(),
                nodes,
                |p| changes.iter().flatten().for_each(|change| change.apply(p)),
            )?;
            commit(state, ops)?;
        }
        Edit::SetList { target, kind: list } => {
            let current = paragraph(&state.document, &target.node)?
                .properties
                .get()
                .numbering;
            let (leading, numbering) = match list {
                ListKind::None => {
                    if current.is_none() {
                        return Err(AiError::new("no-change", "The paragraph is not in a list."));
                    }
                    (Vec::new(), None)
                }
                ListKind::Bullet | ListKind::Numbered => {
                    let numbered = *list == ListKind::Numbered;
                    // The editor's definition, joined if the document already
                    // has it, so consecutive items share one list.
                    let (instance, install) = match casual_doc_command::lists::existing_definition(
                        &state.document,
                        numbered,
                    ) {
                        Some(instance) => (instance, Vec::new()),
                        None => {
                            let ids = &mut state.ids;
                            casual_doc_command::lists::new_definition(numbered, &mut || {
                                ids.next_id().ok()
                            })
                            .map_err(|message| refusal(&message))?
                        }
                    };
                    // An item already in a list keeps its level.
                    let level = current.map_or(0, |numbering| numbering.level);
                    (install, Some(NumberingRef { instance, level }))
                }
            };
            let ops = paragraph_change(
                state,
                &target.node,
                mode,
                (author, date),
                leading,
                nodes,
                |p| ParagraphChange::Numbering(numbering).apply(p),
            )?;
            commit(state, ops)?;
        }
        Edit::ChangeListLevel { target, delta } => {
            if paragraph(&state.document, &target.node)?
                .properties
                .get()
                .numbering
                .is_none()
            {
                return Err(AiError::invalid(
                    "list level adjustment requires a list paragraph",
                ));
            }
            let ops = paragraph_change(
                state,
                &target.node,
                mode,
                (author, date),
                Vec::new(),
                nodes,
                |p| ParagraphChange::ListLevel(*delta).apply(p),
            )?;
            commit(state, ops)?;
        }
        Edit::RestartNumbering { target, value } => {
            refuse_untracked(kind, mode)?;
            let node = paragraph(&state.document, &target.node)?.id;
            nodes.push(node);
            let ordered = ordered_paragraphs(&state.document);
            let ids = &mut state.ids;
            let ops = casual_doc_command::lists::restart(
                &state.document,
                &ordered,
                node,
                value.unwrap_or(1),
                &mut || ids.next_id().ok(),
            )
            .map_err(|message| refusal(&message))?;
            commit(state, ops)?;
        }
        Edit::ContinueNumbering { target } => {
            refuse_untracked(kind, mode)?;
            let node = paragraph(&state.document, &target.node)?.id;
            nodes.push(node);
            let ordered = ordered_paragraphs(&state.document);
            let ops =
                casual_doc_command::lists::continue_numbering(&state.document, &ordered, node)
                    .map_err(|message| refusal(&message))?;
            commit(state, ops)?;
        }
        Edit::InsertTable {
            after,
            rows,
            columns,
        } => {
            refuse_untracked(kind, mode)?;
            let node = paragraph(&state.document, &after.node)?.id;
            nodes.push(node);
            let index = casual_doc_command::table::body_index_after(&state.document, node);
            let page = casual_doc_layout::document_layout::document_page_config(&state.document);
            let width =
                page.page_size.width.raw() - page.margin_start.raw() - page.margin_end.raw();
            let ids = &mut state.ids;
            let edit =
                casual_doc_command::table::insert_table(index, *rows, *columns, width, &mut || {
                    ids.next_id().ok()
                })
                .map_err(|message| refusal(&message))?;
            commit(state, edit.operations)?;
        }
        Edit::TableInsertColumn { table, index } => {
            refuse_untracked(kind, mode)?;
            let found = table_of(state, table)?;
            let ids = &mut state.ids;
            let edit =
                casual_doc_command::table::insert_column_at(&found, *index as usize, &mut || {
                    ids.next_id().ok()
                })
                .map_err(|message| refusal(&message))?;
            commit(state, edit.operations)?;
        }
        Edit::TableDeleteColumn { table, index } => {
            refuse_untracked(kind, mode)?;
            let found = table_of(state, table)?;
            let index = *index as usize;
            let edit = casual_doc_command::table::delete_columns(&found, 0, index, index)
                .map_err(|message| refusal(&message))?;
            commit(state, edit.operations)?;
        }
        Edit::TableMergeCells { table, from, to } => {
            refuse_untracked(kind, mode)?;
            let found = table_of(state, table)?;
            let rect = casual_doc_command::table::cell_rect(
                &found,
                cell_of(&found, *from)?,
                cell_of(&found, *to)?,
            )
            .map_err(|message| refusal(&message))?;
            let ids = &mut state.ids;
            let edit: TableEdit =
                casual_doc_command::table::merge(&found, rect, &mut || ids.next_id().ok())
                    .map_err(|message| refusal(&message))?;
            commit(state, edit.operations)?;
        }
        Edit::TableSplitCell { table, cell } => {
            refuse_untracked(kind, mode)?;
            let found = table_of(state, table)?;
            let (row, column) = cell_of(&found, *cell)?;
            let ids = &mut state.ids;
            let edit =
                casual_doc_command::table::unmerge(&found, row, column, &mut || ids.next_id().ok())
                    .map_err(|message| refusal(&message))?;
            commit(state, edit.operations)?;
        }
        Edit::TableDelete { table } => {
            refuse_untracked(kind, mode)?;
            let found = table_of(state, table)?;
            commit(state, casual_doc_command::table::delete_table(found.id))?;
        }
        Edit::TableSetCellFormat {
            table,
            from,
            to,
            shading,
            vertical_alignment,
        } => {
            refuse_untracked(kind, mode)?;
            if shading.is_none() && vertical_alignment.is_none() {
                return Err(AiError::invalid(
                    "Set `shading`, `vertical_alignment` or both.",
                ));
            }
            let fill = match shading.as_deref() {
                None => None,
                Some("none") => Some(None),
                Some(value) => Some(Some(
                    casual_doc_command::format::parse_color(value)
                        .ok_or_else(|| AiError::invalid("A colour is `#RRGGBB` or `none`."))?,
                )),
            };
            let alignment = vertical_alignment.map(|alignment| {
                casual_doc_command::table::cell_vertical_alignment(match alignment {
                    CellAlignment::Top => "top",
                    CellAlignment::Center => "center",
                    CellAlignment::Bottom => "bottom",
                })
            });
            let found = table_of(state, table)?;
            let from = cell_of(&found, *from)?;
            let to = to.map_or(Ok(from), |to| cell_of(&found, to))?;
            // The editor's cell range: the rectangle grown to whole merged
            // cells, each merged cell once.
            let cells = casual_doc_command::table::range_cells(&found, from, to)
                .map_err(|message| refusal(&message))?;
            commit(
                state,
                casual_doc_command::table::set_cell_properties(cells, |p| {
                    if let Some(fill) = fill {
                        p.shading.fill = fill;
                    }
                    if let Some(alignment) = alignment {
                        p.vertical_alignment = Some(alignment);
                    }
                }),
            )?;
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
        Edit::SetHyperlink {
            target,
            url,
            tooltip,
        } => {
            refuse_untracked(kind, mode)?;
            casual_doc_command::links::agent_target(url).map_err(AiError::invalid)?;
            let resolved = resolve(&state.document, target)?;
            nodes.push(resolved.node);
            if resolved.start == resolved.end {
                return Err(AiError::invalid("A link needs a non-empty quote."));
            }
            let ids = &mut state.ids;
            let ops = casual_doc_command::links::set_hyperlink(
                resolved.node,
                resolved.start,
                resolved.end,
                url,
                tooltip.clone(),
                &mut || ids.next_id().ok(),
            )
            .map_err(|message| refusal(&message))?;
            commit(state, ops)?;
        }
        Edit::RemoveHyperlink { target } => {
            refuse_untracked(kind, mode)?;
            let resolved = resolve(&state.document, target)?;
            nodes.push(resolved.node);
            if resolved.start == resolved.end {
                return Err(AiError::invalid("Quote the link's text exactly."));
            }
            commit(
                state,
                casual_doc_command::links::remove_hyperlink(
                    resolved.node,
                    resolved.start,
                    resolved.end,
                ),
            )?;
        }
        Edit::CreateBookmark { target, name } => {
            refuse_untracked(kind, mode)?;
            bookmark_name(name)?;
            if casual_doc_command::links::bookmark_named(&state.document, name).is_some() {
                return Err(AiError::invalid(format!(
                    "A bookmark named `{name}` exists; rename or delete it first."
                )));
            }
            let resolved = resolve(&state.document, target)?;
            nodes.push(resolved.node);
            let ids = &mut state.ids;
            let ops = casual_doc_command::links::create_bookmark(
                name.clone(),
                Pos::new(resolved.node, resolved.start),
                Pos::new(resolved.node, resolved.end),
                &mut || ids.next_id().ok(),
            )
            .map_err(|message| refusal(&message))?;
            commit(state, ops)?;
        }
        Edit::RenameBookmark { name, new_name } => {
            refuse_untracked(kind, mode)?;
            bookmark_name(new_name)?;
            let bookmark = bookmark_called(state, name)?;
            if casual_doc_command::links::bookmark_named(&state.document, new_name).is_some() {
                return Err(AiError::invalid(format!(
                    "A bookmark named `{new_name}` exists already."
                )));
            }
            commit(
                state,
                casual_doc_command::links::rename_bookmark(bookmark, new_name.clone()),
            )?;
        }
        Edit::DeleteBookmark { name } => {
            refuse_untracked(kind, mode)?;
            let bookmark = bookmark_called(state, name)?;
            commit(state, casual_doc_command::links::delete_bookmark(bookmark))?;
        }
        Edit::InsertField {
            at,
            kind: field,
            result,
        } => {
            refuse_untracked(kind, mode)?;
            let target = resolve(&state.document, at)?;
            nodes.push(target.node);
            let point = Pos::new(target.node, target.point(at.side.unwrap_or(Side::After)));
            let ids = &mut state.ids;
            let ops = casual_doc_command::fields::insert_field(
                point,
                field.token(),
                result.clone().unwrap_or_default(),
                &mut || ids.next_id().ok(),
            )
            .map_err(|message| refusal(&message))?;
            commit(state, ops)?;
        }
        Edit::InsertImage {
            at,
            data,
            content_type,
            width_pt,
            height_pt,
            alt_text,
        } => {
            refuse_untracked(kind, mode)?;
            let (media_type, _) = casual_doc_command::objects::image_part_type(content_type)
                .ok_or_else(|| {
                    AiError::invalid(
                        "`content_type` is image/png, image/jpeg, image/gif, image/bmp, \
                         image/tiff or image/webp.",
                    )
                })?;
            let bytes = casual_doc_command::objects::decode_base64(
                data,
                casual_doc_command::objects::MAX_IMAGE_BYTES,
            )
            .map_err(AiError::invalid)?;
            if !casual_doc_command::objects::bytes_are(media_type, &bytes) {
                return Err(AiError::invalid(format!(
                    "The data is not a {media_type} file."
                )));
            }
            // The picture's own size where its header states it (96 DPI), so
            // one given dimension keeps the proportions.
            let natural =
                casual_doc_command::objects::pixel_size(media_type, &bytes).map(|(w, h)| {
                    (
                        casual_doc_command::objects::pixels_to_emu(w),
                        casual_doc_command::objects::pixels_to_emu(h),
                    )
                });
            let (width, height) = match (width_pt, height_pt, natural) {
                (Some(w), Some(h), _) => (emu(*w, "width_pt")?, emu(*h, "height_pt")?),
                (Some(w), None, Some((nw, nh))) => {
                    let w = emu(*w, "width_pt")?;
                    (
                        w,
                        (i128::from(w) * i128::from(nh) / i128::from(nw)).max(1) as i64,
                    )
                }
                (None, Some(h), Some((nw, nh))) => {
                    let h = emu(*h, "height_pt")?;
                    (
                        (i128::from(h) * i128::from(nw) / i128::from(nh)).max(1) as i64,
                        h,
                    )
                }
                (None, None, Some(size)) => size,
                _ => {
                    return Err(AiError::invalid(
                        "This picture's header does not state its size; give `width_pt` and \
                         `height_pt`.",
                    ));
                }
            };
            let target = resolve(&state.document, at)?;
            nodes.push(target.node);
            let point = Pos::new(target.node, target.point(at.side.unwrap_or(Side::After)));
            let ids = &mut state.ids;
            let mut picture = casual_doc_command::objects::insert_picture(
                point,
                content_type,
                width,
                height,
                &mut || ids.next_id().ok(),
            )
            .map_err(|message| refusal(&message))?;
            if let Some(text) = alt_text.as_ref().filter(|text| !text.is_empty()) {
                picture
                    .operations
                    .extend(casual_doc_command::objects::set_alt_text(
                        picture.drawing,
                        Some(text.clone()),
                    ));
            }
            commit(state, picture.operations)?;
            state.media.push((picture.part_name, bytes));
        }
        Edit::SetAltText { object, text } => {
            refuse_untracked(kind, mode)?;
            let text = text.clone().filter(|text| !text.is_empty());
            commit(
                state,
                casual_doc_command::objects::set_alt_text(node_id(object)?, text),
            )?;
        }
        Edit::ResizeObject {
            object,
            width_pt,
            height_pt,
        } => {
            refuse_untracked(kind, mode)?;
            let ops = casual_doc_command::objects::set_extent(
                node_id(object)?,
                emu(*width_pt, "width_pt")?,
                emu(*height_pt, "height_pt")?,
            )
            .map_err(AiError::invalid)?;
            commit(state, ops)?;
        }
        Edit::SetWrap { object, wrap } => {
            refuse_untracked(kind, mode)?;
            let ops = casual_doc_command::objects::set_wrap(
                &state.document,
                node_id(object)?,
                wrap.token(),
            )
            .map_err(|message| refusal(&message))?;
            commit(state, ops)?;
        }
        Edit::CropImage {
            object,
            left,
            top,
            right,
            bottom,
        } => {
            refuse_untracked(kind, mode)?;
            let insets = [*left, *top, *right, *bottom];
            if insets.iter().any(|edge| !(0.0..=1.0).contains(edge)) {
                return Err(AiError::invalid(
                    "Each crop edge is a fraction from 0 to 1.",
                ));
            }
            let crop = casual_doc_command::objects::crop(insets);
            let crop = (!crop.is_identity()).then_some(crop);
            commit(
                state,
                casual_doc_command::objects::set_crop(node_id(object)?, crop),
            )?;
        }
        Edit::DeleteObject { object } => {
            refuse_untracked(kind, mode)?;
            commit(
                state,
                casual_doc_command::objects::delete_object(node_id(object)?),
            )?;
        }
        Edit::SetShapeFill { shape, color } => {
            refuse_untracked(kind, mode)?;
            let color = match color.as_str() {
                "none" => None,
                value => Some(rgba(value)?),
            };
            commit(
                state,
                casual_doc_command::objects::set_shape_fill(node_id(shape)?, color),
            )?;
        }
        Edit::SetShapeOutline {
            shape,
            color,
            width_pt,
        } => {
            refuse_untracked(kind, mode)?;
            let width = match width_pt {
                Some(points) if points.is_finite() && *points >= 0.0 => {
                    Some(*points * casual_doc_command::objects::EMU_PER_POINT as f64)
                }
                Some(_) => return Err(AiError::invalid("`width_pt` is a size in points.")),
                None => None,
            };
            let ops = match color.as_deref() {
                Some("none") => {
                    if width.is_some() {
                        return Err(AiError::invalid(
                            "`none` removes the outline; it takes no `width_pt`.",
                        ));
                    }
                    casual_doc_command::objects::set_shape_outline(
                        &state.document,
                        node_id(shape)?,
                        None,
                        None,
                    )
                }
                None if width.is_none() => {
                    return Err(AiError::invalid("Set `color`, `width_pt` or both."));
                }
                value => casual_doc_command::objects::set_shape_outline(
                    &state.document,
                    node_id(shape)?,
                    value.map(rgba).transpose()?,
                    width,
                ),
            };
            commit(state, ops)?;
        }
        Edit::InsertFootnote { at, text } | Edit::InsertEndnote { at, text } => {
            refuse_untracked(kind, mode)?;
            if text.is_empty() {
                return Err(AiError::invalid("`text` is empty."));
            }
            let note = if matches!(edit, Edit::InsertFootnote { .. }) {
                NoteKind::Footnote
            } else {
                NoteKind::Endnote
            };
            let target = resolve(&state.document, at)?;
            nodes.push(target.node);
            let point = Pos::new(target.node, target.point(at.side.unwrap_or(Side::After)));
            let ids = &mut state.ids;
            let lowered =
                casual_doc_command::notes::insert_note(note, point, Some(text), &mut || {
                    ids.next_id().ok()
                })
                .map_err(|message| refusal(&message))?;
            nodes.push(lowered.body);
            commit(state, lowered.operations)?;
        }
        Edit::InsertBreak { at, kind: name } => {
            refuse_untracked(kind, mode)?;
            let target = resolve(&state.document, at)?;
            nodes.push(target.node);
            let point = Pos::new(target.node, target.point(at.side.unwrap_or(Side::After)));
            let ids = &mut state.ids;
            let mut mint = || ids.next_id().ok();
            let ops = match name {
                BreakName::BlankPage => {
                    casual_doc_command::breaks::insert_blank_page(&state.document, point, &mut mint)
                }
                BreakName::Line | BreakName::Page | BreakName::Column => {
                    let break_kind = match name {
                        BreakName::Line => BreakKind::Line,
                        BreakName::Page => BreakKind::Page,
                        _ => BreakKind::Column,
                    };
                    casual_doc_command::breaks::insert_break(
                        &state.document,
                        point,
                        break_kind,
                        &mut mint,
                    )
                }
            }
            .map_err(|message| refusal(&message))?;
            commit(state, ops)?;
        }
        Edit::InsertSectionBreak { at, start } => {
            refuse_untracked(kind, mode)?;
            let target = resolve(&state.document, at)?;
            nodes.push(target.node);
            let point = Pos::new(target.node, target.point(at.side.unwrap_or(Side::After)));
            let start = casual_doc_command::breaks::section_start(start.token())
                .ok_or_else(|| AiError::invalid("Unknown section start."))?;
            let ids = &mut state.ids;
            let lowered = casual_doc_command::breaks::section_break(
                &state.document,
                point,
                start,
                &mut || ids.next_id().ok(),
            )
            .map_err(|message| refusal(&message))?;
            commit(state, lowered.operations)?;
        }
        Edit::SetPageSetup {
            section,
            width,
            height,
            orientation,
            margins,
            columns,
            column_spacing,
        } => {
            refuse_untracked(kind, mode)?;
            if width.is_none()
                && height.is_none()
                && orientation.is_none()
                && margins.is_none()
                && columns.is_none()
                && column_spacing.is_none()
            {
                return Err(AiError::invalid(
                    "Set the size, orientation, margins or columns to change.",
                ));
            }
            for value in [*width, *height].into_iter().flatten() {
                if !(1_440..=31_680).contains(&value) {
                    return Err(AiError::invalid(
                        "A page side is 1440 to 31680 twips (1 to 22 inches).",
                    ));
                }
            }
            let margins = margins.unwrap_or_default();
            for value in [margins.top, margins.bottom, margins.left, margins.right]
                .into_iter()
                .flatten()
            {
                if !(0..=31_680).contains(&value) {
                    return Err(AiError::invalid("A margin is 0 to 31680 twips."));
                }
            }
            let mut ops = Vec::new();
            for id in sections_of(state, *section)? {
                let current = casual_doc_command::sections::boundary(&state.document, id)
                    .ok_or_else(|| AiError::engine("A listed section is missing."))?
                    .clone();
                let mut size = current.page_size;
                let landscape =
                    |size: casual_doc_model::v1::PageSize| size.width_twips > size.height_twips;
                // Word's orientation switch swaps the sides; explicit sides win.
                if let Some(orientation) = orientation {
                    let want = *orientation == OrientationName::Landscape;
                    if landscape(size) != want && width.is_none() && height.is_none() {
                        size = casual_doc_model::v1::PageSize {
                            width_twips: size.height_twips,
                            height_twips: size.width_twips,
                        };
                    }
                }
                size.width_twips = width.unwrap_or(size.width_twips);
                size.height_twips = height.unwrap_or(size.height_twips);
                let mut page_margins = current.page_margins;
                page_margins.top_twips = margins.top.unwrap_or(page_margins.top_twips);
                page_margins.bottom_twips = margins.bottom.unwrap_or(page_margins.bottom_twips);
                page_margins.start_twips = margins.left.unwrap_or(page_margins.start_twips);
                page_margins.end_twips = margins.right.unwrap_or(page_margins.end_twips);
                if page_margins.start_twips + page_margins.end_twips >= size.width_twips
                    || page_margins.top_twips + page_margins.bottom_twips >= size.height_twips
                {
                    return Err(AiError::invalid(
                        "The margins leave no room for text on that page.",
                    ));
                }
                let orientation_value = match orientation {
                    Some(OrientationName::Portrait) => Some(PageOrientation::Portrait),
                    Some(OrientationName::Landscape) => Some(PageOrientation::Landscape),
                    None => current.orientation,
                };
                let new_columns = if columns.is_some() || column_spacing.is_some() {
                    let mut layout = current.columns.clone();
                    if let Some(count) = columns {
                        if !(1..=45).contains(count) {
                            return Err(AiError::invalid("A section has 1 to 45 columns."));
                        }
                        layout.count = *count;
                        // A new count is equal-width columns, as Word's
                        // presets make them.
                        layout.equal_width = None;
                        layout.columns = Vec::new();
                    }
                    if column_spacing.is_some() {
                        layout.space_twips = *column_spacing;
                    }
                    Some(layout)
                } else {
                    None
                };
                ops.extend(casual_doc_command::sections::set_geometry(
                    &state.document,
                    id,
                    size,
                    page_margins,
                    orientation_value,
                    new_columns,
                ));
            }
            commit(state, ops)?;
        }
        Edit::SetLineNumbering {
            section,
            count_by,
            start,
            restart,
        } => {
            refuse_untracked(kind, mode)?;
            if !(0..=100).contains(count_by) {
                return Err(AiError::invalid("`count_by` is 0 (off) to 100."));
            }
            let rule = if *count_by == 0 {
                LineNumbering::default()
            } else {
                LineNumbering {
                    count_by: Some(*count_by),
                    start: *start,
                    distance: None,
                    restart: restart.map(|restart| match restart {
                        LineNumberRestartName::EachPage => LineNumberRestart::NewPage,
                        LineNumberRestartName::EachSection => LineNumberRestart::NewSection,
                        LineNumberRestartName::Continuous => LineNumberRestart::Continuous,
                    }),
                }
            };
            let ops = sections_of(state, *section)?
                .into_iter()
                .flat_map(|id| casual_doc_command::sections::set_line_numbering(id, rule))
                .collect();
            commit(state, ops)?;
        }
        Edit::SetPageNumbering {
            section,
            format,
            start,
        } => {
            refuse_untracked(kind, mode)?;
            let format = format.map(|format| match format {
                PageNumberFormat::Decimal => NumberFormat::Decimal,
                PageNumberFormat::LowerRoman => NumberFormat::LowerRoman,
                PageNumberFormat::UpperRoman => NumberFormat::UpperRoman,
                PageNumberFormat::LowerLetter => NumberFormat::LowerLetter,
                PageNumberFormat::UpperLetter => NumberFormat::UpperLetter,
            });
            let mut ops = Vec::new();
            for id in sections_of(state, *section)? {
                // The dialog's lowering writes the vertical alignment too; an
                // agent's numbering keeps the section's own.
                let alignment = casual_doc_command::sections::boundary(&state.document, id)
                    .and_then(|boundary| boundary.vertical_alignment);
                ops.extend(casual_doc_command::sections::set_section_layout(
                    id,
                    format.clone(),
                    *start,
                    alignment,
                ));
            }
            commit(state, ops)?;
        }
        Edit::SetWatermark {
            text,
            color,
            layout,
            font,
        } => {
            refuse_untracked(kind, mode)?;
            let watermark = match text.as_deref().filter(|text| !text.is_empty()) {
                None => None,
                Some(words) => Some(Box::new(casual_doc_command::sections::text_watermark(
                    words.to_owned(),
                    font.as_deref(),
                    None,
                    rgba(color.as_deref().unwrap_or("#C0C0C0"))?,
                    false,
                    false,
                    match layout {
                        Some(WatermarkAngle::Horizontal) => WatermarkLayout::Horizontal,
                        _ => WatermarkLayout::Diagonal,
                    },
                    true,
                ))),
            };
            sections_of(state, None)?;
            commit(
                state,
                casual_doc_command::sections::set_watermark(&state.document, watermark),
            )?;
        }
        Edit::InsertHeader { section, pages } | Edit::InsertFooter { section, pages } => {
            refuse_untracked(kind, mode)?;
            let region = if matches!(edit, Edit::InsertHeader { .. }) {
                RunningRegion::Header
            } else {
                RunningRegion::Footer
            };
            let id = sections_of(state, Some(*section))?[0];
            let page = page_kind(*pages);
            if casual_doc_command::sections::has_running_content(
                &state.document,
                *section as usize,
                region,
                page,
            ) {
                return Err(AiError::invalid(
                    "That section already shows one, its own or linked from an earlier section; \
                     get_outline lists it, and insert_text writes into it.",
                ));
            }
            let ids = &mut state.ids;
            let lowered =
                casual_doc_command::sections::create_running_content(id, region, page, &mut || {
                    ids.next_id().ok()
                })
                .map_err(|message| refusal(&message))?;
            nodes.push(lowered.caret.node);
            commit(state, lowered.operations)?;
        }
        Edit::SetDifferentFirstPage { section, enabled } => {
            refuse_untracked(kind, mode)?;
            let id = sections_of(state, Some(*section))?[0];
            commit(
                state,
                casual_doc_command::sections::set_title_page(id, *enabled),
            )?;
        }
        Edit::SetDifferentOddEven { enabled } => {
            refuse_untracked(kind, mode)?;
            commit(
                state,
                casual_doc_command::sections::set_even_and_odd(*enabled),
            )?;
        }
        // The editor's own lowerings (`casual_doc_command::sections`), so an
        // agent's unlink copies the band exactly as a person's does.
        Edit::LinkToPrevious {
            section,
            band,
            pages,
            linked,
        } => {
            refuse_untracked(kind, mode)?;
            sections_of(state, Some(*section))?;
            let ids = &mut state.ids;
            let lowered = casual_doc_command::sections::set_linked_to_previous(
                &state.document,
                *section as usize,
                band_region(*band),
                page_kind(*pages),
                *linked,
                &mut || ids.next_id().ok(),
            )
            .map_err(|message| refusal(&message))?;
            if lowered.operations.is_empty() {
                return Err(AiError::invalid(if *linked {
                    "That section's band is already linked to the previous section's."
                } else {
                    "That section already has its own; it is not linked."
                }));
            }
            commit(state, lowered.operations)?;
        }
        Edit::RemoveHeaderFooter {
            section,
            band,
            pages,
        } => {
            refuse_untracked(kind, mode)?;
            sections_of(state, Some(*section))?;
            let ids = &mut state.ids;
            let operations = casual_doc_command::sections::remove_running_content(
                &state.document,
                *section as usize,
                band_region(*band),
                page_kind(*pages),
                &mut || ids.next_id().ok(),
            )
            .map_err(|message| refusal(&message))?;
            commit(state, operations)?;
        }
        Edit::InsertPageNumber {
            section,
            band,
            pages,
            side,
            of_total,
            before,
            between,
            after,
        } => {
            refuse_untracked(kind, mode)?;
            sections_of(state, Some(*section))?;
            // An agent names the words; omitted, they are Word's English ones.
            let text = if *of_total {
                casual_doc_command::sections::PageNumberText::OfTotal {
                    before: before.clone().unwrap_or_else(|| "Page ".to_owned()),
                    between: between.clone().unwrap_or_else(|| " of ".to_owned()),
                    after: after.clone().unwrap_or_default(),
                }
            } else {
                casual_doc_command::sections::PageNumberText::Number
            };
            let ids = &mut state.ids;
            let lowered = casual_doc_command::sections::insert_page_number(
                &state.document,
                *section as usize,
                band_region(*band),
                page_kind(*pages),
                number_align(*side),
                &text,
                &mut || ids.next_id().ok(),
            )
            .map_err(|message| refusal(&message))?;
            nodes.push(lowered.caret.node);
            commit(state, lowered.operations)?;
        }
        Edit::InsertChart { at, kind: chart } => {
            refuse_untracked(kind, mode)?;
            let target = resolve(&state.document, at)?;
            nodes.push(target.node);
            let point = Pos::new(target.node, target.point(at.side.unwrap_or(Side::After)));
            let ids = &mut state.ids;
            let ops = casual_doc_command::charts::insert_chart(point, chart.token(), &mut || {
                ids.next_id().ok()
            })
            .map_err(|message| refusal(&message))?;
            commit(state, ops)?;
        }
        // `docs/181` §12, lane references: the editor's own lowerings, with
        // heading levels and page labels from this state (`layout_facts`).
        Edit::InsertTableOfContents { before, levels } => {
            refuse_untracked(kind, mode)?;
            let levels = levels.unwrap_or(casual_doc_command::references::DEFAULT_TOC_LEVEL);
            if !(1..=casual_doc_command::references::MAX_TOC_LEVEL).contains(&levels) {
                return Err(AiError::invalid("`levels` is 1 to 9."));
            }
            let node = paragraph(&state.document, &before.node)?.id;
            let page = casual_doc_layout::document_layout::document_page_config(&state.document);
            let tab = page.page_size.width.raw() - page.margin_start.raw() - page.margin_end.raw();
            let facts = crate::layout_facts::AgentFacts::paginated(&state.document);
            let ids = &mut state.ids;
            let placed = casual_doc_command::references::insert_table_of_contents(
                &state.document,
                &facts,
                node,
                levels,
                tab,
                &mut || ids.next_id().ok(),
            )
            .map_err(|message| refusal(&message))?;
            drop(facts);
            nodes.push(placed.caret.node);
            commit(state, placed.operations)?;
        }
        Edit::UpdateTableOfContents {
            mode: update,
            field,
        } => {
            refuse_untracked(kind, mode)?;
            let field = field
                .as_deref()
                .map(node_id)
                .transpose()?
                .map(casual_doc_model::v1::FieldRangeId::new);
            let page = casual_doc_layout::document_layout::document_page_config(&state.document);
            let tab = page.page_size.width.raw() - page.margin_start.raw() - page.margin_end.raw();
            let facts = crate::layout_facts::AgentFacts::paginated(&state.document);
            let ids = &mut state.ids;
            let ops = casual_doc_command::references::update_table_of_contents(
                &state.document,
                &facts,
                field,
                update.mode(),
                tab,
                &mut || ids.next_id().ok(),
            )
            .map_err(|message| refusal(&message))?;
            drop(facts);
            for op in &ops {
                if let Operation::InsertBlocks { blocks, .. } = op {
                    nodes.extend(blocks.iter().filter_map(|block| match block {
                        BlockNode::Paragraph(paragraph) => Some(paragraph.id),
                        _ => None,
                    }));
                }
            }
            commit(state, ops)?;
        }
        Edit::InsertCaption {
            target,
            label,
            text,
            position,
            exclude_label,
            number_format,
            chapter_level,
            separator,
        } => {
            refuse_untracked(kind, mode)?;
            let target = node_id(target)?;
            let spec = casual_doc_edit::references::CaptionSpec {
                label: label.clone(),
                text: text.clone(),
                position: position.position(),
                exclude_label: *exclude_label,
                number_format: number_format.format(),
                chapter: chapter_level.map(|heading_level| {
                    casual_doc_edit::references::CaptionChapter {
                        heading_level,
                        separator: separator.separator(),
                    }
                }),
            };
            let facts = crate::layout_facts::AgentFacts::headings(&state.document);
            let ids = &mut state.ids;
            let placed = casual_doc_command::references::insert_caption(
                &state.document,
                &facts,
                target,
                spec,
                &mut || ids.next_id().ok(),
            )
            .map_err(|message| refusal(&message))?;
            drop(facts);
            nodes.push(placed.caret.node);
            commit(state, placed.operations)?;
        }
        Edit::UpdateCaptionNumbers {} => {
            refuse_untracked(kind, mode)?;
            let facts = crate::layout_facts::AgentFacts::headings(&state.document);
            let ids = &mut state.ids;
            let placed = casual_doc_command::references::update_caption_numbers(
                &state.document,
                &facts,
                &mut || ids.next_id().ok(),
            )
            .map_err(|message| refusal(&message))?;
            drop(facts);
            nodes.extend(placed.operations.iter().filter_map(|op| match op {
                Operation::SetInlines { node, .. } => Some(*node),
                _ => None,
            }));
            commit(state, placed.operations)?;
        }
        Edit::InsertCrossReference {
            at,
            target,
            show,
            hyperlink,
            include_above_below,
        } => {
            refuse_untracked(kind, mode)?;
            let point = resolve(&state.document, at)?;
            let caret = Pos::new(point.node, point.point(at.side.unwrap_or(Side::After)));
            let target = node_id(target)?;
            let to = show.reference_to();
            // Only a page reference prints a page, so only it pays for a
            // layout pass; every other kind reads the model alone.
            let facts = if to == casual_doc_edit::references::ReferenceTo::PageNumber {
                crate::layout_facts::AgentFacts::paginated(&state.document)
            } else {
                crate::layout_facts::AgentFacts::headings(&state.document)
            };
            let ids = &mut state.ids;
            let ops = casual_doc_command::references::insert_cross_reference(
                &state.document,
                &facts,
                caret,
                target,
                to,
                *hyperlink,
                *include_above_below,
                &mut || ids.next_id().ok(),
            )
            .map_err(|message| refusal(&message))?;
            drop(facts);
            nodes.push(caret.node);
            commit(state, ops)?;
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
pub(crate) fn run(session: &Session, plan: &EditPlan) -> AiResult<(State, Vec<Outcome>)> {
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
        let needed = needs(edit, plan.mode);
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
            "afterRuns": now.map(|p| redline_runs(p, 100)),
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
    Ok(preview_value(session, plan, &state, &outcomes))
}

/// The preview's report of a run, shared by [`preview`] and the live editor's
/// compile (`crate::editor`), so the two cannot describe one run differently.
/// O(edits + affected blocks).
pub(crate) fn preview_value(
    session: &Session,
    plan: &EditPlan,
    state: &State,
    outcomes: &[Outcome],
) -> Value {
    let refused = outcomes
        .iter()
        .filter(|outcome| outcome.error.is_some())
        .count();
    json!({
        "baseRevision": session.revision(),
        "mode": match plan.mode { Mode::Suggest => "suggest", Mode::Direct => "direct" },
        "author": session.config().author,
        "wouldApply": refused == 0,
        "refusals": refused,
        "operations": outcomes.iter().map(|o| o.operations).sum::<usize>(),
        "edits": outcomes_json(outcomes),
        "redline": redline(session, state, outcomes),
        "reportImpact": "not computed here; call compatibility_report after applying",
    })
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
    session.adopt(state);
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
