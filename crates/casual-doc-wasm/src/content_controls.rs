// SPDX-License-Identifier: Apache-2.0

//! Authoring content controls (`w:sdt`): Word's Developer ▸ Controls
//! (`docs/153` `page.contentControls`, `shell.content-controls`,
//! `shell.form-fields`; `docs/109` RM-34).
//!
//! The model has carried `w:sdt` as a first-class container since Layer 1, and
//! import, layout and export all honour it. What was missing is everything a
//! person does to one: insert it, configure it, fill it, lock it, remove it.
//!
//! # The interaction, from Word
//!
//! - **Insert** wraps the selection, or — with none — inserts the control with
//!   Word's placeholder text, flagged `w:showingPlcHdr` so the first keystroke
//!   replaces it ([`WasmDocument::fill_content_control_text`]). Word's seven
//!   insertable kinds are offered: rich text, plain text, picture, check box,
//!   combo box, drop-down list and date picker. A check box carries Word's own
//!   default glyphs (U+2612 / U+2610 in MS Gothic), a drop-down list Word's
//!   "Choose an item." entry. Every inserted control is given a unique `w:id`,
//!   which Word always writes and which the form-roles sidecar keys by.
//! - **Properties** is Word's Content Control Properties dialog: title
//!   (`w:alias`), tag, colour (`w15:color`), "remove when edited"
//!   (`w:temporary`), the two lock boxes (`w:lock`), the list entries, the date
//!   display format — and ONLYOFFICE's placeholder field, which rewrites the
//!   content while the control is still showing its placeholder.
//! - **Remove Content Control** unwraps the control and keeps its content, as
//!   Word's context menu does; a control locked against deletion refuses.
//! - **Fill**: a drop-down or combo box offers its entries, a date picker a
//!   date, a picture control a file, a plain or rich text control takes typing.
//!
//! # Locks are enforced at the choke point
//!
//! [`refuse_if_locked`] runs inside `refuse_if_protected` for every operation:
//! a `contentLocked` / `sdtContentLocked` control refuses an edit inside it, and
//! an `sdtLocked` / `sdtContentLocked` control refuses a deletion that would
//! remove it whole. ONLYOFFICE checks the same two flags in its controller
//! (sdkjs `word/Editor/StructuredDocumentTags/InlineLevel.js`,
//! `CanBeDeleted` / `CanBeEdited`); here a host driving the engine directly gets
//! the same refusal. `SetInlines` is not judged, because it is the inverse
//! vehicle every undo arrives as — the methods in this module check the lock
//! themselves before they build one.
//!
//! # Forms protection
//!
//! `w:documentProtection w:edit="forms"` leaves form fields editable and locks
//! everything else. [`fills_a_control`] is the content-control half of "is this
//! operation inside a form field": a text-taking control (rich text, plain text,
//! combo box, date) that is not content-locked and whose role, if the reader
//! fills as one, is theirs.
//!
//! # Complexity
//!
//! Each method resolves its control once — O(document) for a lookup by id, the
//! same cost `find_form_checkbox` pays — and rewrites one paragraph or one
//! block. Nothing here runs per keystroke except [`refuse_if_locked`] and
//! [`fills_a_control`], which resolve only the paragraphs the operations name:
//! O(those paragraphs' inlines).

use std::collections::BTreeSet;
use std::str::FromStr;

use casual_doc_edit::{Operation, Pos, find_paragraph_any, inline_text_len, surface_block_lists};
use casual_doc_layout::flow::node_plain_text;
use casual_doc_model::v1::{
    BlockNode, BlockSdt, Color, Document, InlineNode, InlineSdt, Paragraph, RgbColor, Run,
    RunProperties, SdtCheckbox, SdtCheckboxSymbol, SdtControlData, SdtControlKind, SdtDate,
    SdtListItem, SdtLock, SdtProperties, Symbol,
};
use casual_doc_model::{IdGenerator, NodeId};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::form_roles::FormRoles;
use crate::{EditResult, HistoryKind, WasmDocument, to_js};

/// The longest title, tag or placeholder this editor writes (the model's own
/// bound on `w:alias` / `w:tag`).
pub const MAX_PROPERTY_LEN: usize = 255;
/// The most list entries a drop-down or combo box may carry.
pub const MAX_LIST_ITEMS: usize = 1_000;

/// Word's check box glyphs and their font.
const CHECKED_GLYPH: &str = "2612";
const UNCHECKED_GLYPH: &str = "2610";
const CHECKBOX_FONT: &str = "MS Gothic";

/// Word's Placeholder Text style colour, drawn on placeholder content.
const PLACEHOLDER_GREY: RgbColor = RgbColor {
    r: 0x80,
    g: 0x80,
    b: 0x80,
};

/// The refusal for a straddling selection.
const STRADDLE: &str = "A content control can't start or end inside a link, a field or another control. Select the whole of it, or none of it.";

fn refusal(code: &str, sentence: &str) -> String {
    casual_doc_edit::refusal::marked(code, sentence)
}

/// The kind token the host speaks.
const fn kind_token(kind: Option<SdtControlKind>) -> &'static str {
    match kind {
        // No type marker is the OOXML default: rich text.
        None | Some(SdtControlKind::RichText) => "richText",
        Some(SdtControlKind::PlainText) => "plainText",
        Some(SdtControlKind::ComboBox) => "comboBox",
        Some(SdtControlKind::DropDownList) => "dropDownList",
        Some(SdtControlKind::Date) => "date",
        Some(SdtControlKind::Picture) => "picture",
        Some(SdtControlKind::Checkbox) => "checkbox",
        Some(SdtControlKind::Group) => "group",
        Some(SdtControlKind::BuildingBlockGallery) => "buildingBlock",
        Some(SdtControlKind::RepeatingSection) => "repeatingSection",
        Some(SdtControlKind::Citation) => "citation",
        Some(SdtControlKind::Bibliography) => "bibliography",
    }
}

/// The seven kinds Word's Developer tab inserts.
fn insertable_kind(token: &str) -> Result<SdtControlKind, String> {
    Ok(match token {
        "richText" => SdtControlKind::RichText,
        "plainText" => SdtControlKind::PlainText,
        "comboBox" => SdtControlKind::ComboBox,
        "dropDownList" => SdtControlKind::DropDownList,
        "date" => SdtControlKind::Date,
        "picture" => SdtControlKind::Picture,
        "checkbox" => SdtControlKind::Checkbox,
        _ => {
            return Err(refusal(
                "content-control.unknown-kind",
                &format!("\u{201c}{token}\u{201d} isn't a kind of content control."),
            ));
        }
    })
}

const fn lock_token(lock: Option<SdtLock>) -> &'static str {
    match lock {
        None | Some(SdtLock::Unlocked) => "unlocked",
        Some(SdtLock::SdtLocked) => "sdtLocked",
        Some(SdtLock::ContentLocked) => "contentLocked",
        Some(SdtLock::SdtContentLocked) => "sdtContentLocked",
    }
}

/// `w:lock` from the dialog's two boxes.
const fn lock_of(control: bool, content: bool) -> Option<SdtLock> {
    match (control, content) {
        (false, false) => None,
        (true, false) => Some(SdtLock::SdtLocked),
        (false, true) => Some(SdtLock::ContentLocked),
        (true, true) => Some(SdtLock::SdtContentLocked),
    }
}

/// The contents may not be edited.
const fn content_locked(properties: &SdtProperties) -> bool {
    matches!(
        properties.lock,
        Some(SdtLock::ContentLocked | SdtLock::SdtContentLocked)
    )
}

/// The control may not be deleted.
const fn control_locked(properties: &SdtProperties) -> bool {
    matches!(
        properties.lock,
        Some(SdtLock::SdtLocked | SdtLock::SdtContentLocked)
    )
}

/// Whether typing goes into this kind of control. A drop-down list is chosen
/// from, a check box ticked and a picture replaced, so none of them take keys.
const fn takes_text(kind: Option<SdtControlKind>) -> bool {
    matches!(
        kind,
        None | Some(
            SdtControlKind::RichText
                | SdtControlKind::PlainText
                | SdtControlKind::ComboBox
                | SdtControlKind::Date
        )
    )
}

/// Whether formatting may change inside: rich text only, as in Word.
const fn takes_formatting(kind: Option<SdtControlKind>) -> bool {
    matches!(kind, None | Some(SdtControlKind::RichText))
}

// ---- What the host reads ------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ItemJson {
    display: String,
    value: String,
}

/// One control, as the host sees it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ControlJson {
    id: String,
    /// `inline` (around runs) or `block` (around paragraphs).
    scope: &'static str,
    kind: &'static str,
    title: Option<String>,
    tag: Option<String>,
    control_id: Option<String>,
    color: Option<String>,
    lock: &'static str,
    showing_placeholder: bool,
    temporary: bool,
    items: Vec<ItemJson>,
    date_format: Option<String>,
    full_date: Option<String>,
    checked: Option<bool>,
    /// The control's text, at most 200 characters.
    text: String,
    /// Where the caret goes to reach it: the paragraph, and for an inline
    /// control its span in that paragraph's text.
    paragraph: Option<String>,
    start: u32,
    end: u32,
    /// The role the sidecar assigns it, if any.
    role: Option<String>,
    /// Whether the reader may fill it now: the lock and their role allow it.
    fillable: bool,
}

fn control_json(
    id: NodeId,
    scope: &'static str,
    properties: &SdtProperties,
    text: &str,
    place: (Option<NodeId>, u32, u32),
    roles: &FormRoles,
) -> ControlJson {
    let (items, date_format, full_date, checked) = match &properties.data {
        Some(SdtControlData::List(items)) => (
            items
                .iter()
                .map(|item| ItemJson {
                    display: item.display.clone().unwrap_or_else(|| item.value.clone()),
                    value: item.value.clone(),
                })
                .collect(),
            None,
            None,
            None,
        ),
        Some(SdtControlData::Date(date)) => (
            Vec::new(),
            date.date_format.clone(),
            date.full_date.clone(),
            None,
        ),
        Some(SdtControlData::Checkbox(checkbox)) => {
            (Vec::new(), None, None, Some(checkbox.checked))
        }
        None => (Vec::new(), None, None, None),
    };
    let control_id = properties.control_id.as_deref();
    ControlJson {
        id: id.to_string(),
        scope,
        kind: kind_token(properties.control_kind),
        title: properties.alias.clone(),
        tag: properties.tag.clone(),
        control_id: properties.control_id.clone(),
        color: properties.color.clone(),
        lock: lock_token(properties.lock),
        showing_placeholder: properties.showing_placeholder,
        temporary: properties.temporary,
        items,
        date_format,
        full_date,
        checked,
        text: text.chars().take(200).collect(),
        paragraph: place.0.map(|node| node.to_string()),
        start: place.1,
        end: place.2,
        role: roles.role_of(control_id).map(str::to_owned),
        fillable: !content_locked(properties) && roles.may_fill(control_id),
    }
}

// ---- Finding controls ---------------------------------------------------------

/// Where a block control sits, for `DeleteBlocks` / `InsertBlocks`.
#[derive(Clone, Copy, Debug)]
struct BlockSite {
    /// `None` for the body, else the owning cell or content control.
    container: Option<NodeId>,
    index: u32,
}

/// Every control in document order, with where it is.
fn collect_controls(document: &Document, roles: &FormRoles) -> Vec<ControlJson> {
    fn inlines(
        list: &[InlineNode],
        paragraph: NodeId,
        offset: &mut u32,
        roles: &FormRoles,
        out: &mut Vec<ControlJson>,
    ) {
        for inline in list {
            let start = *offset;
            let end = start + inline_text_len(inline);
            match inline {
                InlineNode::Sdt(sdt) => {
                    out.push(control_json(
                        sdt.id,
                        "inline",
                        &sdt.properties,
                        &node_plain_text(&sdt.inlines),
                        (Some(paragraph), start, end),
                        roles,
                    ));
                    let mut inner = start;
                    inlines(&sdt.inlines, paragraph, &mut inner, roles, out);
                }
                InlineNode::Hyperlink(link) => {
                    let mut inner = start;
                    inlines(&link.inlines, paragraph, &mut inner, roles, out);
                }
                _ => {}
            }
            *offset = end;
        }
    }
    fn blocks(list: &[BlockNode], roles: &FormRoles, out: &mut Vec<ControlJson>) {
        for block in list {
            match block {
                BlockNode::Paragraph(paragraph) => {
                    inlines(&paragraph.inlines, paragraph.id, &mut 0, roles, out);
                }
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            blocks(&cell.blocks, roles, out);
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    out.push(control_json(
                        sdt.id,
                        "block",
                        &sdt.properties,
                        &block_text(&sdt.blocks),
                        (first_paragraph(&sdt.blocks), 0, 0),
                        roles,
                    ));
                    blocks(&sdt.blocks, roles, out);
                }
                BlockNode::AltChunk(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    for surface in surface_block_lists(document) {
        blocks(surface, roles, &mut out);
    }
    out
}

fn first_paragraph(blocks: &[BlockNode]) -> Option<NodeId> {
    blocks.iter().find_map(|block| match block {
        BlockNode::Paragraph(paragraph) => Some(paragraph.id),
        BlockNode::Table(table) => table
            .rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .find_map(|cell| first_paragraph(&cell.blocks)),
        BlockNode::Sdt(sdt) => first_paragraph(&sdt.blocks),
        BlockNode::AltChunk(_) => None,
    })
}

fn block_text(blocks: &[BlockNode]) -> String {
    let mut out = String::new();
    for block in blocks {
        if out.len() > 400 {
            break;
        }
        match block {
            BlockNode::Paragraph(paragraph) => {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&node_plain_text(&paragraph.inlines));
            }
            BlockNode::Sdt(sdt) => out.push_str(&block_text(&sdt.blocks)),
            BlockNode::Table(_) | BlockNode::AltChunk(_) => {}
        }
    }
    out
}

/// The innermost inline control whose span covers `offset` (ends inclusive),
/// with its span. O(the paragraph's inlines).
fn inline_control_at(
    inlines: &[InlineNode],
    offset: u32,
    seen: &mut u32,
) -> Option<(InlineSdt, u32, u32)> {
    for inline in inlines {
        let start = *seen;
        let end = start + inline_text_len(inline);
        match inline {
            InlineNode::Sdt(sdt) if offset >= start && offset <= end => {
                let mut inner = start;
                if let Some(found) = inline_control_at(&sdt.inlines, offset, &mut inner) {
                    return Some(found);
                }
                return Some(((**sdt).clone(), start, end));
            }
            InlineNode::Hyperlink(link) if offset >= start && offset <= end => {
                let mut inner = start;
                if let Some(found) = inline_control_at(&link.inlines, offset, &mut inner) {
                    return Some(found);
                }
            }
            _ => {}
        }
        *seen = end;
    }
    None
}

/// Where the inline control `id` starts in `inlines` (a byte offset in the
/// paragraph, counted as `collect_controls` counts it), or `None`. Its start is
/// inside it for `contentControlAt`, whose spans are end-inclusive. O(the
/// paragraph's inlines).
fn inline_control_start(inlines: &[InlineNode], id: NodeId, seen: &mut u32) -> Option<u32> {
    for inline in inlines {
        let start = *seen;
        let end = start + inline_text_len(inline);
        match inline {
            InlineNode::Sdt(sdt) => {
                if sdt.id == id {
                    return Some(start);
                }
                let mut inner = start;
                if let Some(found) = inline_control_start(&sdt.inlines, id, &mut inner) {
                    return Some(found);
                }
            }
            InlineNode::Hyperlink(link) => {
                let mut inner = start;
                if let Some(found) = inline_control_start(&link.inlines, id, &mut inner) {
                    return Some(found);
                }
            }
            _ => {}
        }
        *seen = end;
    }
    None
}

/// The innermost block control holding `paragraph`, with its site, in the body.
/// O(body blocks).
fn block_control_holding(document: &Document, paragraph: NodeId) -> Option<(BlockSdt, BlockSite)> {
    fn holds(blocks: &[BlockNode], paragraph: NodeId) -> bool {
        blocks.iter().any(|block| match block {
            BlockNode::Paragraph(candidate) => candidate.id == paragraph,
            BlockNode::Table(table) => table
                .rows
                .iter()
                .any(|row| row.cells.iter().any(|cell| holds(&cell.blocks, paragraph))),
            BlockNode::Sdt(sdt) => holds(&sdt.blocks, paragraph),
            BlockNode::AltChunk(_) => false,
        })
    }
    fn walk(
        blocks: &[BlockNode],
        container: Option<NodeId>,
        paragraph: NodeId,
    ) -> Option<(BlockSdt, BlockSite)> {
        for (index, block) in blocks.iter().enumerate() {
            match block {
                BlockNode::Sdt(sdt) if holds(&sdt.blocks, paragraph) => {
                    return walk(&sdt.blocks, Some(sdt.id), paragraph).or_else(|| {
                        Some((
                            (**sdt).clone(),
                            BlockSite {
                                container,
                                index: u32::try_from(index).unwrap_or(u32::MAX),
                            },
                        ))
                    });
                }
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            if holds(&cell.blocks, paragraph) {
                                return walk(&cell.blocks, Some(cell.id), paragraph);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }
    walk(document.body(), None, paragraph)
}

/// A block control by id, with its site, in the body. O(body blocks).
fn block_control(document: &Document, id: NodeId) -> Option<(BlockSdt, BlockSite)> {
    fn walk(
        blocks: &[BlockNode],
        container: Option<NodeId>,
        id: NodeId,
    ) -> Option<(BlockSdt, BlockSite)> {
        for (index, block) in blocks.iter().enumerate() {
            match block {
                BlockNode::Sdt(sdt) if sdt.id == id => {
                    return Some((
                        (**sdt).clone(),
                        BlockSite {
                            container,
                            index: u32::try_from(index).unwrap_or(u32::MAX),
                        },
                    ));
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(found) = walk(&sdt.blocks, Some(sdt.id), id) {
                        return Some(found);
                    }
                }
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            if let Some(found) = walk(&cell.blocks, Some(cell.id), id) {
                                return Some(found);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }
    walk(document.body(), None, id)
}

/// Whether `inlines` hold the inline control `id`, at any depth.
fn holds_inline_control(inlines: &[InlineNode], id: NodeId) -> bool {
    inlines.iter().any(|inline| match inline {
        InlineNode::Sdt(sdt) => sdt.id == id || holds_inline_control(&sdt.inlines, id),
        InlineNode::Hyperlink(link) => holds_inline_control(&link.inlines, id),
        _ => false,
    })
}

/// The paragraph holding the inline control `id`, on any surface. O(document).
fn paragraph_of_inline_control(document: &Document, id: NodeId) -> Option<&Paragraph> {
    let mut found = None;
    crate::visit_paragraphs_in(document, &mut |paragraph| {
        if found.is_none() && holds_inline_control(&paragraph.inlines, id) {
            found = Some(paragraph.id);
        }
    });
    found.and_then(|paragraph| find_paragraph_any(document, paragraph))
}

/// Runs `edit` on the inline control `id` inside `inlines`. True when found.
fn with_inline_control(
    inlines: &mut [InlineNode],
    id: NodeId,
    edit: &mut dyn FnMut(&mut InlineSdt),
) -> bool {
    for inline in inlines {
        match inline {
            InlineNode::Sdt(sdt) if sdt.id == id => {
                edit(sdt);
                return true;
            }
            InlineNode::Sdt(sdt) => {
                if with_inline_control(&mut sdt.inlines, id, edit) {
                    return true;
                }
            }
            InlineNode::Hyperlink(link) => {
                if with_inline_control(&mut link.inlines, id, edit) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Replaces the inline control `id` with its own content, wherever it is in
/// `inlines`, and re-joins the runs that meet at the two seams. True when found.
fn unwrap_inline_control(inlines: &mut Vec<InlineNode>, id: NodeId) -> bool {
    if let Some(index) = inlines
        .iter()
        .position(|inline| matches!(inline, InlineNode::Sdt(sdt) if sdt.id == id))
    {
        let InlineNode::Sdt(sdt) = inlines.remove(index) else {
            return false;
        };
        let content = sdt.inlines;
        let count = content.len();
        inlines.splice(index..index, content);
        coalesce_runs(inlines, index.saturating_sub(1), index + count);
        return true;
    }
    for inline in inlines.iter_mut() {
        match inline {
            InlineNode::Sdt(sdt) => {
                if unwrap_inline_control(&mut sdt.inlines, id) {
                    return true;
                }
            }
            InlineNode::Hyperlink(link) => {
                if unwrap_inline_control(&mut link.inlines, id) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Joins adjacent runs with equal properties in `inlines[from..=to]`, which the
/// model forbids leaving apart. O(the span).
fn coalesce_runs(inlines: &mut Vec<InlineNode>, from: usize, to: usize) {
    let mut index = from;
    let mut to = to.min(inlines.len().saturating_sub(1));
    while index < to && index + 1 < inlines.len() {
        let joinable = matches!(
            (&inlines[index], &inlines[index + 1]),
            (InlineNode::Run(a), InlineNode::Run(b)) if a.properties == b.properties
        );
        if joinable {
            let InlineNode::Run(next) = inlines.remove(index + 1) else {
                return;
            };
            if let InlineNode::Run(run) = &mut inlines[index] {
                run.text.push_str(&next.text);
            }
            to = to.saturating_sub(1);
        } else {
            index += 1;
        }
    }
}

/// Splits a paragraph's TOP-LEVEL inlines at byte `offset`. A run straddling it
/// is split (the tail takes a fresh id); anything else straddling it is the
/// [`STRADDLE`] refusal. O(the inlines).
fn split_top(
    inlines: Vec<InlineNode>,
    offset: u32,
    ids: &mut IdGenerator,
) -> Result<(Vec<InlineNode>, Vec<InlineNode>), String> {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut seen = 0_u32;
    for inline in inlines {
        let length = inline_text_len(&inline);
        if seen >= offset {
            right.push(inline);
        } else if seen + length <= offset {
            left.push(inline);
        } else {
            let InlineNode::Run(run) = inline else {
                return Err(refusal("content-control.straddle", STRADDLE));
            };
            let at = (offset - seen) as usize;
            if !run.text.is_char_boundary(at) {
                return Err(refusal("content-control.straddle", STRADDLE));
            }
            let (head, tail) = run.text.split_at(at);
            let tail_id = ids.next_id().map_err(|_| "id space exhausted".to_owned())?;
            left.push(InlineNode::Run(Run {
                id: run.id,
                properties: run.properties.clone(),
                text: head.to_owned(),
            }));
            right.push(InlineNode::Run(Run {
                id: tail_id,
                properties: run.properties,
                text: tail.to_owned(),
            }));
        }
        seen += length;
    }
    Ok((left, right))
}

/// The run formatting at `offset`: the run ending at or covering it, else the
/// next one. What a new control's content is formatted with.
fn formatting_at(inlines: &[InlineNode], offset: u32) -> RunProperties {
    let mut seen = 0_u32;
    let mut before = None;
    for inline in inlines {
        let length = inline_text_len(inline);
        if let InlineNode::Run(run) = inline {
            if seen < offset || (seen == offset && before.is_none() && length > 0) {
                before = Some(run.properties.get().clone());
            }
            if seen >= offset && before.is_some() {
                break;
            }
        }
        seen += length;
    }
    before.unwrap_or_default()
}

fn placeholder_run(id: NodeId, text: &str, base: &RunProperties) -> InlineNode {
    let mut properties = base.clone();
    properties.color = Some(Color::Rgb(PLACEHOLDER_GREY));
    InlineNode::Run(Run {
        id,
        properties: properties.into(),
        text: text.to_owned(),
    })
}

/// Every `w:id` already in use. O(document).
fn control_ids_in_use(document: &Document) -> BTreeSet<String> {
    fn inlines(list: &[InlineNode], out: &mut BTreeSet<String>) {
        for inline in list {
            match inline {
                InlineNode::Sdt(sdt) => {
                    if let Some(id) = &sdt.properties.control_id {
                        out.insert(id.clone());
                    }
                    inlines(&sdt.inlines, out);
                }
                InlineNode::Hyperlink(link) => inlines(&link.inlines, out),
                _ => {}
            }
        }
    }
    fn blocks(list: &[BlockNode], out: &mut BTreeSet<String>) {
        for block in list {
            match block {
                BlockNode::Paragraph(paragraph) => inlines(&paragraph.inlines, out),
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            blocks(&cell.blocks, out);
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(id) = &sdt.properties.control_id {
                        out.insert(id.clone());
                    }
                    blocks(&sdt.blocks, out);
                }
                BlockNode::AltChunk(_) => {}
            }
        }
    }
    let mut out = BTreeSet::new();
    for surface in surface_block_lists(document) {
        blocks(surface, &mut out);
    }
    out
}

/// A fresh `w:id`: a positive 31-bit integer, as Word writes, derived from the
/// node's identity (so the same edit mints the same id) and stepped past any in
/// use. O(log ids) per step.
fn fresh_control_id(in_use: &BTreeSet<String>, seed: NodeId) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.to_string().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let mut candidate = (hash % 2_000_000_000) + 1;
    while in_use.contains(&candidate.to_string()) {
        candidate = candidate % 2_000_000_000 + 1;
    }
    candidate.to_string()
}

// ---- The choke-point checks ---------------------------------------------------

/// Where an operation writes, as paragraph spans `(paragraph, start, end)`, for
/// the lock and the forms checks. `None` when it is not addressed to text.
fn operation_spans(op: &Operation) -> Option<Vec<(NodeId, u32, u32)>> {
    Some(match op {
        Operation::InsertText { at, .. }
        | Operation::SplitParagraph { at, .. }
        | Operation::InsertInlineObject { at, .. } => vec![(at.node, at.offset, at.offset)],
        Operation::DeleteText { range }
        | Operation::FormatText { range, .. }
        | Operation::ClearFormatting { range }
        | Operation::SetHyperlink { range, .. } => {
            if range.start.node == range.end.node {
                vec![(range.start.node, range.start.offset, range.end.offset)]
            } else {
                vec![
                    (range.start.node, range.start.offset, u32::MAX),
                    (range.end.node, 0, range.end.offset),
                ]
            }
        }
        _ => return None,
    })
}

/// Every inline control in `inlines` with its span, at any depth.
fn spans_of(inlines: &[InlineNode], seen: &mut u32, out: &mut Vec<(SdtProperties, u32, u32)>) {
    for inline in inlines {
        let start = *seen;
        let end = start + inline_text_len(inline);
        match inline {
            InlineNode::Sdt(sdt) => {
                out.push((sdt.properties.clone(), start, end));
                let mut inner = start;
                spans_of(&sdt.inlines, &mut inner, out);
            }
            InlineNode::Hyperlink(link) => {
                let mut inner = start;
                spans_of(&link.inlines, &mut inner, out);
            }
            _ => {}
        }
        *seen = end;
    }
}

/// Refuses an operation a content control's `w:lock` forbids: an edit inside a
/// content-locked control, or a deletion that removes a control locked against
/// deletion. O(the inlines of the paragraphs `ops` name), plus one body walk per
/// paragraph only when the document has a block control (a paragraph inside a
/// content-locked block control is locked too).
///
/// # Errors
/// The refusal naming which lock refused.
pub(crate) fn refuse_if_locked(document: &Document, ops: &[Operation]) -> Result<(), String> {
    let has_block_controls = document
        .body()
        .iter()
        .any(|block| matches!(block, BlockNode::Sdt(_)));
    for op in ops {
        let Some(spans) = operation_spans(op) else {
            continue;
        };
        let deleting = matches!(op, Operation::DeleteText { .. });
        for (node, start, end) in spans {
            let Some(paragraph) = find_paragraph_any(document, node) else {
                continue;
            };
            let mut controls = Vec::new();
            spans_of(&paragraph.inlines, &mut 0, &mut controls);
            for (properties, from, to) in controls {
                // Strictly inside, or a non-empty range overlapping the interior.
                let touches = if start == end {
                    start > from && start < to
                } else {
                    start < to && end > from
                };
                if touches && content_locked(&properties) {
                    return Err(refusal(
                        "content-control.content-locked",
                        "This content control's contents are locked. Clear \u{201c}Contents cannot be edited\u{201d} in its properties first.",
                    ));
                }
                if deleting && start <= from && end >= to && control_locked(&properties) {
                    return Err(refusal(
                        "content-control.control-locked",
                        "This content control can't be deleted. Clear \u{201c}Content control cannot be deleted\u{201d} in its properties first.",
                    ));
                }
            }
            if has_block_controls
                && let Some((sdt, _)) = block_control_holding(document, node)
                && content_locked(&sdt.properties)
            {
                return Err(refusal(
                    "content-control.content-locked",
                    "This content control's contents are locked. Clear \u{201c}Contents cannot be edited\u{201d} in its properties first.",
                ));
            }
        }
    }
    Ok(())
}

/// Whether `op` writes only inside a content control a reader may fill under
/// forms protection: a text-taking kind, not content-locked, and — when the
/// reader fills as a role — assigned to no role or to theirs. Formatting, a
/// paragraph break or an object is allowed only in rich text. O(the inlines of
/// the paragraphs `op` names).
pub(crate) fn fills_a_control(document: &Document, roles: &FormRoles, op: &Operation) -> bool {
    let formatting = matches!(
        op,
        Operation::FormatText { .. }
            | Operation::ClearFormatting { .. }
            | Operation::SetHyperlink { .. }
            | Operation::InsertInlineObject { .. }
    );
    let open = |properties: &SdtProperties| {
        takes_text(properties.control_kind)
            && (!formatting || takes_formatting(properties.control_kind))
            && !content_locked(properties)
            && roles.may_fill(properties.control_id.as_deref())
    };
    // A paragraph break, or a range across paragraphs, only inside one block
    // rich text control.
    let block_open = |paragraph: NodeId| {
        block_control_holding(document, paragraph).filter(|(sdt, _)| {
            open(&sdt.properties) && takes_formatting(sdt.properties.control_kind)
        })
    };
    if let Operation::SplitParagraph { at, .. } = op {
        return block_open(at.node).is_some();
    }
    if let Operation::JoinParagraphs { first, second, .. } = op {
        return match (block_open(*first), block_open(*second)) {
            (Some((a, _)), Some((b, _))) => a.id == b.id,
            _ => false,
        };
    }
    let Some(spans) = operation_spans(op) else {
        return false;
    };
    if spans.len() == 2 {
        return match (block_open(spans[0].0), block_open(spans[1].0)) {
            (Some((a, _)), Some((b, _))) => a.id == b.id,
            _ => false,
        };
    }
    let (node, start, end) = spans[0];
    let Some(paragraph) = find_paragraph_any(document, node) else {
        return false;
    };
    let mut controls = Vec::new();
    spans_of(&paragraph.inlines, &mut 0, &mut controls);
    // The innermost control covering the whole span decides.
    let inner = controls
        .iter()
        .filter(|(_, from, to)| start >= *from && end <= *to)
        .min_by_key(|(_, from, to)| to - from);
    match inner {
        Some((properties, ..)) => open(properties),
        None => block_control_holding(document, node).is_some_and(|(sdt, _)| open(&sdt.properties)),
    }
}

// ---- The dialog's patch -------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ItemPatch {
    #[serde(default)]
    display: String,
    #[serde(default)]
    value: String,
}

/// What the properties dialog sends. Every field is optional: absent leaves it.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PropertiesPatch {
    title: Option<String>,
    tag: Option<String>,
    /// Six hex digits, or empty for none.
    color: Option<String>,
    temporary: Option<bool>,
    lock_control: Option<bool>,
    lock_content: Option<bool>,
    /// Rewrites the content while the control is showing its placeholder.
    placeholder: Option<String>,
    items: Option<Vec<ItemPatch>>,
    date_format: Option<String>,
    /// The role, or empty for anyone. Applied to the sidecar, not the document.
    role: Option<String>,
}

fn bounded(value: &str, what: &str) -> Result<Option<String>, String> {
    let value = value.trim();
    if value.len() > MAX_PROPERTY_LEN {
        return Err(refusal(
            "content-control.too-long",
            &format!("The {what} can be at most {MAX_PROPERTY_LEN} characters."),
        ));
    }
    Ok((!value.is_empty()).then(|| value.to_owned()))
}

/// Applies `patch` to `properties`. The content is not touched here.
fn apply_patch(properties: &mut SdtProperties, patch: &PropertiesPatch) -> Result<(), String> {
    if let Some(title) = &patch.title {
        properties.alias = bounded(title, "title")?;
    }
    if let Some(tag) = &patch.tag {
        properties.tag = bounded(tag, "tag")?;
    }
    if let Some(color) = &patch.color {
        let color = color.trim().trim_start_matches('#').to_ascii_uppercase();
        if color.is_empty() {
            properties.color = None;
        } else if SdtProperties::is_valid_color(&color) {
            properties.color = Some(color);
        } else {
            return Err(refusal(
                "content-control.bad-colour",
                &format!("\u{201c}{color}\u{201d} is not a colour."),
            ));
        }
    }
    if let Some(temporary) = patch.temporary {
        properties.temporary = temporary;
    }
    if patch.lock_control.is_some() || patch.lock_content.is_some() {
        properties.lock = lock_of(
            patch
                .lock_control
                .unwrap_or_else(|| control_locked(properties)),
            patch
                .lock_content
                .unwrap_or_else(|| content_locked(properties)),
        );
    }
    if let Some(items) = &patch.items {
        if !matches!(
            properties.control_kind,
            Some(SdtControlKind::ComboBox | SdtControlKind::DropDownList)
        ) {
            return Err(refusal(
                "content-control.no-list",
                "Only a drop-down list or a combo box has list entries.",
            ));
        }
        if items.len() > MAX_LIST_ITEMS {
            return Err(refusal(
                "content-control.too-many-items",
                &format!("A list can have at most {MAX_LIST_ITEMS} entries."),
            ));
        }
        let mut list = Vec::with_capacity(items.len());
        for item in items {
            let display = bounded(&item.display, "entry")?;
            let value = bounded(&item.value, "value")?;
            if display.is_none() && value.is_none() {
                continue;
            }
            list.push(SdtListItem {
                display,
                value: value.unwrap_or_default(),
            });
        }
        properties.data = Some(SdtControlData::List(list));
    }
    if let Some(format) = &patch.date_format {
        let Some(SdtControlData::Date(date)) = properties.data.as_mut() else {
            return Err(refusal(
                "content-control.no-date",
                "Only a date picker has a date format.",
            ));
        };
        date.date_format = bounded(format, "date format")?;
    }
    Ok(())
}

/// Rewrites a control's content to `text` in one run formatted like `base`.
/// `placeholder` greys it and sets `w:showingPlcHdr`. The first existing node id
/// is kept so the content keeps its identity; an empty control mints one.
fn rewrite_text(
    properties: &mut SdtProperties,
    content: &mut Vec<InlineNode>,
    text: &str,
    placeholder: bool,
    ids: &mut IdGenerator,
) -> Result<(), String> {
    let base = properties
        .run_properties
        .clone()
        .or_else(|| {
            content.iter().find_map(|inline| match inline {
                InlineNode::Run(run) => Some(run.properties.get().clone()),
                _ => None,
            })
        })
        .unwrap_or_default();
    let mut base = base;
    if !placeholder && base.color == Some(Color::Rgb(PLACEHOLDER_GREY)) {
        base.color = None;
    }
    let id = match content.iter().find_map(|inline| match inline {
        InlineNode::Run(run) => Some(run.id),
        _ => None,
    }) {
        Some(id) => id,
        None => ids.next_id().map_err(|_| "id space exhausted".to_owned())?,
    };
    content.clear();
    content.push(if placeholder {
        placeholder_run(id, text, &base)
    } else {
        InlineNode::Run(Run {
            id,
            properties: base.into(),
            text: text.to_owned(),
        })
    });
    properties.showing_placeholder = placeholder;
    Ok(())
}

/// What a fill sends: the text to show, and for a date its ISO date.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ValuePatch {
    text: String,
    full_date: Option<String>,
}

// ---- The bound methods --------------------------------------------------------

#[wasm_bindgen]
impl WasmDocument {
    /// Every content control in the document, in document order, as a JSON
    /// array (`ControlJson`: id, scope, kind, title, tag, lock, list entries,
    /// date format, checked state, text, where it is, its role and whether the
    /// reader may fill it). O(document).
    #[wasm_bindgen(js_name = contentControls)]
    #[must_use]
    pub fn content_controls(&self) -> String {
        serde_json::to_string(&collect_controls(&self.document, &self.form_roles))
            .unwrap_or_else(|_| "[]".to_owned())
    }

    /// The innermost content control at a caret position (a paragraph and a
    /// byte offset, as a hit test returns), or `undefined`. An inline control
    /// counts from its start to its end inclusive; a paragraph inside a block
    /// control is in it. O(the paragraph's inlines), plus one body walk when no
    /// inline control holds the caret.
    #[wasm_bindgen(js_name = contentControlAt)]
    #[must_use]
    pub fn content_control_at(&self, node: &str, offset: u32) -> Option<String> {
        let paragraph = NodeId::from_str(node).ok()?;
        let found = find_paragraph_any(&self.document, paragraph)?;
        let json =
            if let Some((sdt, start, end)) = inline_control_at(&found.inlines, offset, &mut 0) {
                control_json(
                    sdt.id,
                    "inline",
                    &sdt.properties,
                    &node_plain_text(&sdt.inlines),
                    (Some(paragraph), start, end),
                    &self.form_roles,
                )
            } else {
                let (sdt, _) = block_control_holding(&self.document, paragraph)?;
                control_json(
                    sdt.id,
                    "block",
                    &sdt.properties,
                    &block_text(&sdt.blocks),
                    (first_paragraph(&sdt.blocks), 0, 0),
                    &self.form_roles,
                )
            };
        serde_json::to_string(&json).ok()
    }

    /// Inserts a content control of `kind` (`richText`, `plainText`,
    /// `comboBox`, `dropDownList`, `date`, `picture`, `checkbox`) over the
    /// selection `start..end` in paragraph `node`, as one undoable action.
    ///
    /// A text kind wraps the selection; with none it holds `placeholder`
    /// (Word's localised prompt, which the host supplies) flagged as showing its
    /// placeholder. A check box or a picture is inserted at the selection's end
    /// and leaves the selected text alone.
    ///
    /// # Errors
    /// An unknown kind, a range outside the paragraph, a selection whose ends
    /// fall inside a link, a field or another control, or a picture without a
    /// placeholder image.
    #[wasm_bindgen(js_name = insertContentControl)]
    pub fn insert_content_control(
        &mut self,
        node: &str,
        start: u32,
        end: u32,
        kind: &str,
        placeholder: &str,
    ) -> Result<EditResult, JsValue> {
        self.insert_content_control_inner(node, start, end, kind, placeholder, None)
            .map_err(to_js)
    }

    /// Inserts a picture content control holding `bytes` as its placeholder
    /// picture, at `offset` in paragraph `node`. One undoable action, the media
    /// registration included.
    ///
    /// # Errors
    /// As [`Self::insert_content_control`], or an image the engine cannot hold.
    #[wasm_bindgen(js_name = insertPictureContentControl)]
    pub fn insert_picture_content_control(
        &mut self,
        node: &str,
        offset: u32,
        bytes: Vec<u8>,
        width_emu: f64,
        height_emu: f64,
        mime: &str,
    ) -> Result<EditResult, JsValue> {
        self.insert_content_control_inner(
            node,
            offset,
            offset,
            "picture",
            "",
            Some((bytes, width_emu, height_emu, mime)),
        )
        .map_err(to_js)
    }

    /// Applies the properties dialog's `patch` (JSON: `title`, `tag`, `color`,
    /// `temporary`, `lockControl`, `lockContent`, `placeholder`, `items`,
    /// `dateFormat`, `role`) to the control `id`, as one undoable action. A
    /// control without a `w:id` is given one, so a role can name it. The role
    /// goes to the sidecar (`form_roles.rs`), not the document.
    ///
    /// # Errors
    /// No such control, a value over its bound, list entries on a control that
    /// has none, a date format on one that is not a date picker, or an unknown
    /// role.
    #[wasm_bindgen(js_name = setContentControlProperties)]
    pub fn set_content_control_properties(
        &mut self,
        id: &str,
        patch: &str,
    ) -> Result<EditResult, JsValue> {
        self.set_content_control_properties_inner(id, patch)
            .map_err(to_js)
    }

    /// Word's Remove Content Control: the control goes and its content stays,
    /// as one undoable action.
    ///
    /// # Errors
    /// No such control, or one locked against deletion.
    #[wasm_bindgen(js_name = removeContentControl)]
    pub fn remove_content_control(&mut self, id: &str) -> Result<EditResult, JsValue> {
        self.remove_content_control_inner(id).map_err(to_js)
    }

    /// Fills the control `id` with a chosen or typed value — a list entry, a
    /// date (`fullDate`, ISO, and the `text` the host formatted from the
    /// control's date format), or the first keystroke over a placeholder — as
    /// one undoable action. Allowed under forms protection, because it IS the
    /// filling of a form field.
    ///
    /// # Errors
    /// No such control, a kind that is not filled with text, a content lock, or
    /// a role that is not the reader's.
    #[wasm_bindgen(js_name = fillContentControl)]
    pub fn fill_content_control(&mut self, id: &str, value: &str) -> Result<EditResult, JsValue> {
        self.fill_content_control_inner(id, value).map_err(to_js)
    }

    /// Replaces a picture control's picture with `bytes`, as one undoable action
    /// that is allowed under forms protection.
    ///
    /// # Errors
    /// No such picture control, a content lock, a role that is not the
    /// reader's, or an image the engine cannot hold.
    #[wasm_bindgen(js_name = fillPictureContentControl)]
    pub fn fill_picture_content_control(
        &mut self,
        id: &str,
        bytes: Vec<u8>,
        width_emu: f64,
        height_emu: f64,
        mime: &str,
    ) -> Result<EditResult, JsValue> {
        self.fill_picture_content_control_inner(id, bytes, width_emu, height_emu, mime)
            .map_err(to_js)
    }
}

/// A picture to place: bytes, width and height in EMU, and its type.
type PictureBytes<'a> = (Vec<u8>, f64, f64, &'a str);

impl WasmDocument {
    fn node(id: &str) -> Result<NodeId, String> {
        NodeId::from_str(id).map_err(|_| format!("invalid node id: {id}"))
    }

    fn no_such_control() -> String {
        refusal(
            "content-control.not-found",
            "That content control is no longer in the document.",
        )
    }

    /// The picture lowering, for a control's placeholder or its fill. Returns
    /// the operations that register the media and the drawing to place.
    fn picture_parts(
        &mut self,
        at: Pos,
        picture: PictureBytes<'_>,
    ) -> Result<(Vec<Operation>, InlineNode), String> {
        let (bytes, width, height, mime) = picture;
        if bytes.is_empty() {
            return Err("the image has no bytes".to_owned());
        }
        let clamp = |value: f64, what: &str| -> Result<i64, String> {
            if value.is_finite() && (1.0..=1.0e9).contains(&value) {
                // Bounded above, so the cast cannot truncate.
                #[allow(clippy::cast_possible_truncation)]
                Ok(value.round() as i64)
            } else {
                Err(format!("{what} is out of bounds"))
            }
        };
        let width = clamp(width, "width")?;
        let height = clamp(height, "height")?;
        let ids = &mut self.edit_ids;
        let picture =
            casual_doc_command::objects::insert_picture(at, mime, width, height, &mut || {
                ids.next_id().ok()
            })?;
        self.resources.insert(picture.part_name, bytes);
        let mut registration = Vec::new();
        let mut drawing = None;
        for op in picture.operations {
            match op {
                Operation::InsertInlineObject { node, .. } => drawing = Some(*node),
                // The aspect lock names a drawing that, inside a control, is
                // placed by `SetInlines`; it is added after the inlines land.
                other => registration.push(other),
            }
        }
        let drawing = drawing.ok_or_else(|| "the picture lowering placed no drawing".to_owned())?;
        Ok((registration, drawing))
    }

    pub(crate) fn insert_content_control_inner(
        &mut self,
        node: &str,
        start: u32,
        end: u32,
        kind: &str,
        placeholder: &str,
        picture: Option<PictureBytes<'_>>,
    ) -> Result<EditResult, String> {
        let kind = insertable_kind(kind)?;
        let paragraph_id = Self::node(node)?;
        let paragraph = find_paragraph_any(&self.document, paragraph_id)
            .ok_or_else(|| format!("no paragraph {node}"))?;
        let length = casual_doc_edit::inlines_text_len(&paragraph.inlines);
        if start > end || end > length {
            return Err(refusal(
                "content-control.bad-range",
                "The selection is outside its paragraph.",
            ));
        }
        let source = paragraph.inlines.clone();
        let base = formatting_at(&source, start);
        let in_use = control_ids_in_use(&self.document);
        let atomic = matches!(kind, SdtControlKind::Checkbox | SdtControlKind::Picture);
        let (from, to) = if atomic { (end, end) } else { (start, end) };

        let (left, rest) = split_top(source, from, &mut self.edit_ids)?;
        let (middle, right) = split_top(rest, to - from, &mut self.edit_ids)?;
        let sdt_id = self
            .edit_ids
            .next_id()
            .map_err(|_| "id space exhausted".to_owned())?;
        let mut properties = SdtProperties {
            control_kind: Some(kind),
            control_id: Some(fresh_control_id(&in_use, sdt_id)),
            run_properties: (base != RunProperties::default()).then(|| base.clone()),
            ..SdtProperties::default()
        };
        let mut registration = Vec::new();
        let mut lock_drawing = None;
        let content = match kind {
            SdtControlKind::Checkbox => {
                properties.data = Some(SdtControlData::Checkbox(SdtCheckbox {
                    checked: false,
                    checked_state: Some(SdtCheckboxSymbol {
                        val: CHECKED_GLYPH.to_owned(),
                        font: Some(CHECKBOX_FONT.to_owned()),
                    }),
                    unchecked_state: Some(SdtCheckboxSymbol {
                        val: UNCHECKED_GLYPH.to_owned(),
                        font: Some(CHECKBOX_FONT.to_owned()),
                    }),
                }));
                let id = self
                    .edit_ids
                    .next_id()
                    .map_err(|_| "id space exhausted".to_owned())?;
                vec![InlineNode::Symbol(Box::new(Symbol {
                    id,
                    font: CHECKBOX_FONT.to_owned(),
                    char: 0x2610,
                    properties: base.clone().into(),
                }))]
            }
            SdtControlKind::Picture => {
                let picture = picture.ok_or_else(|| {
                    refusal(
                        "content-control.picture-needs-image",
                        "A picture content control needs a placeholder picture.",
                    )
                })?;
                let (ops, drawing) = self.picture_parts(Pos::new(paragraph_id, from), picture)?;
                registration = ops;
                lock_drawing = Some(drawing.id());
                properties.showing_placeholder = true;
                vec![drawing]
            }
            _ => {
                if matches!(
                    kind,
                    SdtControlKind::DropDownList | SdtControlKind::ComboBox
                ) {
                    // Word's default entry: the prompt, with an empty value.
                    properties.data = Some(SdtControlData::List(vec![SdtListItem {
                        display: bounded(placeholder, "entry")?,
                        value: String::new(),
                    }]));
                }
                if kind == SdtControlKind::Date {
                    properties.data = Some(SdtControlData::Date(SdtDate {
                        date_format: Some("M/d/yyyy".to_owned()),
                        lid: Some("en-US".to_owned()),
                        store_mapped_as: Some("dateTime".to_owned()),
                        calendar: Some("gregorian".to_owned()),
                        ..SdtDate::default()
                    }));
                }
                if middle.is_empty() {
                    let prompt = placeholder.trim();
                    if prompt.is_empty() {
                        return Err(refusal(
                            "content-control.needs-placeholder",
                            "An empty content control needs placeholder text.",
                        ));
                    }
                    let id = self
                        .edit_ids
                        .next_id()
                        .map_err(|_| "id space exhausted".to_owned())?;
                    properties.showing_placeholder = true;
                    vec![placeholder_run(id, prompt, &base)]
                } else {
                    middle.clone()
                }
            }
        };
        let mut inlines = left;
        if atomic {
            inlines.extend(middle);
        }
        inlines.push(InlineNode::Sdt(Box::new(InlineSdt {
            id: sdt_id,
            properties,
            inlines: content,
        })));
        inlines.extend(right);
        let mut ops = registration;
        ops.push(Operation::SetInlines {
            node: paragraph_id,
            inlines,
        });
        if let Some(drawing) = lock_drawing {
            ops.push(Operation::SetObjectLocks {
                object: drawing,
                locks: casual_doc_model::v1::ObjectLocks {
                    frame: casual_doc_model::v1::LockFlags {
                        no_change_aspect: true,
                        ..casual_doc_model::v1::LockFlags::default()
                    },
                    object: casual_doc_model::v1::LockFlags::default(),
                },
            });
        }
        self.apply_action_caret_as(ops, Pos::new(paragraph_id, from), HistoryKind::Edit)
    }

    pub(crate) fn set_content_control_properties_inner(
        &mut self,
        id: &str,
        patch: &str,
    ) -> Result<EditResult, String> {
        let control = Self::node(id)?;
        let patch: PropertiesPatch = serde_json::from_str(patch)
            .map_err(|error| format!("The properties could not be read: {error}"))?;
        // The role first, on a copy: a bad role refuses before the document moves.
        let role = match patch.role.as_deref().map(str::trim) {
            None => None,
            Some("") => Some(None),
            Some(name) => {
                if !self.form_roles.roles.iter().any(|role| role.name == name) {
                    return Err(refusal(
                        "content-control.unknown-role",
                        &format!("There is no role called \u{201c}{name}\u{201d}."),
                    ));
                }
                Some(Some(name.to_owned()))
            }
        };
        let in_use = control_ids_in_use(&self.document);
        let fresh = fresh_control_id(&in_use, control);
        let mut control_id = None;
        let ids = &mut self.edit_ids;
        let mut failure = None;
        let mut edit = |properties: &mut SdtProperties, content: Option<&mut Vec<InlineNode>>| {
            if let Err(error) = apply_patch(properties, &patch) {
                failure = Some(error);
                return;
            }
            if properties.control_id.is_none() {
                properties.control_id = Some(fresh.clone());
            }
            control_id.clone_from(&properties.control_id);
            if let (Some(text), Some(content)) = (patch.placeholder.as_deref(), content) {
                let text = text.trim();
                if !text.is_empty()
                    && properties.showing_placeholder
                    && takes_text(properties.control_kind)
                    && let Err(error) = rewrite_text(properties, content, text, true, ids)
                {
                    failure = Some(error);
                }
            }
        };
        let (ops, caret) =
            if let Some(paragraph) = paragraph_of_inline_control(&self.document, control) {
                let paragraph_id = paragraph.id;
                // The caret stays IN the control, as Word leaves it: its start
                // does not move when its properties (or its placeholder text)
                // change. It was the paragraph's start, which put the caret
                // outside every control not opening the paragraph, so Alt+Down,
                // Fill and Properties stopped finding the control just edited.
                let start = inline_control_start(&paragraph.inlines, control, &mut 0).unwrap_or(0);
                let mut inlines = paragraph.inlines.clone();
                with_inline_control(&mut inlines, control, &mut |sdt| {
                    edit(&mut sdt.properties, Some(&mut sdt.inlines));
                });
                (
                    vec![Operation::SetInlines {
                        node: paragraph_id,
                        inlines,
                    }],
                    Pos::new(paragraph_id, start),
                )
            } else if let Some((mut sdt, site)) = block_control(&self.document, control) {
                edit(&mut sdt.properties, None);
                let caret = first_paragraph(&sdt.blocks)
                    .map(|paragraph| Pos::new(paragraph, 0))
                    .ok_or_else(Self::no_such_control)?;
                (replace_block(site, BlockNode::Sdt(Box::new(sdt))), caret)
            } else {
                return Err(Self::no_such_control());
            };
        if let Some(error) = failure {
            return Err(error);
        }
        let result = self.apply_action_caret_as(ops, caret, HistoryKind::Edit)?;
        if let (Some(role), Some(key)) = (role, control_id) {
            match role {
                Some(name) => {
                    self.form_roles.assignments.insert(key, name);
                }
                None => {
                    self.form_roles.assignments.remove(&key);
                }
            }
        }
        Ok(result)
    }

    pub(crate) fn remove_content_control_inner(&mut self, id: &str) -> Result<EditResult, String> {
        let control = Self::node(id)?;
        let locked = || {
            refusal(
                "content-control.control-locked",
                "This content control can't be deleted. Clear \u{201c}Content control cannot be deleted\u{201d} in its properties first.",
            )
        };
        if let Some(paragraph) = paragraph_of_inline_control(&self.document, control) {
            let paragraph_id = paragraph.id;
            let mut inlines = paragraph.inlines.clone();
            let mut refused = false;
            with_inline_control(&mut inlines, control, &mut |sdt| {
                refused = control_locked(&sdt.properties);
            });
            if refused {
                return Err(locked());
            }
            unwrap_inline_control(&mut inlines, control);
            return self.apply_action_caret_as(
                vec![Operation::SetInlines {
                    node: paragraph_id,
                    inlines,
                }],
                Pos::new(paragraph_id, 0),
                HistoryKind::Edit,
            );
        }
        let (sdt, site) =
            block_control(&self.document, control).ok_or_else(Self::no_such_control)?;
        if control_locked(&sdt.properties) {
            return Err(locked());
        }
        let caret = first_paragraph(&sdt.blocks)
            .map(|paragraph| Pos::new(paragraph, 0))
            .ok_or_else(Self::no_such_control)?;
        let ops = vec![
            Operation::DeleteBlocks {
                container: site.container,
                index: site.index,
                count: 1,
            },
            Operation::InsertBlocks {
                container: site.container,
                index: site.index,
                blocks: sdt.blocks,
            },
        ];
        self.apply_action_caret_as(ops, caret, HistoryKind::Edit)
    }

    /// The fill gate: the control is there, its kind takes the value, its
    /// content is not locked and the reader's role allows it.
    fn fill_gate(&self, properties: &SdtProperties) -> Result<(), String> {
        if content_locked(properties) {
            return Err(refusal(
                "content-control.content-locked",
                "This content control's contents are locked. Clear \u{201c}Contents cannot be edited\u{201d} in its properties first.",
            ));
        }
        if !self.form_roles.may_fill(properties.control_id.as_deref()) {
            return Err(refusal(
                "content-control.other-role",
                "This field is for another role to fill.",
            ));
        }
        Ok(())
    }

    pub(crate) fn fill_content_control_inner(
        &mut self,
        id: &str,
        value: &str,
    ) -> Result<EditResult, String> {
        let control = Self::node(id)?;
        let value: ValuePatch = serde_json::from_str(value)
            .map_err(|error| format!("The value could not be read: {error}"))?;
        if value.text.is_empty() {
            return Err(refusal(
                "content-control.empty-value",
                "Choose or type a value.",
            ));
        }
        let paragraph = paragraph_of_inline_control(&self.document, control)
            .ok_or_else(Self::no_such_control)?;
        let paragraph_id = paragraph.id;
        let mut inlines = paragraph.inlines.clone();
        let mut stored = None;
        with_inline_control(&mut inlines, control, &mut |sdt| {
            stored = Some(sdt.properties.clone());
        });
        let stored = stored.ok_or_else(Self::no_such_control)?;
        self.fill_gate(&stored)?;
        if !matches!(
            stored.control_kind,
            None | Some(
                SdtControlKind::RichText
                    | SdtControlKind::PlainText
                    | SdtControlKind::ComboBox
                    | SdtControlKind::DropDownList
                    | SdtControlKind::Date
            )
        ) {
            return Err(refusal(
                "content-control.not-text",
                "This content control isn't filled with text.",
            ));
        }
        let ids = &mut self.edit_ids;
        let mut failure = None;
        with_inline_control(&mut inlines, control, &mut |sdt| {
            let properties = &mut sdt.properties;
            if let (Some(SdtControlData::Date(date)), Some(full)) =
                (properties.data.as_mut(), value.full_date.as_deref())
            {
                date.full_date = Some(full.chars().take(64).collect());
            }
            if let Err(error) = rewrite_text(properties, &mut sdt.inlines, &value.text, false, ids)
            {
                failure = Some(error);
            }
        });
        if let Some(error) = failure {
            return Err(error);
        }
        // Word removes a temporary control once it has been filled, keeping
        // what was filled in.
        if stored.temporary {
            unwrap_inline_control(&mut inlines, control);
        }
        let caret = Pos::new(paragraph_id, 0);
        self.apply_form_action(
            vec![Operation::SetInlines {
                node: paragraph_id,
                inlines,
            }],
            caret,
            HistoryKind::Typing,
        )
    }

    pub(crate) fn fill_picture_content_control_inner(
        &mut self,
        id: &str,
        bytes: Vec<u8>,
        width_emu: f64,
        height_emu: f64,
        mime: &str,
    ) -> Result<EditResult, String> {
        let control = Self::node(id)?;
        let paragraph = paragraph_of_inline_control(&self.document, control)
            .ok_or_else(Self::no_such_control)?;
        let paragraph_id = paragraph.id;
        let mut stored = None;
        with_inline_control(&mut paragraph.inlines.clone(), control, &mut |sdt| {
            stored = Some(sdt.properties.clone());
        });
        let stored = stored.ok_or_else(Self::no_such_control)?;
        if stored.control_kind != Some(SdtControlKind::Picture) {
            return Err(refusal(
                "content-control.not-picture",
                "This content control doesn't hold a picture.",
            ));
        }
        self.fill_gate(&stored)?;
        let (mut ops, drawing) = self.picture_parts(
            Pos::new(paragraph_id, 0),
            (bytes, width_emu, height_emu, mime),
        )?;
        let drawing_id = drawing.id();
        let paragraph =
            find_paragraph_any(&self.document, paragraph_id).ok_or_else(Self::no_such_control)?;
        let mut inlines = paragraph.inlines.clone();
        let mut slot = Some(drawing);
        with_inline_control(&mut inlines, control, &mut |sdt| {
            if let Some(drawing) = slot.take() {
                sdt.inlines = vec![drawing];
                sdt.properties.showing_placeholder = false;
            }
        });
        ops.push(Operation::SetInlines {
            node: paragraph_id,
            inlines,
        });
        ops.push(Operation::SetObjectLocks {
            object: drawing_id,
            locks: casual_doc_model::v1::ObjectLocks {
                frame: casual_doc_model::v1::LockFlags {
                    no_change_aspect: true,
                    ..casual_doc_model::v1::LockFlags::default()
                },
                object: casual_doc_model::v1::LockFlags::default(),
            },
        });
        self.apply_form_action(ops, Pos::new(paragraph_id, 0), HistoryKind::ObjectInsert)
    }
}

/// `DeleteBlocks` then `InsertBlocks` of `block` at `site`: one block replaced,
/// with its ids kept, which undo reverses exactly.
fn replace_block(site: BlockSite, block: BlockNode) -> Vec<Operation> {
    vec![
        Operation::DeleteBlocks {
            container: site.container,
            index: site.index,
            count: 1,
        },
        Operation::InsertBlocks {
            container: site.container,
            index: site.index,
            blocks: vec![block],
        },
    ]
}
