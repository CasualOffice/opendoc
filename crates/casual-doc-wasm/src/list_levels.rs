// SPDX-License-Identifier: Apache-2.0

//! List levels and indents on the paragraph the reader SEES, not on its direct
//! formatting alone (`docs/109` UX-058, audit CANVAS-01/CANVAS-02).
//!
//! A list item very often carries no `w:numPr` of its own: Word's *List
//! Bullet* style puts the numbering reference — and an indent — on the STYLE,
//! and the paragraph names only the style. Every list command here used to
//! read the paragraph's direct properties, so on such an item:
//!
//! * the editor did not know it was in a list at all, and Tab fell through to
//!   "add 0.25in to the DIRECT left indent". There was none, so the direct
//!   indent became 0.25in — LESS than the style's 0.375in — and Tab moved the
//!   item LEFT; Shift+Tab then set it to 0 and the hanging bullet went into
//!   the page margin;
//! * Enter on an empty item could not end the list, because Bullets "turned
//!   on" a list the paragraph visibly already had.
//!
//! The rule now: every read resolves through the style cascade, the way layout
//! resolves it; a level change lands the item where its new level puts it —
//! the level's own indent when the definition has one and nothing overrides
//! it, otherwise one step (the definition's own step between the two levels,
//! else Word's 0.25in) from where the item is drawn now; and an outdent never
//! moves an item left of where its style puts it, so a hanging marker cannot
//! leave the page. Turning a style-supplied list off writes `numId 0`
//! (`numbering_none`), the only way OOXML can cancel an inherited list.
//!
//! Complexity: each command resolves the selected paragraphs through one
//! `ParagraphIndex` and one `StyleCascade` — O(document + selected), the cost of
//! the paragraph commands it replaces.

use casual_doc_edit::Operation;
use casual_doc_layout::cascade::StyleCascade;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{Indentation, NumberingRef, ParagraphProperties, StyleId, StyleKind};
use std::collections::BTreeSet;
use wasm_bindgen::JsValue;

use crate::{EditResult, HistoryKind, ParagraphIndex, WasmDocument, to_js};

/// One indent step when the list definition gives none: Word's 0.25in, which
/// is also what Increase Indent moves an ordinary paragraph by.
pub(crate) const INDENT_STEP_TWIPS: i32 = 360;

/// The deepest list level OOXML defines (`w:ilvl` 0..=8).
const MAX_LEVEL: i32 = 8;

/// `over` field by field on top of `base` — layout's own merge
/// (`flow::merge_indent_over`): the paragraph's (style + direct) indentation
/// wins per field and the list level fills in what it leaves unset.
fn merged(base: Option<Indentation>, over: Option<Indentation>) -> Indentation {
    let base = base.unwrap_or_default();
    let Some(over) = over else {
        return base;
    };
    Indentation {
        start_twips: over.start_twips.or(base.start_twips),
        end_twips: over.end_twips.or(base.end_twips),
        first_line_twips: over.first_line_twips.or(base.first_line_twips),
        hanging_twips: over.hanging_twips.or(base.hanging_twips),
    }
}

/// What a list paragraph looks like to the level-change rule.
pub(crate) struct ListItem {
    /// The paragraph's own properties.
    pub direct: ParagraphProperties,
    /// Style cascade plus direct properties — what layout flows.
    pub effective: ParagraphProperties,
    /// Style cascade WITHOUT the direct indentation: where the item sits when
    /// it carries no indent of its own.
    pub inherited: Option<Indentation>,
}

/// The direct properties a level change of `delta` writes, or `None` when it
/// changes nothing (an outdent of an item already at its outermost position).
///
/// `level_indent(l)` is `None` when the list definition has no level `l`, and
/// `Some(indent)` (possibly `Some(None)`) when it does. Pure, O(1).
pub(crate) fn level_change(
    item: &ListItem,
    level_indent: &dyn Fn(u8) -> Option<Option<Indentation>>,
    delta: i32,
) -> Option<ParagraphProperties> {
    let numbering = item.effective.numbering?;
    let current = numbering.level;
    let current_def = level_indent(current).flatten();
    let drawn = merged(current_def, item.effective.indentation);
    let drawn_start = drawn.start_twips.unwrap_or(0);
    let hanging = drawn.hanging_twips.unwrap_or(0);
    let natural_at = |level: u8| {
        merged(level_indent(level).flatten(), item.inherited)
            .start_twips
            .unwrap_or(0)
    };
    let wanted = u8::try_from((i32::from(current) + delta).clamp(0, MAX_LEVEL)).unwrap_or(0);

    let (level, desired) = if wanted != current && level_indent(wanted).is_some() {
        // A real level change. The step is the definition's own distance
        // between the two levels, measured from where the item is DRAWN — so
        // an item whose style overrides the level indent still moves, and by
        // the amount the list's author chose.
        let wanted_def = level_indent(wanted).flatten();
        let step = match (
            current_def.and_then(|i| i.start_twips),
            wanted_def.and_then(|i| i.start_twips),
        ) {
            (Some(from), Some(to)) if from != to => to - from,
            _ => INDENT_STEP_TWIPS * (i32::from(wanted) - i32::from(current)),
        };
        (wanted, drawn_start + step)
    } else if delta > 0 && wanted != current {
        // A single-level definition (Word's legacy List Bullet lists) has no
        // deeper level to demote to: a level it does not define would lose its
        // marker. The item is indented one step instead and keeps its bullet.
        (current, drawn_start + INDENT_STEP_TWIPS * delta)
    } else if delta < 0 {
        // At the outermost level: take back only indent the item was GIVEN,
        // never past where its style and level put it.
        let natural = natural_at(current);
        if drawn_start <= natural {
            return None;
        }
        (
            current,
            (drawn_start + INDENT_STEP_TWIPS * delta).max(natural),
        )
    } else {
        return None;
    };
    // Never into the margin: the marker hangs `hanging` left of the text.
    let desired = desired.max(hanging).max(0);

    let mut next = item.direct.clone();
    if level != current || next.numbering.is_some() {
        next.numbering = Some(NumberingRef { level, ..numbering });
        next.numbering_none = false;
    }
    let mut indent = next.indentation.unwrap_or_default();
    // Only the left edge is written, and only when it differs from where the
    // new level would put the item anyway: Tab then Shift+Tab leaves no trace.
    indent.start_twips = (desired != natural_at(level)).then_some(desired);
    next.indentation = (indent != Indentation::default()).then_some(indent);
    (next != item.direct).then_some(next)
}

/// Increase / Decrease Indent on an ordinary paragraph: `delta` from where the
/// paragraph is DRAWN (style included), not from its direct indent alone, which
/// moved a style-indented paragraph LEFT on Increase. `None` when nothing moves.
/// Pure, O(1).
pub(crate) fn indent_change(
    direct: &ParagraphProperties,
    effective: &ParagraphProperties,
    inherited: Option<Indentation>,
    delta: i32,
) -> Option<ParagraphProperties> {
    let drawn = effective.indentation.unwrap_or_default();
    let drawn_start = drawn.start_twips.unwrap_or(0);
    // A hanging first line never protrudes past the page edge.
    let floor = drawn.hanging_twips.unwrap_or(0);
    let desired = (drawn_start + delta).max(floor).max(0);
    let natural = inherited.and_then(|i| i.start_twips).unwrap_or(0);
    let mut next = direct.clone();
    let mut indent = next.indentation.unwrap_or_default();
    indent.start_twips = (desired != natural).then_some(desired);
    next.indentation = (indent != Indentation::default()).then_some(indent);
    (next != *direct).then_some(next)
}

/// Turns a paragraph's list on (`Some`) or off (`None`). Off on a paragraph
/// whose style supplies the list (`styled` names those styles) writes `numId 0`
/// — clearing the direct reference alone would leave the style's list showing;
/// on clears any such cancellation, which would otherwise win over the new
/// reference. O(log styles).
pub(crate) fn set_list_numbering(
    p: &mut ParagraphProperties,
    target: Option<NumberingRef>,
    styled: &BTreeSet<Option<StyleId>>,
) {
    p.numbering = target;
    p.numbering_none = target.is_none() && styled.contains(&p.style_ref);
}

impl WasmDocument {
    /// The numbering reference layout draws `node` with — the paragraph's own, or
    /// the one its style chain supplies, unless cancelled (`numId 0`). O(document).
    pub(crate) fn effective_numbering(&self, node: NodeId) -> Option<NumberingRef> {
        let direct = casual_doc_edit::paragraph_properties(&self.document, node)?;
        StyleCascade::new(self.document.definitions())
            .resolve_paragraph(&direct)
            .numbering
    }

    /// The paragraph styles (`None` = the default paragraph style) that hand a
    /// numbering reference to a paragraph which sets none itself. Turning a list
    /// off on such a paragraph must write `numId 0`; anywhere else it must not,
    /// or every unlisted paragraph would grow a meaningless `w:numPr`.
    /// O(styles × style-chain depth).
    pub(crate) fn styles_supplying_numbering(&self) -> BTreeSet<Option<StyleId>> {
        let definitions = self.document.definitions();
        let cascade = StyleCascade::new(definitions);
        let supplies = |style_ref: Option<StyleId>| {
            let probe = ParagraphProperties {
                style_ref,
                ..ParagraphProperties::default()
            };
            cascade.resolve_paragraph(&probe).numbering.is_some()
        };
        let mut out = BTreeSet::new();
        if supplies(None) {
            out.insert(None);
        }
        for (id, style) in definitions.styles.iter() {
            if style.kind == StyleKind::Paragraph && supplies(Some(*id)) {
                out.insert(Some(*id));
            }
        }
        out
    }

    /// Each selected paragraph's direct, effective and inherited properties,
    /// through one index and one cascade.
    fn selected_list_items(
        &self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> Result<Vec<(NodeId, ListItem)>, String> {
        let (start, end) = self.order_endpoints(start_node, start_offset, end_node, end_offset)?;
        let by_id = ParagraphIndex::build(&self.document);
        let cascade = StyleCascade::new(self.document.definitions());
        Ok(self
            .paragraphs_in_selection(start, end)
            .into_iter()
            .filter_map(|node| {
                let direct = by_id.properties(node)?;
                let effective = cascade.resolve_paragraph(&direct);
                let bare = ParagraphProperties {
                    indentation: None,
                    ..direct.clone()
                };
                let inherited = cascade.resolve_paragraph(&bare).indentation;
                Some((
                    node,
                    ListItem {
                        direct,
                        effective,
                        inherited,
                    },
                ))
            })
            .collect())
    }

    /// Writes each `(node, current, next)` as one `SetParagraphProperties`,
    /// offered to Suggesting's tracker, as ONE undoable step.
    fn apply_paragraph_rewrites(
        &mut self,
        rewrites: Vec<(NodeId, ParagraphProperties, ParagraphProperties)>,
        kind: HistoryKind,
        nothing: &str,
    ) -> Result<EditResult, JsValue> {
        if rewrites.is_empty() {
            return Err(to_js(nothing.to_owned()));
        }
        let mut ops: Vec<Operation> = Vec::with_capacity(rewrites.len());
        for (node, current, next) in rewrites {
            let properties = self.track_paragraph_change(&current, next).map_err(to_js)?;
            ops.push(Operation::SetParagraphProperties {
                node,
                properties: Box::new(properties),
            });
        }
        self.apply_action_as(ops, kind).map_err(to_js)
    }

    /// Tab / Shift+Tab / Increase and Decrease Indent on list items
    /// (`adjustListLevel`): see the module comment for the rule.
    pub(crate) fn adjust_list_level_inner(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        delta: i32,
    ) -> Result<EditResult, JsValue> {
        let items = self
            .selected_list_items(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        if items
            .first()
            .is_none_or(|(_, item)| item.effective.numbering.is_none())
        {
            return Err(to_js(
                "list level adjustment requires a list paragraph".into(),
            ));
        }
        let resolver = self.document.definitions().numbering_resolver();
        let rewrites: Vec<_> = items
            .into_iter()
            .filter_map(|(node, item)| {
                let numbering = item.effective.numbering?;
                let level_indent = |level: u8| {
                    resolver
                        .level(NumberingRef { level, ..numbering })
                        .map(|l| l.paragraph_properties.as_ref().and_then(|p| p.indentation))
                };
                level_change(&item, &level_indent, delta).map(|next| (node, item.direct, next))
            })
            .collect();
        self.apply_paragraph_rewrites(
            rewrites,
            HistoryKind::ListFormatting,
            "refused: The list item is already at its outermost level",
        )
    }

    /// Increase / Decrease Indent on ordinary paragraphs (`adjustIndent`): each
    /// selected paragraph moves `delta` from where it is drawn.
    pub(crate) fn adjust_indent_inner(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        delta: i32,
    ) -> Result<EditResult, JsValue> {
        let items = self
            .selected_list_items(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)?;
        let rewrites: Vec<_> = items
            .into_iter()
            .filter_map(|(node, item)| {
                indent_change(&item.direct, &item.effective, item.inherited, delta)
                    .map(|next| (node, item.direct, next))
            })
            .collect();
        self.apply_paragraph_rewrites(
            rewrites,
            HistoryKind::ParagraphFormatting,
            "refused: The paragraph is already at the margin",
        )
    }
}
