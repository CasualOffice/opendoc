// SPDX-License-Identifier: Apache-2.0

//! Align and distribute floating objects (`docs/109` RM-28, `docs/153`
//! `object.align-objects`, `object.align-to-page-margin`, `object.distribute`).
//!
//! Competitive standard first. ONLYOFFICE's Arrange ▸ Align
//! (`web-apps/apps/documenteditor/main/app/view/Toolbar.js`, `mniAlignToPage`,
//! `mniAlignToMargin`, `mniAlignObjects`, `mniDistribHor`, `mniDistribVert`) and
//! Word's Arrange ▸ Align both offer six alignments, two distributions and a
//! three-way target: the page, the margin, or the selected objects. Word's
//! default with one object selected is the margin; with several, the selected
//! objects. This module is that command.
//!
//! # What it writes
//!
//! * **To the page or the margin**, an object gets an ALIGNMENT, not an offset:
//!   `wp:positionH relativeFrom="margin"` with `wp:align left`, exactly what
//!   Word writes for Align Left with Align to Margin. The object then stays
//!   aligned when the margins or the page size change, which an offset would
//!   not.
//! * **To the selected objects**, and for a distribution, each object gets a
//!   page-relative offset — the same anchor `setObjectAnchorPosition` writes for
//!   a drag — computed from where layout actually PLACED the objects.
//!
//! Every object's anchor changes in ONE action, so one Ctrl+Z puts the whole
//! arrangement back. Only `SetAnchor` is emitted, so the closed operation set
//! is unchanged and this composes with undo and OT like a drag.
//!
//! # Deliberate differences
//!
//! * Alignment is by the object's own (unrotated) frame — the box the model
//!   stores and the selection outline shows — where Word uses the turned
//!   object's bounding box. A rotated object is aligned by its frame.
//! * A distribution is always among the selected objects, between the two
//!   outermost, and needs three. Word also distributes two objects across the
//!   page or margin; that is not offered here yet.
//!
//! # Cost
//!
//! O(document) for ONE object-box resolution and one anchor read per named
//! object (`anchor_of`, O(document) each), so O(objects × document) for a
//! selection of `objects` — bounded by what a person can select, never called
//! in a loop over the document.

use casual_doc_edit::refused;
use casual_doc_edit::{Operation, Pos};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AnchorHorizontal, AnchorVertical, DrawingAnchor, HorizontalAlign, HorizontalAnchor,
    HorizontalPosition, MAX_EMU, VerticalAlign, VerticalAnchor, VerticalPosition,
};
use wasm_bindgen::prelude::*;

use crate::{
    EMU_PER_TWIP_I64, EditResult, HistoryKind, WasmDocument, object_anchor_any_surface, to_js,
};

pub(crate) const ALIGN_NOTHING: &str = refused!(
    "object.align-nothing",
    "Select a floating object to align it."
);
pub(crate) const ALIGN_NOT_FLOATING: &str = refused!(
    "object.align-not-floating",
    "Only floating objects can be aligned. Give each one a text wrap first."
);
pub(crate) const ALIGN_UNPLACED: &str = refused!(
    "object.align-unplaced",
    "One of these objects is not laid out on a page yet, so it cannot be aligned."
);
pub(crate) const ALIGN_PAGES: &str = refused!(
    "object.align-pages",
    "These objects are on different pages, so they cannot be aligned to each other."
);
pub(crate) const ALIGN_NEEDS_TWO: &str = refused!(
    "object.align-needs-two",
    "Select two or more objects to align them to each other."
);
pub(crate) const DISTRIBUTE_NEEDS_THREE: &str = refused!(
    "object.distribute-needs-three",
    "Select three or more objects to distribute them."
);
pub(crate) const ALIGN_UNKNOWN: &str = refused!(
    "object.align-unknown",
    "That is not an alignment this editor knows."
);

/// One of the six alignments, or a distribution along one axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Arrangement {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
    DistributeHorizontally,
    DistributeVertically,
}

impl Arrangement {
    fn parse(token: &str) -> Option<Self> {
        Some(match token {
            "left" => Self::Left,
            "center" => Self::Center,
            "right" => Self::Right,
            "top" => Self::Top,
            "middle" => Self::Middle,
            "bottom" => Self::Bottom,
            "distributeHorizontally" => Self::DistributeHorizontally,
            "distributeVertically" => Self::DistributeVertically,
            _ => return None,
        })
    }

    fn is_distribution(self) -> bool {
        matches!(
            self,
            Self::DistributeHorizontally | Self::DistributeVertically
        )
    }
}

/// What an alignment is relative to: ONLYOFFICE's and Word's three targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Target {
    Page,
    Margin,
    Selected,
}

impl Target {
    fn parse(token: &str) -> Option<Self> {
        Some(match token {
            "page" => Self::Page,
            "margin" => Self::Margin,
            "selected" => Self::Selected,
            _ => return None,
        })
    }
}

/// One object's placed frame, in page-local twips, with its anchor.
struct Placed {
    id: NodeId,
    page: u32,
    left: i64,
    top: i64,
    width: i64,
    height: i64,
    anchor: DrawingAnchor,
}

/// The new top-left of each object for an alignment or a distribution among
/// `placed`, in twips. Pure, so the geometry is a unit test.
///
/// Distribution keeps the two outermost objects where they are (by their
/// leading edges) and makes the GAPS between neighbours equal, which is what
/// Word and ONLYOFFICE do; objects wider than the span they share get a
/// negative gap and overlap evenly rather than being refused.
///
/// O(n log n) for the distribution's sort, O(n) otherwise.
fn arranged_origins(placed: &[(i64, i64, i64, i64)], how: Arrangement) -> Vec<(i64, i64)> {
    let left = placed.iter().map(|p| p.0).min().unwrap_or(0);
    let top = placed.iter().map(|p| p.1).min().unwrap_or(0);
    let right = placed.iter().map(|p| p.0 + p.2).max().unwrap_or(0);
    let bottom = placed.iter().map(|p| p.1 + p.3).max().unwrap_or(0);
    match how {
        Arrangement::Left => placed.iter().map(|p| (left, p.1)).collect(),
        Arrangement::Right => placed.iter().map(|p| (right - p.2, p.1)).collect(),
        Arrangement::Center => placed
            .iter()
            .map(|p| ((left + right - p.2) / 2, p.1))
            .collect(),
        Arrangement::Top => placed.iter().map(|p| (p.0, top)).collect(),
        Arrangement::Bottom => placed.iter().map(|p| (p.0, bottom - p.3)).collect(),
        Arrangement::Middle => placed
            .iter()
            .map(|p| (p.0, (top + bottom - p.3) / 2))
            .collect(),
        Arrangement::DistributeHorizontally | Arrangement::DistributeVertically => {
            let horizontal = how == Arrangement::DistributeHorizontally;
            let lead = |p: &(i64, i64, i64, i64)| if horizontal { p.0 } else { p.1 };
            let extent = |p: &(i64, i64, i64, i64)| if horizontal { p.2 } else { p.3 };
            let mut order: Vec<usize> = (0..placed.len()).collect();
            order.sort_by_key(|&i| (lead(&placed[i]), i));
            let first = order.first().map_or(0, |&i| lead(&placed[i]));
            let span_end = order
                .last()
                .map_or(0, |&i| lead(&placed[i]) + extent(&placed[i]));
            let total: i64 = placed.iter().map(extent).sum();
            let gaps = i64::try_from(placed.len().saturating_sub(1))
                .unwrap_or(1)
                .max(1);
            let gap = (span_end - first - total) / gaps;
            let mut out: Vec<(i64, i64)> = placed.iter().map(|p| (p.0, p.1)).collect();
            let mut cursor = first;
            for &i in &order {
                out[i] = if horizontal {
                    (cursor, placed[i].1)
                } else {
                    (placed[i].0, cursor)
                };
                cursor += extent(&placed[i]) + gap;
            }
            out
        }
    }
}

#[wasm_bindgen]
impl WasmDocument {
    /// Word's and ONLYOFFICE's Arrange ▸ Align and Distribute, over the
    /// floating objects named by `nodes` (a JSON array of node ids), as ONE
    /// undoable action.
    ///
    /// `how` is `left`, `center`, `right`, `top`, `middle`, `bottom`,
    /// `distributeHorizontally` or `distributeVertically`; `relative_to` is
    /// `page`, `margin` or `selected`. See the module documentation for what
    /// each writes.
    ///
    /// # Errors
    ///
    /// With a marked refusal when nothing is named, an object is not floating
    /// or not placed, objects aligned to each other lie on different pages, an
    /// alignment to each other names fewer than two objects, a distribution
    /// fewer than three, or a token is unknown.
    ///
    /// **O(objects × document)**; see the module documentation.
    #[wasm_bindgen(js_name = alignObjects)]
    pub fn align_objects(
        &mut self,
        nodes: &str,
        how: &str,
        relative_to: &str,
    ) -> Result<EditResult, JsValue> {
        self.align_objects_inner(nodes, how, relative_to)
            .map_err(to_js)
    }

    /// Whether [`align_objects`](Self::align_objects) would act, as
    /// `{"can":true}` or `{"can":false,"reason":"…"}`, so a host disables the
    /// row WITH the sentence instead of offering a command that refuses.
    /// Nothing is written. O(objects × document).
    #[wasm_bindgen(js_name = canAlignObjects)]
    #[must_use]
    pub fn can_align_objects(&self, nodes: &str, how: &str, relative_to: &str) -> String {
        match self.align_plan(nodes, how, relative_to) {
            Ok(_) => "{\"can\":true}".to_owned(),
            Err(reason) => serde_json::json!({"can": false, "reason": reason}).to_string(),
        }
    }
}

impl WasmDocument {
    /// [`align_objects`](Self::align_objects) with a plain error, for native
    /// tests.
    pub(crate) fn align_objects_inner(
        &mut self,
        nodes: &str,
        how: &str,
        relative_to: &str,
    ) -> Result<EditResult, String> {
        let operations = self.align_plan(nodes, how, relative_to)?;
        let first = match operations.first() {
            Some(Operation::SetAnchor { object, .. }) => *object,
            _ => return Err(ALIGN_NOTHING.to_owned()),
        };
        self.apply_action_caret_as(operations, Pos::new(first, 0), HistoryKind::ObjectMove)
    }

    /// The `SetAnchor` operations an alignment would emit, or why it cannot.
    fn align_plan(
        &self,
        nodes: &str,
        how: &str,
        relative_to: &str,
    ) -> Result<Vec<Operation>, String> {
        let how = Arrangement::parse(how).ok_or_else(|| ALIGN_UNKNOWN.to_owned())?;
        let target = Target::parse(relative_to).ok_or_else(|| ALIGN_UNKNOWN.to_owned())?;
        let ids: Vec<NodeId> = serde_json::from_str::<Vec<String>>(nodes)
            .map_err(|_| ALIGN_NOTHING.to_owned())?
            .iter()
            .map(|id| id.parse::<NodeId>().map_err(|_| ALIGN_NOTHING.to_owned()))
            .collect::<Result<_, _>>()?;
        let mut ids_unique = Vec::with_capacity(ids.len());
        for id in ids {
            if !ids_unique.contains(&id) {
                ids_unique.push(id);
            }
        }
        if ids_unique.is_empty() {
            return Err(ALIGN_NOTHING.to_owned());
        }
        if how.is_distribution() && ids_unique.len() < 3 {
            return Err(DISTRIBUTE_NEEDS_THREE.to_owned());
        }
        if target == Target::Selected && !how.is_distribution() && ids_unique.len() < 2 {
            return Err(ALIGN_NEEDS_TWO.to_owned());
        }

        // ONE resolution of the placed boxes for the whole selection.
        let boxes = self.object_boxes();
        let mut placed = Vec::with_capacity(ids_unique.len());
        for id in &ids_unique {
            let anchor = object_anchor_any_surface(&self.document, *id)
                .ok_or_else(|| ALIGN_NOT_FLOATING.to_owned())?;
            let found = boxes
                .iter()
                .find(|candidate| candidate.root == *id || candidate.subject == *id)
                .ok_or_else(|| ALIGN_UNPLACED.to_owned())?;
            placed.push(Placed {
                id: *id,
                page: found.page,
                left: i64::from(found.rect.origin.x.raw()),
                top: i64::from(found.rect.origin.y.raw()),
                width: i64::from(found.rect.size.width.raw()),
                height: i64::from(found.rect.size.height.raw()),
                anchor,
            });
        }

        if target != Target::Selected && !how.is_distribution() {
            return Ok(placed
                .into_iter()
                .map(|object| Operation::SetAnchor {
                    object: object.id,
                    anchor: Box::new(aligned_to_box(object.anchor, how, target)),
                })
                .collect());
        }

        if placed.iter().any(|object| object.page != placed[0].page) {
            return Err(ALIGN_PAGES.to_owned());
        }
        let frames: Vec<(i64, i64, i64, i64)> = placed
            .iter()
            .map(|p| (p.left, p.top, p.width, p.height))
            .collect();
        let origins = arranged_origins(&frames, how);
        Ok(placed
            .into_iter()
            .zip(origins)
            .map(|(object, (left, top))| {
                let mut anchor = object.anchor;
                anchor.horizontal = AnchorHorizontal {
                    relative_from: HorizontalAnchor::Page,
                    position: HorizontalPosition::Offset(
                        (left * EMU_PER_TWIP_I64).clamp(-MAX_EMU, MAX_EMU),
                    ),
                };
                anchor.vertical = AnchorVertical {
                    relative_from: VerticalAnchor::Page,
                    position: VerticalPosition::Offset(
                        (top * EMU_PER_TWIP_I64).clamp(-MAX_EMU, MAX_EMU),
                    ),
                };
                Operation::SetAnchor {
                    object: object.id,
                    anchor: Box::new(anchor),
                }
            })
            .collect())
    }
}

/// `anchor` with ONE axis rewritten to Word's alignment token relative to the
/// page or the margin; the other axis is kept exactly as authored. O(1).
fn aligned_to_box(mut anchor: DrawingAnchor, how: Arrangement, target: Target) -> DrawingAnchor {
    let (horizontal_from, vertical_from) = match target {
        Target::Page => (HorizontalAnchor::Page, VerticalAnchor::Page),
        Target::Margin | Target::Selected => (HorizontalAnchor::Margin, VerticalAnchor::Margin),
    };
    let horizontal = |align| AnchorHorizontal {
        relative_from: horizontal_from,
        position: HorizontalPosition::Align(align),
    };
    let vertical = |align| AnchorVertical {
        relative_from: vertical_from,
        position: VerticalPosition::Align(align),
    };
    match how {
        Arrangement::Left => anchor.horizontal = horizontal(HorizontalAlign::Left),
        Arrangement::Center => anchor.horizontal = horizontal(HorizontalAlign::Center),
        Arrangement::Right => anchor.horizontal = horizontal(HorizontalAlign::Right),
        Arrangement::Top => anchor.vertical = vertical(VerticalAlign::Top),
        Arrangement::Middle => anchor.vertical = vertical(VerticalAlign::Center),
        Arrangement::Bottom => anchor.vertical = vertical(VerticalAlign::Bottom),
        Arrangement::DistributeHorizontally | Arrangement::DistributeVertically => {}
    }
    anchor
}

#[cfg(test)]
mod tests {
    use super::{Arrangement, arranged_origins};

    /// Three frames `(left, top, width, height)` in twips, deliberately unequal.
    const FRAMES: [(i64, i64, i64, i64); 3] = [
        (100, 500, 200, 100),
        (1000, 50, 400, 300),
        (600, 900, 100, 50),
    ];

    #[test]
    fn the_six_alignments_line_the_frames_up_on_the_selection_bounds() {
        let left = arranged_origins(&FRAMES, Arrangement::Left);
        assert!(left.iter().all(|o| o.0 == 100), "{left:?}");
        let right = arranged_origins(&FRAMES, Arrangement::Right);
        assert_eq!(right, vec![(1200, 500), (1000, 50), (1300, 900)]);
        let center = arranged_origins(&FRAMES, Arrangement::Center);
        assert_eq!(center, vec![(650, 500), (550, 50), (700, 900)]);
        let top = arranged_origins(&FRAMES, Arrangement::Top);
        assert!(top.iter().all(|o| o.1 == 50), "{top:?}");
        let bottom = arranged_origins(&FRAMES, Arrangement::Bottom);
        assert_eq!(bottom, vec![(100, 850), (1000, 650), (600, 900)]);
        let middle = arranged_origins(&FRAMES, Arrangement::Middle);
        assert_eq!(middle, vec![(100, 450), (1000, 350), (600, 475)]);
    }

    /// Mutation: spacing the LEADING edges equally instead of the gaps puts the
    /// middle frame at 550 rather than 600, and this fails.
    #[test]
    fn distributing_keeps_the_outermost_frames_and_equalises_the_gaps() {
        let spread = arranged_origins(&FRAMES, Arrangement::DistributeHorizontally);
        // Span 100..1400, widths 200 + 100 + 400 = 700, so two gaps of 300:
        // [100..300] gap [600..700] gap [1000..1400].
        assert_eq!(spread, vec![(100, 500), (1000, 50), (600, 900)]);
        // The middle frame crowded against the first moves to equalise the gaps.
        let moved = [(100, 0, 200, 10), (250, 0, 100, 10), (1000, 0, 400, 10)];
        let spread = arranged_origins(&moved, Arrangement::DistributeHorizontally);
        assert_eq!(spread, vec![(100, 0), (600, 0), (1000, 0)]);
        let down = arranged_origins(&FRAMES, Arrangement::DistributeVertically);
        // Span 50..950, heights 300 + 100 + 50 = 450, two gaps of 225.
        assert_eq!(down, vec![(100, 575), (1000, 50), (600, 900)]);
    }
}
