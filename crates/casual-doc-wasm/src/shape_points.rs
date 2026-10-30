// SPDX-License-Identifier: Apache-2.0

//! Word's Edit Points (`docs/153` `object.edit-points`, `docs/109` FID-SH-02).
//!
//! Right-click a shape ▸ Edit Points turns it into a freeform whose vertices can
//! be dragged. Word does this by converting the preset into its own custom
//! geometry at the shape's current size and adjust values, then editing that;
//! this module is the engine half of the same two steps:
//!
//! * [`WasmDocument::shape_edit_points`] reads the shape's geometry — an authored
//!   `a:custGeom` or one of the standard's 187 presets, evaluated by the one
//!   [`GeometryProgram`] that paints it (`docs/119` §6) — as LITERAL path
//!   commands in the shape's own EMU box. Arcs are already cubic Béziers there
//!   (the program resolves each arc to at most four), so every vertex and control
//!   point is a plain coordinate the host can drag.
//! * [`WasmDocument::set_shape_edit_points`] writes the edited commands back as a
//!   literal custom geometry (`ShapeGeometry::Other`, no preset token, no adjust
//!   values — they no longer drive anything) with the shape's text rectangle kept
//!   where it was, as ONE undoable action.
//!
//! The write is a `SetInlines` on the paragraph that holds the shape's drawing:
//! the closed op set has no geometry operation, and adding one would be a new
//! `Operation` for one command; `SetInlines` is the existing exact-inverse
//! vehicle for replacing a paragraph's inline content (it is what undo restores
//! verbatim). Every other identity in the paragraph is kept, because the
//! replacement is a clone with one field changed.
//!
//! Deliberately different from Word: a point dragged outside the shape's box is
//! drawn outside it rather than growing the box. Word re-fits the frame to the
//! path; that is a resize as well as a geometry edit, and is left to a later
//! increment.
//!
//! # Complexity
//!
//! Reading resolves the shape (`find_shape`, O(document)) and evaluates its
//! program (O(guides + commands)). Writing walks every surface's paragraphs once
//! to find the one holding the shape (O(document)) and clones that paragraph's
//! inlines. Neither runs per pointer move: the host drags its own copy of the
//! points and writes once, on commit.

use casual_doc_edit::{Operation, find_shape, visit_paragraphs_all_surfaces};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    CustomGeometry, GeometryPoint, GeometryProgram, GeometryRect, GeometryValue, GroupChild,
    GroupShape, InlineDescent, InlineDescentMut, InlineNode, PathFill, ResolvedCommand,
    ResolvedGeometry, ShapeGeometry, ShapePath, ShapePathCommand, inline_descent,
    inline_descent_mut, preset_shape,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::{EditResult, HistoryKind, WasmDocument, node_id_msg, to_js};

/// Paths one edited geometry may carry, and commands per path. Far past any
/// preset (the most complex has a handful) and any freeform a person draws;
/// a bound so a crafted payload cannot make the write walk long.
const MAX_PATHS: usize = 64;
const MAX_COMMANDS: usize = 4096;

/// How far outside its box a point may be dragged, as a multiple of the box's
/// larger side — room to pull a vertex well out, not to the edge of the EMU range.
const MAX_OVERSHOOT: f64 = 4.0;

/// The points of a shape, as the host edits them.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EditPoints {
    /// The shape's box, in EMU: every coordinate below is in it.
    pub width_emu: i64,
    pub height_emu: i64,
    /// How the box is placed, so the host can map the points onto the page.
    #[serde(default)]
    pub flip_h: bool,
    #[serde(default)]
    pub flip_v: bool,
    #[serde(default)]
    pub rotation: i32,
    pub paths: Vec<EditPath>,
}

/// One path: its paint flags (kept as they were) and its commands.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EditPath {
    #[serde(default)]
    pub fill: PathFill,
    #[serde(default = "yes")]
    pub stroke: bool,
    pub commands: Vec<EditCommand>,
}

const fn yes() -> bool {
    true
}

/// A command with literal points: `moveTo`/`lineTo` one, `quadTo` two (control,
/// end), `cubicTo` three (control 1, control 2, end), `close` none. The LAST
/// point of a command is its vertex; the others are its control points.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EditCommand {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub points: Vec<[f64; 2]>,
}

#[wasm_bindgen]
impl WasmDocument {
    /// Word's Edit Points, read: the shape's geometry as literal commands in its
    /// own EMU box, as JSON (`{widthEmu, heightEmu, flipH, flipV, rotation,
    /// paths: [{fill, stroke, commands: [{type, points}]}]}`). A preset is
    /// converted at its current size and adjust values.
    ///
    /// Throws when `shape` is not a shape, or its geometry has no answer at its
    /// size (`shape.points-unavailable`).
    #[wasm_bindgen(js_name = shapeEditPoints)]
    pub fn shape_edit_points(&self, shape: &str) -> Result<String, JsValue> {
        self.shape_edit_points_inner(shape)
            .and_then(|points| serde_json::to_string(&points).map_err(|e| e.to_string()))
            .map_err(to_js)
    }

    /// Word's Edit Points, written: replaces the shape's geometry with the edited
    /// commands (the JSON [`shape_edit_points`](Self::shape_edit_points) returns,
    /// points moved) as one undoable action.
    ///
    /// Throws on a malformed or unbounded payload (`shape.points-invalid`) or a
    /// shape that cannot be found.
    #[wasm_bindgen(js_name = setShapeEditPoints)]
    pub fn set_shape_edit_points(
        &mut self,
        shape: &str,
        points_json: &str,
    ) -> Result<EditResult, JsValue> {
        self.set_shape_edit_points_inner(shape, points_json)
            .map_err(to_js)
    }
}

impl WasmDocument {
    /// See [`WasmDocument::shape_edit_points`].
    pub(crate) fn shape_edit_points_inner(&self, shape: &str) -> Result<EditPoints, String> {
        let id = node_id_msg(shape)?;
        let shape = find_shape(&self.document, id).ok_or_else(not_a_shape)?;
        let (width, height) = box_of(shape);
        let resolved = evaluate(shape, width, height).ok_or_else(unavailable)?;
        Ok(EditPoints {
            width_emu: shape.extent.width_emu,
            height_emu: shape.extent.height_emu,
            flip_h: shape.flip_h,
            flip_v: shape.flip_v,
            rotation: shape.rotation.unwrap_or(0),
            paths: resolved
                .paths
                .iter()
                .map(|path| EditPath {
                    fill: path.fill,
                    stroke: path.stroke,
                    commands: path.commands.iter().map(edit_command).collect(),
                })
                .collect(),
        })
    }

    /// See [`WasmDocument::set_shape_edit_points`].
    pub(crate) fn set_shape_edit_points_inner(
        &mut self,
        shape: &str,
        points_json: &str,
    ) -> Result<EditResult, String> {
        let id = node_id_msg(shape)?;
        let points: EditPoints = serde_json::from_str(points_json).map_err(|_| invalid())?;
        let current = find_shape(&self.document, id).ok_or_else(not_a_shape)?;
        let (width, height) = box_of(current);
        // The text rectangle where it is now, as literals: the preset's guides
        // that placed it stop meaning anything once the path is a freeform.
        let text_rect = evaluate(current, width, height).map(|resolved| {
            let r = resolved.text_rect;
            GeometryRect {
                left: literal(r.left),
                top: literal(r.top),
                right: literal(r.right),
                bottom: literal(r.bottom),
            }
        });
        let geometry = custom_geometry(&points, width, height, current.extent.width_emu)?
            .with_text_rect(text_rect);
        geometry.check(&[]).map_err(|_| invalid())?;

        let (paragraph, mut inlines) =
            paragraph_holding(&self.document, id).ok_or_else(not_a_shape)?;
        let target = shape_in_inlines_mut(&mut inlines, id).ok_or_else(not_a_shape)?;
        target.geometry = ShapeGeometry::Other;
        target.preset = None;
        target.adjustments.clear();
        target.path = Some(geometry);
        self.apply_action_as(
            vec![Operation::SetInlines {
                node: paragraph,
                inlines,
            }],
            HistoryKind::Formatting,
        )
    }
}

/// The shape's box as the program evaluates it. Extents are validated to
/// `0..=MAX_EMU`, inside f64's exact integer range.
#[allow(clippy::cast_precision_loss)]
fn box_of(shape: &GroupShape) -> (f64, f64) {
    (
        shape.extent.width_emu as f64,
        shape.extent.height_emu as f64,
    )
}

/// The geometry the shape is painted with, resolved at `width` x `height` —
/// the same choice `casual-doc-layout`'s `geometry_content` makes: an authored
/// custom geometry wins, else the preset (typed or by token), whose table
/// carries `rect`, `ellipse` and `line` too.
fn evaluate(shape: &GroupShape, width: f64, height: f64) -> Option<ResolvedGeometry> {
    if let Some(custom) = &shape.path {
        return GeometryProgram::compile(&shape.adjustments, custom)
            .ok()?
            .evaluate(width, height, &[]);
    }
    let token = shape.geometry.preset_token().or(shape.preset.as_deref())?;
    preset_shape(token)?
        .program()
        .evaluate(width, height, &shape.adjustments)
}

fn edit_command(command: &ResolvedCommand) -> EditCommand {
    let at = |p: &casual_doc_model::v1::ResolvedPoint| [p.x, p.y];
    let (kind, points) = match command {
        ResolvedCommand::MoveTo(p) => ("moveTo", vec![at(p)]),
        ResolvedCommand::LineTo(p) => ("lineTo", vec![at(p)]),
        ResolvedCommand::QuadTo { control, point } => ("quadTo", vec![at(control), at(point)]),
        ResolvedCommand::CubicTo {
            control1,
            control2,
            point,
        } => ("cubicTo", vec![at(control1), at(control2), at(point)]),
        ResolvedCommand::Close => ("close", Vec::new()),
    };
    EditCommand {
        kind: kind.to_owned(),
        points,
    }
}

/// The edited commands as a literal custom geometry in the shape's box. Refuses
/// anything malformed or unbounded rather than writing a guess.
fn custom_geometry(
    points: &EditPoints,
    width: f64,
    height: f64,
    width_emu: i64,
) -> Result<CustomGeometry, String> {
    if points.paths.is_empty() || points.paths.len() > MAX_PATHS {
        return Err(invalid());
    }
    let reach = width.max(height).max(1.0) * MAX_OVERSHOOT;
    let point = |[x, y]: [f64; 2]| -> Result<GeometryPoint, String> {
        if !(x.is_finite() && y.is_finite()) || x.abs() > reach || y.abs() > reach {
            return Err(invalid());
        }
        // Rounded to whole EMU, which is the unit the path space is in.
        #[allow(clippy::cast_possible_truncation)]
        Ok(GeometryPoint::literal(x.round() as i64, y.round() as i64))
    };
    let mut paths = Vec::with_capacity(points.paths.len());
    for path in &points.paths {
        if path.commands.is_empty() || path.commands.len() > MAX_COMMANDS {
            return Err(invalid());
        }
        if path.commands[0].kind != "moveTo" {
            return Err(invalid());
        }
        let mut commands = Vec::with_capacity(path.commands.len());
        for command in &path.commands {
            let p = &command.points;
            let built = match (command.kind.as_str(), p.len()) {
                ("moveTo", 1) => ShapePathCommand::MoveTo {
                    point: point(p[0])?,
                },
                ("lineTo", 1) => ShapePathCommand::LineTo {
                    point: point(p[0])?,
                },
                ("quadTo", 2) => ShapePathCommand::QuadBezTo {
                    control: point(p[0])?,
                    point: point(p[1])?,
                },
                ("cubicTo", 3) => ShapePathCommand::CubicBezTo {
                    control1: point(p[0])?,
                    control2: point(p[1])?,
                    point: point(p[2])?,
                },
                ("close", 0) => ShapePathCommand::Close,
                _ => return Err(invalid()),
            };
            commands.push(built);
        }
        let mut shape_path = ShapePath::new(commands);
        // The path space IS the box: one EMU of path is one EMU of shape.
        shape_path.width_emu = width_emu;
        #[allow(clippy::cast_possible_truncation)]
        {
            shape_path.height_emu = height.round() as i64;
        }
        shape_path.fill = path.fill;
        shape_path.stroke = path.stroke;
        paths.push(shape_path);
    }
    Ok(CustomGeometry::from_paths(paths))
}

#[allow(clippy::cast_possible_truncation)]
fn literal(value: f64) -> GeometryValue {
    GeometryValue::Literal(value.round() as i64)
}

/// The paragraph whose own inline content holds shape `id` (through hyperlinks,
/// fields, revisions, content controls and drawing groups — NOT through a text
/// box's blocks, whose paragraphs are visited in their own right), with a clone
/// of its inlines. O(document).
fn paragraph_holding(
    document: &casual_doc_model::v1::Document,
    id: NodeId,
) -> Option<(NodeId, Vec<InlineNode>)> {
    let mut found = None;
    visit_paragraphs_all_surfaces(document, &mut |paragraph| {
        if found.is_none() && inlines_hold(&paragraph.inlines, id) {
            found = Some((paragraph.id, paragraph.inlines.clone()));
        }
    });
    found
}

fn inlines_hold(inlines: &[InlineNode], id: NodeId) -> bool {
    inlines.iter().any(|inline| match inline_descent(inline) {
        InlineDescent::Inlines(nested) => inlines_hold(nested, id),
        InlineDescent::Group(children) => children_hold(children, id),
        InlineDescent::Blocks(_) | InlineDescent::Leaf => false,
    })
}

fn children_hold(children: &[GroupChild], id: NodeId) -> bool {
    children.iter().any(|child| match child {
        GroupChild::Shape(shape) => shape.id == id,
        GroupChild::Group(nested) => children_hold(&nested.children, id),
        GroupChild::Picture(_) | GroupChild::TextBox(_) => false,
    })
}

/// The mutable twin of [`inlines_hold`], over the paragraph's cloned inlines.
fn shape_in_inlines_mut(inlines: &mut [InlineNode], id: NodeId) -> Option<&mut GroupShape> {
    for inline in inlines {
        let found = match inline_descent_mut(inline) {
            InlineDescentMut::Inlines(nested) => shape_in_inlines_mut(nested, id),
            InlineDescentMut::Group(children) => shape_in_children_mut(children, id),
            InlineDescentMut::Blocks(_) | InlineDescentMut::Leaf => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn shape_in_children_mut(children: &mut [GroupChild], id: NodeId) -> Option<&mut GroupShape> {
    for child in children {
        let found = match child {
            GroupChild::Shape(shape) if shape.id == id => Some(shape),
            GroupChild::Group(nested) => shape_in_children_mut(&mut nested.children, id),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn not_a_shape() -> String {
    casual_doc_edit::refusal::marked(
        "shape.points-not-shape",
        "Edit Points works on a shape. Select a shape first.",
    )
}

fn unavailable() -> String {
    casual_doc_edit::refusal::marked(
        "shape.points-unavailable",
        "This shape's outline can't be edited at its current size.",
    )
}

fn invalid() -> String {
    casual_doc_edit::refusal::marked(
        "shape.points-invalid",
        "These points don't make a shape outline.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_model::v1::BlockNode;

    /// A document with one inserted preset shape, and the shape's id.
    fn with_shape(preset: &str) -> (WasmDocument, String) {
        let mut doc =
            crate::open_document(include_bytes!("../../../sample.docx")).expect("sample opens");
        let paragraph = doc
            .document
            .body()
            .iter()
            .find_map(|block| match block {
                BlockNode::Paragraph(p) => Some(p.id.to_string()),
                _ => None,
            })
            .expect("a body paragraph");
        let before = shape_ids(&doc);
        assert!(
            doc.insert_shape(&paragraph, 0, preset).is_ok(),
            "shape inserts"
        );
        let shape = shape_ids(&doc)
            .into_iter()
            .find(|id| !before.contains(id))
            .expect("the new shape");
        (doc, shape.to_string())
    }

    fn shape_ids(doc: &WasmDocument) -> Vec<NodeId> {
        let mut ids = Vec::new();
        visit_paragraphs_all_surfaces(&doc.document, &mut |p| {
            fn walk(children: &[GroupChild], ids: &mut Vec<NodeId>) {
                for child in children {
                    match child {
                        GroupChild::Shape(s) => ids.push(s.id),
                        GroupChild::Group(g) => walk(&g.children, ids),
                        _ => {}
                    }
                }
            }
            for inline in &p.inlines {
                if let InlineDescent::Group(children) = inline_descent(inline) {
                    walk(children, &mut ids);
                }
            }
        });
        ids
    }

    #[test]
    fn a_preset_converts_to_literal_points_in_its_box() {
        let (doc, shape) = with_shape("triangle");
        let points = doc.shape_edit_points_inner(&shape).expect("points");
        assert_eq!(points.paths.len(), 1);
        let commands = &points.paths[0].commands;
        assert_eq!(commands[0].kind, "moveTo");
        // An isosceles triangle: three vertices and a close.
        let vertices: Vec<_> = commands.iter().filter(|c| !c.points.is_empty()).collect();
        assert_eq!(vertices.len(), 3, "{commands:?}");
        assert_eq!(commands.last().unwrap().kind, "close");
        let (w, h) = (points.width_emu as f64, points.height_emu as f64);
        // Its apex sits at the top centre, at the preset's default adjust value.
        assert!(
            vertices.iter().any(|c| {
                let [x, y] = *c.points.last().unwrap();
                (x - w / 2.0).abs() < 1.0 && y.abs() < 1.0
            }),
            "an apex at the top centre: {vertices:?}"
        );
        for c in &vertices {
            let [x, y] = *c.points.last().unwrap();
            assert!(
                (0.0..=w).contains(&x) && (0.0..=h).contains(&y),
                "inside the box"
            );
        }
    }

    #[test]
    fn moving_a_point_writes_a_freeform_and_undo_restores_the_preset() {
        let (mut doc, shape) = with_shape("triangle");
        let mut points = doc.shape_edit_points_inner(&shape).unwrap();
        let before = find_shape(&doc.document, node_id_msg(&shape).unwrap())
            .unwrap()
            .clone();
        // Drag the first vertex to the top-left corner.
        points.paths[0].commands[0].points[0] = [0.0, 0.0];
        let json = serde_json::to_string(&points).unwrap();
        doc.set_shape_edit_points_inner(&shape, &json)
            .expect("writes");

        let id = node_id_msg(&shape).unwrap();
        let after = find_shape(&doc.document, id).unwrap();
        assert_eq!(after.geometry, ShapeGeometry::Other);
        assert_eq!(after.preset, None);
        let custom = after.path.as_ref().expect("a custom geometry");
        assert_eq!(
            custom.paths[0].commands[0],
            ShapePathCommand::MoveTo {
                point: GeometryPoint::literal(0, 0)
            }
        );
        // It reads back as the points that were written.
        let reread = doc.shape_edit_points_inner(&shape).unwrap();
        assert_eq!(reread.paths[0].commands[0].points[0], [0.0, 0.0]);
        assert_eq!(after.extent, before.extent, "the box is untouched");

        // One undo puts the preset back exactly.
        doc.undo_inner().expect("undo");
        let restored = find_shape(&doc.document, id).unwrap();
        assert_eq!(restored, &before);
    }

    #[test]
    fn an_ellipse_is_editable_too_its_arcs_are_cubics() {
        let (doc, shape) = with_shape("ellipse");
        let points = doc.shape_edit_points_inner(&shape).unwrap();
        assert!(
            points.paths[0]
                .commands
                .iter()
                .any(|c| c.kind == "cubicTo" && c.points.len() == 3),
            "arcs arrive as cubic segments"
        );
    }

    #[test]
    fn a_malformed_or_unbounded_payload_is_refused_and_changes_nothing() {
        let (mut doc, shape) = with_shape("rectangle");
        let snapshot = doc.document.clone();
        let mut points = doc.shape_edit_points_inner(&shape).unwrap();
        points.paths[0].commands[0].points[0] = [f64::MAX, 0.0];
        let far = serde_json::to_string(&points).unwrap();
        let err = doc.set_shape_edit_points_inner(&shape, &far).unwrap_err();
        assert!(err.contains("shape.points-invalid"), "{err}");
        let not_moveto = far.replacen("moveTo", "lineTo", 1);
        assert!(
            doc.set_shape_edit_points_inner(&shape, &not_moveto)
                .is_err()
        );
        assert!(doc.set_shape_edit_points_inner(&shape, "{}").is_err());
        assert_eq!(doc.document, snapshot);
    }

    #[test]
    fn a_node_that_is_not_a_shape_is_refused() {
        let (doc, _) = with_shape("rectangle");
        let paragraph = doc.document.body()[0].clone();
        let BlockNode::Paragraph(p) = paragraph else {
            panic!("a paragraph first")
        };
        let err = doc.shape_edit_points_inner(&p.id.to_string()).unwrap_err();
        assert!(err.contains("shape.points-not-shape"), "{err}");
    }
}
