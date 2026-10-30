// SPDX-License-Identifier: Apache-2.0

//! Shape effects: Word's Shape Format ▸ Shape Effects ▸ Shadow, and
//! ONLYOFFICE's shape shadow (`docs/153` `object.shape-shadow`).
//!
//! Two bindings, `shapeShadow` and `setShapeShadow`, over the model's
//! [`ShapeShadow`] on a shape (`GroupShape`), a grouped text box
//! (`GroupTextBox`) and a lone text box (`TextBox`). Every binding has an
//! `_inner` twin returning `Result<_, String>`, because a native test cannot
//! build the `JsValue` the boundary returns.
//!
//! The write is ONE undoable `SetInlines` on the paragraph that owns the
//! object, the same lowering rotation and flip take (`objects.rs`
//! `edit_transform`), so no new operation reaches the transaction layer and
//! OT, protection and the undo log treat it exactly as they treat a turn.
//!
//! ## The host's vocabulary
//!
//! The JSON speaks Word's Shadow Options pane rather than DrawingML: `color`
//! (`#rrggbb`), `transparency` (`0..=1`, Word's percentage over 100), `angle`
//! (degrees clockwise from the positive x-axis — DrawingML's `@dir`), and
//! `distancePt` / `blurPt` (points). Word bounds the pane's distance to
//! 0..200 pt and its blur to 0..100 pt; so does this.
//!
//! ## Complexity
//!
//! Resolving the object is one walk of every surface's paragraphs, O(document)
//! as every object command already is; the write clones one paragraph's
//! inlines. Neither is on a keystroke path.

use std::str::FromStr;

use casual_doc_edit::{Operation, find_paragraph_any, visit_paragraphs_all_surfaces};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, GroupChild, InlineNode, Rgba, SHADOW_FULL_TURN, ShadowAlignment, ShapeShadow,
    contained_inlines, contained_inlines_mut,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::{EditResult, HistoryKind, WasmDocument, to_js};

/// EMU per typographic point.
const EMU_PER_POINT: f64 = 12_700.0;
/// Word's Shadow Options distance limit, in points.
const MAX_DISTANCE_PT: f64 = 200.0;
/// Word's Shadow Options blur limit, in points.
const MAX_BLUR_PT: f64 = 100.0;

/// `setShapeShadow`'s refusal for a node that carries no shadow slot (a
/// picture, a paragraph, an id that is not in the document).
pub(crate) const SHADOW_NOT_A_SHAPE: &str = "only a shape or a text box has a shadow";

/// The shadow Word's "Offset: Bottom Right" preset applies, which is what a
/// partial request starts from on a shape that has none: black at 60%
/// transparency, 3 pt away at 45°, blurred 4 pt, anchored top-left and not
/// turning with the shape — the attributes Word writes for it.
fn word_default_shadow() -> ShapeShadow {
    ShapeShadow {
        color: Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 102,
        },
        blur_rad_emu: 50_800,
        dist_emu: 38_100,
        dir: 2_700_000,
        alignment: Some(ShadowAlignment::TopLeft),
        rot_with_shape: Some(false),
        sx: None,
        sy: None,
        kx: None,
        ky: None,
    }
}

/// What `shapeShadow` answers inside `{"shadow": …}`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShadowRead {
    color: String,
    transparency: f64,
    angle: f64,
    distance_pt: f64,
    blur_pt: f64,
}

/// The `shapeShadow` envelope: `shadow` is `null` for a shape without one.
#[derive(Debug, Serialize)]
struct ShadowEnvelope {
    shadow: Option<ShadowRead>,
}

/// What `setShapeShadow` accepts. Every field optional: an omitted one keeps
/// what the shadow has (or Word's preset value for a shape without one).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShadowWrite {
    color: Option<String>,
    transparency: Option<f64>,
    angle: Option<f64>,
    distance_pt: Option<f64>,
    blur_pt: Option<f64>,
}

/// The read view of a model shadow. O(1).
fn read(shadow: &ShapeShadow) -> ShadowRead {
    let round = |value: f64, places: i32| {
        let scale = 10_f64.powi(places);
        (value * scale).round() / scale
    };
    #[allow(clippy::cast_precision_loss)] // bounded to MAX_EMU
    ShadowRead {
        color: format!(
            "#{:02x}{:02x}{:02x}",
            shadow.color.r, shadow.color.g, shadow.color.b
        ),
        transparency: round(1.0 - f64::from(shadow.color.a) / 255.0, 2),
        angle: round(f64::from(shadow.dir) / 60_000.0, 2),
        distance_pt: round(shadow.dist_emu as f64 / EMU_PER_POINT, 2),
        blur_pt: round(shadow.blur_rad_emu as f64 / EMU_PER_POINT, 2),
    }
}

/// `want` applied over `base`, validated. O(1).
fn apply(base: ShapeShadow, want: &ShadowWrite) -> Result<ShapeShadow, String> {
    let mut shadow = base;
    if let Some(hex) = want.color.as_deref() {
        let rgb = crate::parse_hex_color(hex).ok_or_else(|| format!("invalid color: {hex}"))?;
        shadow.color = Rgba {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
            a: shadow.color.a,
        };
    }
    if let Some(transparency) = want.transparency {
        if !(0.0..=1.0).contains(&transparency) {
            return Err("transparency must be between 0 and 1".to_owned());
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..=255
        {
            shadow.color.a = ((1.0 - transparency) * 255.0).round() as u8;
        }
    }
    if let Some(angle) = want.angle {
        if !angle.is_finite() {
            return Err("the shadow angle is not a finite number".to_owned());
        }
        #[allow(clippy::cast_possible_truncation)] // 0..360 * 60000 fits in i32
        let dir = (angle.rem_euclid(360.0) * 60_000.0).round() as i32;
        // 359.99999… rounds to a full turn, which the model refuses.
        shadow.dir = dir % SHADOW_FULL_TURN;
    }
    if let Some(points) = want.distance_pt {
        shadow.dist_emu = points_emu(points, MAX_DISTANCE_PT, "the shadow distance")?;
    }
    if let Some(points) = want.blur_pt {
        shadow.blur_rad_emu = points_emu(points, MAX_BLUR_PT, "the shadow blur")?;
    }
    Ok(shadow)
}

/// A length in points, bounded to `0..=max`, as EMU. O(1).
fn points_emu(points: f64, max: f64, label: &str) -> Result<i64, String> {
    if !points.is_finite() || !(0.0..=max).contains(&points) {
        return Err(format!("{label} must be between 0 and {max} pt"));
    }
    #[allow(clippy::cast_possible_truncation)] // at most 200 pt in EMU
    Ok((points * EMU_PER_POINT).round() as i64)
}

/// The shadow slot of the shape or text box `id` within `inlines`:
/// `Some(slot)` when found, `None` when it is not here. O(inlines in the
/// subtree).
fn find_slot(inlines: &[InlineNode], id: NodeId) -> Option<Option<ShapeShadow>> {
    for inline in inlines {
        let found = match inline {
            InlineNode::TextBox(text_box) if text_box.id == id => Some(text_box.shadow),
            InlineNode::TextBox(text_box) => find_in_blocks(&text_box.blocks, id),
            InlineNode::Group(group) => find_in_children(&group.children, id),
            other => contained_inlines(other).and_then(|nested| find_slot(nested, id)),
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn find_in_children(children: &[GroupChild], id: NodeId) -> Option<Option<ShapeShadow>> {
    for child in children {
        let found = match child {
            GroupChild::Shape(shape) if shape.id == id => Some(shape.shadow),
            GroupChild::TextBox(text_box) if text_box.id == id => Some(text_box.shadow),
            GroupChild::TextBox(text_box) => find_in_blocks(&text_box.blocks, id),
            GroupChild::Group(nested) => find_in_children(&nested.children, id),
            GroupChild::Shape(_) | GroupChild::Picture(_) => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn find_in_blocks(blocks: &[BlockNode], id: NodeId) -> Option<Option<ShapeShadow>> {
    for block in blocks {
        let found = match block {
            BlockNode::Paragraph(paragraph) => find_slot(&paragraph.inlines, id),
            BlockNode::Table(table) => table
                .rows
                .iter()
                .flat_map(|row| &row.cells)
                .find_map(|cell| find_in_blocks(&cell.blocks, id)),
            BlockNode::Sdt(sdt) => find_in_blocks(&sdt.blocks, id),
            BlockNode::AltChunk(_) => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Writes `shadow` into the slot of `id` within `inlines`; whether it was
/// found. The mutable twin of [`find_slot`], on the same container set.
fn write_slot(inlines: &mut [InlineNode], id: NodeId, shadow: Option<ShapeShadow>) -> bool {
    for inline in inlines.iter_mut() {
        let done = match inline {
            InlineNode::TextBox(text_box) if text_box.id == id => {
                text_box.shadow = shadow;
                true
            }
            InlineNode::TextBox(text_box) => write_in_blocks(&mut text_box.blocks, id, shadow),
            InlineNode::Group(group) => write_in_children(&mut group.children, id, shadow),
            other => {
                contained_inlines_mut(other).is_some_and(|nested| write_slot(nested, id, shadow))
            }
        };
        if done {
            return true;
        }
    }
    false
}

fn write_in_children(children: &mut [GroupChild], id: NodeId, shadow: Option<ShapeShadow>) -> bool {
    for child in children.iter_mut() {
        let done = match child {
            GroupChild::Shape(shape) if shape.id == id => {
                shape.shadow = shadow;
                true
            }
            GroupChild::TextBox(text_box) if text_box.id == id => {
                text_box.shadow = shadow;
                true
            }
            GroupChild::TextBox(text_box) => write_in_blocks(&mut text_box.blocks, id, shadow),
            GroupChild::Group(nested) => write_in_children(&mut nested.children, id, shadow),
            GroupChild::Shape(_) | GroupChild::Picture(_) => false,
        };
        if done {
            return true;
        }
    }
    false
}

fn write_in_blocks(blocks: &mut [BlockNode], id: NodeId, shadow: Option<ShapeShadow>) -> bool {
    for block in blocks.iter_mut() {
        let done = match block {
            BlockNode::Paragraph(paragraph) => write_slot(&mut paragraph.inlines, id, shadow),
            BlockNode::Table(table) => table
                .rows
                .iter_mut()
                .flat_map(|row| row.cells.iter_mut())
                .any(|cell| write_in_blocks(&mut cell.blocks, id, shadow)),
            BlockNode::Sdt(sdt) => write_in_blocks(&mut sdt.blocks, id, shadow),
            BlockNode::AltChunk(_) => false,
        };
        if done {
            return true;
        }
    }
    false
}

#[wasm_bindgen]
impl WasmDocument {
    /// The shadow of the shape or text box `node`, as
    /// `{"shadow":{"color":"#000000","transparency":0.6,"angle":45,"distancePt":3,"blurPt":4}}`,
    /// or `{"shadow":null}` when it has none. An empty string when `node` is
    /// not a shape or a text box, so a host can tell "no shadow" from "cannot
    /// have one" and disable the control with a reason.
    ///
    /// **O(document)**, one walk.
    #[wasm_bindgen(js_name = shapeShadow)]
    #[must_use]
    pub fn shape_shadow(&self, node: &str) -> String {
        let Ok(id) = NodeId::from_str(node) else {
            return String::new();
        };
        match self.shadow_owner(id) {
            Some((_, current)) => serde_json::to_string(&ShadowEnvelope {
                shadow: current.as_ref().map(read),
            })
            .unwrap_or_default(),
            None => String::new(),
        }
    }

    /// Word's Shape Effects ▸ Shadow: sets (`options` is the JSON
    /// [`shape_shadow`](Self::shape_shadow) reads, every field optional) or
    /// removes (`options` omitted or `"null"`) the outer shadow of the shape or
    /// text box `node`, as ONE undoable action.
    ///
    /// # Errors
    ///
    /// When `node` is not a shape or a text box, the JSON does not parse, or a
    /// value is out of Word's range.
    #[wasm_bindgen(js_name = setShapeShadow)]
    pub fn set_shape_shadow(
        &mut self,
        node: &str,
        options: Option<String>,
    ) -> Result<EditResult, JsValue> {
        self.set_shape_shadow_inner(node, options.as_deref())
            .map_err(to_js)
    }

    /// [`set_shape_shadow`](Self::set_shape_shadow) with a plain error, so its
    /// refusals are reachable from a native test.
    pub(crate) fn set_shape_shadow_inner(
        &mut self,
        node: &str,
        options: Option<&str>,
    ) -> Result<EditResult, String> {
        let id = NodeId::from_str(node).map_err(|_| "invalid node id".to_owned())?;
        let (paragraph, current) = self
            .shadow_owner(id)
            .ok_or_else(|| SHADOW_NOT_A_SHAPE.to_owned())?;
        let wanted = match options.map(str::trim) {
            None | Some("null") => None,
            Some(json) => {
                let want: ShadowWrite =
                    serde_json::from_str(json).map_err(|error| format!("bad shadow: {error}"))?;
                Some(apply(current.unwrap_or_else(word_default_shadow), &want)?)
            }
        };
        let source = find_paragraph_any(&self.document, paragraph)
            .ok_or_else(|| SHADOW_NOT_A_SHAPE.to_owned())?;
        let mut inlines = source.inlines.clone();
        if !write_slot(&mut inlines, id, wanted) {
            return Err(SHADOW_NOT_A_SHAPE.to_owned());
        }
        self.apply_action_as(
            vec![Operation::SetInlines {
                node: paragraph,
                inlines,
            }],
            HistoryKind::Formatting,
        )
    }

    /// The paragraph owning the shape or text box `id`, and its current
    /// shadow, across every surface. **O(document)**.
    fn shadow_owner(&self, id: NodeId) -> Option<(NodeId, Option<ShapeShadow>)> {
        let mut found = None;
        visit_paragraphs_all_surfaces(&self.document, &mut |paragraph| {
            if found.is_none()
                && let Some(current) = find_slot(&paragraph.inlines, id)
            {
                found = Some((paragraph.id, current));
            }
        });
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_document;

    const RICH_DOCX: &[u8] = include_bytes!("../../../fixtures/corpus/real-producer-rich.docx");

    /// A document with one freshly inserted rectangle; returns its shape id.
    fn with_shape() -> (WasmDocument, String) {
        let mut document = open_document(RICH_DOCX).expect("the fixture opens");
        let paragraph = document
            .document
            .body()
            .iter()
            .find_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph.id.to_string()),
                _ => None,
            })
            .expect("a body paragraph");
        document
            .insert_shape(&paragraph, 0, "rectangle")
            .expect("insert a rectangle");
        let shape = document
            .document
            .body()
            .iter()
            .find_map(|block| match block {
                BlockNode::Paragraph(paragraph) => {
                    paragraph.inlines.iter().find_map(|inline| match inline {
                        InlineNode::Group(group) => {
                            group.children.iter().find_map(|child| match child {
                                GroupChild::Shape(shape) => Some(shape.id.to_string()),
                                _ => None,
                            })
                        }
                        _ => None,
                    })
                }
                _ => None,
            })
            .expect("the inserted shape");
        (document, shape)
    }

    /// Word's preset applies on a partial request, reads back in the pane's
    /// units, and ONE undo removes it.
    ///
    /// Mutation: writing the slot without the `SetInlines` (mutating
    /// `self.document` directly) leaves undo with nothing to undo and fails
    /// the last assertion; dropping `word_default_shadow` for a zero shadow
    /// fails the preset assertion.
    #[test]
    fn a_shadow_applies_reads_back_and_undoes_as_one_action() {
        let (mut d, shape) = with_shape();
        assert_eq!(d.shape_shadow(&shape), r#"{"shadow":null}"#);

        d.set_shape_shadow_inner(&shape, Some(r##"{"color":"#ff0000"}"##))
            .expect("a shadow on a shape");
        assert_eq!(
            d.shape_shadow(&shape),
            r##"{"shadow":{"color":"#ff0000","transparency":0.6,"angle":45.0,"distancePt":3.0,"blurPt":4.0}}"##,
            "Word's preset, recoloured"
        );

        d.set_shape_shadow_inner(
            &shape,
            Some(r#"{"transparency":0,"angle":-90,"distancePt":10,"blurPt":0}"#),
        )
        .expect("options");
        assert_eq!(
            d.shape_shadow(&shape),
            r##"{"shadow":{"color":"#ff0000","transparency":0.0,"angle":270.0,"distancePt":10.0,"blurPt":0.0}}"##,
            "the colour is kept and the angle normalised"
        );

        d.undo_inner().expect("undo the options");
        assert!(d.shape_shadow(&shape).contains(r#""distancePt":3.0"#));
        d.undo_inner().expect("undo the shadow");
        assert_eq!(d.shape_shadow(&shape), r#"{"shadow":null}"#);
    }

    #[test]
    fn a_shadow_is_cleared_by_null_and_out_of_range_values_are_refused() {
        let (mut d, shape) = with_shape();
        d.set_shape_shadow_inner(&shape, Some("{}"))
            .expect("preset");
        d.set_shape_shadow_inner(&shape, None).expect("clear");
        assert_eq!(d.shape_shadow(&shape), r#"{"shadow":null}"#);
        for bad in [
            r#"{"transparency":1.5}"#,
            r#"{"distancePt":-1}"#,
            r#"{"blurPt":101}"#,
            r#"{"color":"red"}"#,
            r#"{"glow":true}"#,
        ] {
            assert!(
                d.set_shape_shadow_inner(&shape, Some(bad)).is_err(),
                "{bad} is refused, not ignored"
            );
        }
        let paragraph = d.document.body().iter().find_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(paragraph.id.to_string()),
            _ => None,
        });
        assert_eq!(
            d.set_shape_shadow_inner(&paragraph.expect("paragraph"), Some("{}"))
                .err()
                .as_deref(),
            Some(SHADOW_NOT_A_SHAPE)
        );
        assert_eq!(d.shape_shadow("not-a-node"), "");
    }

    #[test]
    fn a_text_box_takes_a_shadow_too() {
        let mut d = open_document(RICH_DOCX).expect("the fixture opens");
        let paragraph = d
            .document
            .body()
            .iter()
            .find_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph.id),
                _ => None,
            })
            .expect("a body paragraph");
        d.insert_text_box(&paragraph.to_string(), 0)
            .expect("insert a text box");
        let text_box = find_paragraph_any(&d.document, paragraph)
            .and_then(|source| {
                source.inlines.iter().find_map(|inline| match inline {
                    InlineNode::TextBox(text_box) => Some(text_box.id.to_string()),
                    _ => None,
                })
            })
            .expect("the inserted text box");
        d.set_shape_shadow_inner(&text_box, Some(r#"{"blurPt":8}"#))
            .expect("a text box shadow");
        assert!(d.shape_shadow(&text_box).contains(r#""blurPt":8.0"#));
    }
}
