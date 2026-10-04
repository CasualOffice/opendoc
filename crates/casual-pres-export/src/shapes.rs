// SPDX-License-Identifier: Apache-2.0

//! `p:cSld` and the shape tree: `p:sp`, `p:pic`, `p:grpSp`.
//!
//! # One writer for three tiers
//!
//! `p:cSld` is literally the same element on a slide, a layout and a master — a
//! name, an optional background and an ordered shape tree — so there is one
//! writer for it. The importer reads all three through one reader for the same
//! reason, and `SKILL` §8 is explicit that a parallel path is evidence the
//! abstraction is wrong.
//!
//! # What a shape's element name is decided by
//!
//! `GroupChild` is the document model's enum, shared with DOCX, and the mapping to
//! PresentationML is: `Shape` → `p:sp`, `Picture` → `p:pic`, `Group` → `p:grpSp`.
//! `TextBox` cannot occur — `casual-pres-model` REFUSES it on a slide, so the arm
//! is unreachable rather than unimplemented, and it is written as a refusal here
//! rather than silently skipped.
//!
//! A slide's text lives on `SlideNode::text` (`a:txBody`), not inside the
//! `GroupChild`, which is why a `p:sp` can carry text while the shared enum has no
//! field for it.

use std::collections::BTreeMap;

use casual_doc_model::v1::{
    ColorTransform, DashStyle, Definitions, Extent, Fill, GroupChild, GroupPicture, GroupShape,
    GroupTransform, LineEndKind, PointEmu, Rgba, ShapePath, ShapePathCommand, ShapeStroke,
    StyleColor,
};
use casual_pres_model::{Placeholder, ShapeTree, Slide, SlideNode, SlidePaint};

use crate::opc::{Relationships, rel};
use crate::{ExportError, text};

/// Everything a shape writer needs that is not the shape itself.
///
/// One value rather than four parameters, and it is what the earlier draft of this
/// crate lacked: a nested picture had no relationship context, so it was written
/// without its `a:blip` and the gap was documented instead of closed. Threading the
/// context is the fix — a shape five groups deep resolves its media exactly as a
/// top-level one does.
pub(crate) struct ShapeContext<'a> {
    /// The deck's definition tables, which is where a `MediaId` becomes a part name.
    pub(crate) definitions: &'a Definitions,
    /// Part name to bytes, for the media the caller supplied.
    pub(crate) media: &'a BTreeMap<String, Vec<u8>>,
    /// The part being written, so a relationship target can be made relative to it.
    pub(crate) part_name: &'a str,
}

impl ShapeContext<'_> {
    /// The relationship id for a picture's image, minting one if the picture
    /// resolves to media the caller supplied.
    fn image_id(&self, picture: &GroupPicture, rels: &mut Relationships) -> Option<String> {
        let part = media_part_of(picture, self.definitions, self.media)?;
        let target = crate::relative_target(folder_of(self.part_name), part);
        Some(rels.add(rel::IMAGE, &target))
    }
}

/// One `ppt/slides/slideN.xml`.
pub(crate) fn slide_part(
    slide: &Slide,
    context: &ShapeContext<'_>,
    rels: &mut Relationships,
) -> Result<Vec<u8>, ExportError> {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#,
    );
    // The model stores `hidden` and the file states `show`, with inverted
    // polarity — so `show="0"` is written only for a hidden slide, and a visible
    // one omits the attribute as the schema default covers it. Writing the model's
    // flag straight through would hide every visible slide in the deck.
    if slide.hidden {
        xml.push_str(r#" show="0""#);
    }
    xml.push('>');
    xml.push_str(&common_slide_data(
        slide.name.as_deref(),
        slide.background.as_ref(),
        &slide.shapes,
        context,
        rels,
    )?);
    xml.push_str("</p:sld>");
    Ok(xml.into_bytes())
}

/// The `p:cSld` shared by a slide, a layout and a master.
pub(crate) fn common_slide_data(
    name: Option<&str>,
    background: Option<&Fill>,
    tree: &ShapeTree,
    context: &ShapeContext<'_>,
    rels: &mut Relationships,
) -> Result<String, ExportError> {
    let mut xml = String::from("<p:cSld");
    if let Some(name) = name {
        xml.push_str(&format!(r#" name="{}""#, text::escape(name)));
    }
    xml.push('>');
    if let Some(background) = background {
        // `p:bgPr` requires a fill AND an effect list; the effect list is written
        // empty because the model carries no slide-background effect, and an empty
        // `a:effectLst` is DrawingML for "no effects" rather than a claim.
        xml.push_str(&format!(
            "<p:bg><p:bgPr>{}<a:effectLst/></p:bgPr></p:bg>",
            fill_xml(background)
        ));
    }
    xml.push_str(&shape_tree_xml(tree, context, rels)?);
    xml.push_str("</p:cSld>");
    Ok(xml)
}

/// `p:spTree`.
fn shape_tree_xml(
    tree: &ShapeTree,
    context: &ShapeContext<'_>,
    rels: &mut Relationships,
) -> Result<String, ExportError> {
    let mut xml = String::from("<p:spTree>");
    // The non-visual group properties of the tree itself are required and carry
    // nothing the model keeps, so the minimal valid form is written.
    xml.push_str(r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>"#);
    xml.push_str(&format!(
        "<p:grpSpPr>{}</p:grpSpPr>",
        group_transform_xml(&tree.transform)
    ));
    for (position, node) in tree.children.iter().enumerate() {
        xml.push_str(&node_xml(node, position, context, rels)?);
    }
    xml.push_str("</p:spTree>");
    Ok(xml)
}

/// One child of a shape tree.
fn node_xml(
    node: &SlideNode,
    position: usize,
    context: &ShapeContext<'_>,
    rels: &mut Relationships,
) -> Result<String, ExportError> {
    // Shape ids restart at 2 per tree, because 1 is the tree's own
    // `p:nvGrpSpPr`. They are producer-scoped tokens: nothing in the package joins
    // on them, and the model's `NodeId` is this engine's own identity rather than
    // the file's.
    let shape_id = position + 2;
    // A node carrying an `a:tbl` is a `p:graphicFrame`, whatever its `GroupChild`
    // variant says. The frame's BOX is a `GroupChild::Shape` because that is how
    // the shared placement walk reaches it; following the variant here would
    // write a `p:sp` and lose every table in the deck on the first save, with no
    // geometry guard able to see it.
    if let Some(table) = node.table.as_ref() {
        return Ok(crate::table::graphic_frame_xml(node, table, shape_id));
    }
    match &node.content {
        GroupChild::Shape(shape) => Ok(shape_xml(node, shape, shape_id)),
        GroupChild::Picture(picture) => Ok(picture_xml(node, picture, shape_id, context, rels)),
        GroupChild::Group(group) => {
            let mut xml = format!(
                "<p:grpSp><p:nvGrpSpPr>{}<p:cNvGrpSpPr/>{}</p:nvGrpSpPr><p:grpSpPr>{}</p:grpSpPr>",
                non_visual_properties(shape_id, node.name.as_deref(), "Group", node.hidden),
                placeholder_xml(node.placeholder.as_ref()),
                group_transform_xml(&group.transform)
            );
            for (child_position, child) in group.children.iter().enumerate() {
                // A nested group's children are `GroupChild`, not `SlideNode`, so
                // they carry no placeholder and no `a:txBody` — which is exactly
                // why the presentation model refuses a text-bearing shape inside a
                // group, and why that refusal is visible here as a missing arm
                // rather than a dropped one.
                xml.push_str(&nested_child_xml(child, child_position + 2, context, rels));
            }
            xml.push_str("</p:grpSp>");
            Ok(xml)
        }
        GroupChild::TextBox(_) => {
            // Unreachable by construction: `PresentationError::TextBoxShapeOnSlide`
            // refuses this at model validation, so a `Presentation` carrying one
            // cannot exist. Refused rather than skipped, because skipping would
            // drop a shape from a written deck and say nothing.
            Err(ExportError::DanglingReference)
        }
    }
}

/// A `GroupChild` nested inside a `p:grpSp`, which carries no slide-level
/// placeholder or text.
///
/// Recursive, and it resolves media exactly as the top level does — the earlier
/// draft could not, because it had no relationship context and so wrote a nested
/// picture with no `a:blip`. A picture five groups deep is still a picture.
fn nested_child_xml(
    child: &GroupChild,
    shape_id: usize,
    context: &ShapeContext<'_>,
    rels: &mut Relationships,
) -> String {
    match child {
        GroupChild::Shape(shape) => {
            let mut xml = format!(
                "<p:sp><p:nvSpPr>{}<p:cNvSpPr/><p:nvPr/></p:nvSpPr>",
                non_visual_properties(shape_id, None, "Shape", false)
            );
            // `Inherited` on both, because a group child is a bare `GroupChild`: the
            // importer reports the `a:noFill` it had to drop here as `spPr/@noFill`,
            // and inventing a state for it on the way out would re-emit a fact the
            // model does not hold.
            xml.push_str(&shape_properties_xml(
                shape,
                SlidePaint::Inherited,
                SlidePaint::Inherited,
            ));
            // `p:sp` requires a `p:txBody`; a shape with no text gets the minimal
            // valid one rather than being written invalid.
            xml.push_str(EMPTY_TEXT_BODY);
            xml.push_str("</p:sp>");
            xml
        }
        GroupChild::Group(group) => {
            let mut xml = format!(
                "<p:grpSp><p:nvGrpSpPr>{}<p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr>{}</p:grpSpPr>",
                non_visual_properties(shape_id, None, "Group", false),
                group_transform_xml(&group.transform)
            );
            for (position, nested) in group.children.iter().enumerate() {
                xml.push_str(&nested_child_xml(nested, position + 2, context, rels));
            }
            xml.push_str("</p:grpSp>");
            xml
        }
        GroupChild::Picture(picture) => format!(
            "<p:pic><p:nvPicPr>{}<p:cNvPicPr/><p:nvPr/></p:nvPicPr>{}{}</p:pic>",
            non_visual_properties(shape_id, picture.descr.as_deref(), "Picture", false),
            blip_fill_xml(picture, context, rels),
            picture_shape_properties(picture, SlidePaint::Inherited, SlidePaint::Inherited)
        ),
        // A text box cannot occur in a presentation: the model refuses it
        // (`PresentationError::TextBoxShapeOnSlide`), so this is unreachable rather
        // than unimplemented. It writes nothing because a nested walk has no error
        // channel; the top-level arm refuses with `ExportError` and is the one a
        // hand-built model would hit first.
        GroupChild::TextBox(_) => String::new(),
    }
}

/// A picture's `p:blipFill`: the blip, its source crop, and the stretch.
///
/// Shared by the top-level and nested picture writers, because a picture's fill is
/// the same construct wherever it sits — and because the two diverging is how the
/// nested one lost its image in the first place.
pub(crate) fn blip_fill_xml(
    picture: &GroupPicture,
    context: &ShapeContext<'_>,
    rels: &mut Relationships,
) -> String {
    let mut xml = String::from("<p:blipFill>");
    if let Some(id) = context.image_id(picture, rels) {
        // `a:alphaModFix` carries the picture's opacity, in thousandths of a
        // percent. Dropped, a half-transparent watermark comes back opaque and
        // covers the slide it was sitting behind.
        match picture
            .opacity
            .filter(|amount| *amount < casual_doc_model::v1::OPACITY_FULL)
        {
            Some(amount) => xml.push_str(&format!(
                r#"<a:blip r:embed="{id}"><a:alphaModFix amt="{amount}"/></a:blip>"#
            )),
            None => xml.push_str(&format!(r#"<a:blip r:embed="{id}"/>"#)),
        }
    }
    // `a:srcRect` is the SOURCE crop, in thousandths of a percent per edge. An
    // omitted crop and a zero crop are the same thing, so each edge is written only
    // when non-zero — and dropping it entirely makes a cropped picture come back
    // showing the part its author cut off.
    if let Some(crop) = picture
        .crop
        .filter(|crop| crop.left != 0 || crop.top != 0 || crop.right != 0 || crop.bottom != 0)
    {
        let mut rect = String::from("<a:srcRect");
        for (attribute, value) in [
            ("l", crop.left),
            ("t", crop.top),
            ("r", crop.right),
            ("b", crop.bottom),
        ] {
            if value != 0 {
                rect.push_str(&format!(r#" {attribute}="{value}""#));
            }
        }
        rect.push_str("/>");
        xml.push_str(&rect);
    }
    xml.push_str("<a:stretch><a:fillRect/></a:stretch></p:blipFill>");
    xml
}

/// The minimal valid `p:txBody` for a shape that carries no text.
const EMPTY_TEXT_BODY: &str =
    "<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr/></a:p></p:txBody>";

/// `p:sp`.
fn shape_xml(node: &SlideNode, shape: &GroupShape, shape_id: usize) -> String {
    let mut xml = format!(
        "<p:sp><p:nvSpPr>{}<p:cNvSpPr/><p:nvPr>{}</p:nvPr></p:nvSpPr>",
        non_visual_properties(shape_id, node.name.as_deref(), "Shape", node.hidden),
        placeholder_body(node.placeholder.as_ref())
    );
    xml.push_str(&shape_properties_xml(shape, node.fill, node.outline));
    match node.text.as_ref() {
        Some(body) => xml.push_str(&text::text_body_xml(body)),
        None => xml.push_str(EMPTY_TEXT_BODY),
    }
    xml.push_str("</p:sp>");
    xml
}

/// `p:pic`.
fn picture_xml(
    node: &SlideNode,
    picture: &GroupPicture,
    shape_id: usize,
    context: &ShapeContext<'_>,
    rels: &mut Relationships,
) -> String {
    let mut xml = format!(
        "<p:pic><p:nvPicPr>{}<p:cNvPicPr/><p:nvPr>{}</p:nvPr></p:nvPicPr>",
        non_visual_properties(
            shape_id,
            node.name.as_deref().or(picture.descr.as_deref()),
            "Picture",
            node.hidden
        ),
        placeholder_body(node.placeholder.as_ref())
    );
    xml.push_str(&blip_fill_xml(picture, context, rels));
    xml.push_str(&picture_shape_properties(picture, node.fill, node.outline));
    xml.push_str("</p:pic>");
    xml
}

/// The media part name for a picture, resolved through the model's own table.
///
/// Both halves must agree: `Definitions::media` maps the picture's `MediaId` to a
/// part name, and `media` must carry that part's bytes. A reference with no bytes
/// writes no `a:blip` at all — a relationship pointing at a part the package does
/// not contain is a package a reader refuses, while a missing picture is one it
/// opens (`109` FID-R-06).
fn media_part_of<'a>(
    picture: &GroupPicture,
    definitions: &'a Definitions,
    media: &BTreeMap<String, Vec<u8>>,
) -> Option<&'a str> {
    let reference = definitions.media.get(&picture.media)?;
    media
        .contains_key(&reference.part_name)
        .then_some(reference.part_name.as_str())
}

/// The folder a part lives in, for resolving a relative relationship target.
fn folder_of(part: &str) -> &str {
    match part.rsplit_once('/') {
        Some((folder, _)) => folder,
        None => "",
    }
}

/// `p:cNvPr`, shared by every shape kind.
pub(crate) fn non_visual_properties(
    shape_id: usize,
    name: Option<&str>,
    fallback: &str,
    hidden: bool,
) -> String {
    let name = name.map_or_else(|| format!("{fallback} {shape_id}"), text::escape);
    let mut xml = format!(r#"<p:cNvPr id="{shape_id}" name="{name}""#);
    // `@hidden` is written only when true: the schema default is false, and a
    // shape that states `hidden="0"` is a diff in every package for no change.
    if hidden {
        xml.push_str(r#" hidden="1""#);
    }
    xml.push_str("/>");
    xml
}

/// `p:nvPr`'s contents: the placeholder, or nothing.
fn placeholder_body(placeholder: Option<&Placeholder>) -> String {
    placeholder.map_or_else(String::new, |placeholder| {
        let mut xml = format!(r#"<p:ph type="{}""#, placeholder.kind.token());
        // `@idx` is omitted at zero, which is the schema default — but the PAIR
        // (type, idx) is what inheritance resolves on, so the attribute is written
        // whenever it is non-zero rather than only when it looks interesting.
        if placeholder.index != 0 {
            xml.push_str(&format!(r#" idx="{}""#, placeholder.index));
        }
        xml.push_str(&format!(r#" sz="{}""#, placeholder.size.token()));
        xml.push_str(&format!(r#" orient="{}""#, placeholder.orientation.token()));
        if placeholder.has_custom_prompt {
            xml.push_str(r#" hasCustomPrompt="1""#);
        }
        xml.push_str("/>");
        xml
    })
}

/// The placeholder wrapped in its own `p:nvPr`, for a group where the element is
/// not already being written.
fn placeholder_xml(placeholder: Option<&Placeholder>) -> String {
    format!("<p:nvPr>{}</p:nvPr>", placeholder_body(placeholder))
}

/// `p:spPr` for a shape.
///
/// # Why the two states are parameters rather than read off the shape
///
/// `GroupShape` holds `Option<Fill>` and `Option<ShapeStroke>`, and `None` is the
/// spelling of two different facts on a slide: "the file said nothing" and "the
/// file said `<a:noFill/>`". Writing a fill only when `Some` therefore turned every
/// deliberately transparent shape opaque on save — PowerPoint reopening the written
/// deck applies the placeholder slot's fill, then the shape's `p:style` theme
/// reference, then the theme default, because that is what an absent fill element
/// means. The states come from the `SlideNode`, which is where the model keeps them.
fn shape_properties_xml(
    shape: &GroupShape,
    fill_state: SlidePaint,
    outline_state: SlidePaint,
) -> String {
    let mut xml = String::from("<p:spPr>");
    xml.push_str(&xfrm_xml(
        shape.offset,
        shape.extent,
        shape.rotation,
        shape.flip_h,
        shape.flip_v,
    ));
    // A recovered `a:custGeom` is re-emitted as the custom geometry it was.
    // Without this the shape is rewritten to `prst="rect"` on every save, which is
    // the one place a round trip DESTROYS data rather than mis-drawing it.
    match shape.path.as_ref() {
        Some(path) => xml.push_str(&custom_geometry_xml(path)),
        None => {
            let preset = shape
                .preset
                .as_deref()
                .or_else(|| shape.geometry.preset_token())
                .unwrap_or("rect");
            xml.push_str(&format!(r#"<a:prstGeom prst="{}">"#, text::escape(preset)));
            xml.push_str("<a:avLst>");
            for adjustment in &shape.adjustments {
                xml.push_str(&format!(
                    r#"<a:gd name="{}" fmla="{}"/>"#,
                    text::escape(&adjustment.name),
                    text::escape(&adjustment.formula)
                ));
            }
            xml.push_str("</a:avLst></a:prstGeom>");
        }
    }
    // `a:noFill` first in both matches, because the model refuses it beside a value
    // (`PresentationError::SuppressedPaintCarriesValue`) — so the arm order states
    // the invariant rather than resolving a conflict that cannot arise. Both sit
    // where `CT_ShapeProperties` puts them: the fill group after the geometry, the
    // `a:ln` after the fill.
    match (fill_state, shape.fill.as_ref()) {
        (SlidePaint::Suppressed, _) => xml.push_str("<a:noFill/>"),
        (_, Some(fill)) => xml.push_str(&fill_xml(fill)),
        (SlidePaint::Inherited | SlidePaint::Authored, None) => {}
    }
    match (outline_state, shape.stroke.as_ref()) {
        // An `a:ln` wrapping nothing but `a:noFill` is how "explicitly unstroked" is
        // spelled; an empty `<a:ln/>` would mean "inherit", which is the opposite.
        (SlidePaint::Suppressed, _) => xml.push_str("<a:ln><a:noFill/></a:ln>"),
        (_, Some(stroke)) => xml.push_str(&outline_xml(stroke)),
        (SlidePaint::Inherited | SlidePaint::Authored, None) => {}
    }
    xml.push_str("</p:spPr>");
    xml
}

/// `p:spPr` for a picture, which is rectangular and carries only a border.
///
/// The two states are parameters for the reason `shape_properties_xml`'s are. A
/// picture's `p:spPr` fill is the fill BEHIND the image and `GroupPicture` has no
/// field for it, so `fill_state` can only ever re-emit `<a:noFill/>` — which is
/// still worth re-emitting, because dropping it makes the box behind a transparent
/// PNG come back filled from the placeholder slot.
fn picture_shape_properties(
    picture: &GroupPicture,
    fill_state: SlidePaint,
    outline_state: SlidePaint,
) -> String {
    let mut xml = String::from("<p:spPr>");
    // The picture's full orientation. An earlier draft passed `None` for the
    // rotation and `false` for the vertical flip, which silently un-rotated and
    // un-flipped every picture that had either — a loss no report could see,
    // because the model held the values and the writer dropped them.
    xml.push_str(&xfrm_xml(
        picture.offset,
        picture.extent,
        picture.rotation,
        picture.flip_h,
        picture.flip_v,
    ));
    xml.push_str(r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#);
    if fill_state.suppresses() {
        xml.push_str("<a:noFill/>");
    }
    match (outline_state, picture.border.as_ref()) {
        (SlidePaint::Suppressed, _) => xml.push_str("<a:ln><a:noFill/></a:ln>"),
        (_, Some(border)) => xml.push_str(&outline_xml(border)),
        (SlidePaint::Inherited | SlidePaint::Authored, None) => {}
    }
    xml.push_str("</p:spPr>");
    xml
}

/// `a:xfrm` for a shape.
fn xfrm_xml(
    offset: PointEmu,
    extent: Extent,
    rotation: Option<i32>,
    flip_h: bool,
    flip_v: bool,
) -> String {
    let mut xml = String::from("<a:xfrm");
    // `@rot` is 1/60000 degree, carried verbatim.
    if let Some(rotation) = rotation {
        xml.push_str(&format!(r#" rot="{rotation}""#));
    }
    if flip_h {
        xml.push_str(r#" flipH="1""#);
    }
    if flip_v {
        xml.push_str(r#" flipV="1""#);
    }
    xml.push_str(&format!(
        r#"><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm>"#,
        offset.x_emu, offset.y_emu, extent.width_emu, extent.height_emu
    ));
    xml
}

/// `a:xfrm` for a group, which additionally states its child coordinate space.
fn group_transform_xml(transform: &GroupTransform) -> String {
    let mut xml = String::from("<a:xfrm");
    if let Some(rotation) = transform.rotation {
        xml.push_str(&format!(r#" rot="{rotation}""#));
    }
    if transform.flip_h {
        xml.push_str(r#" flipH="1""#);
    }
    if transform.flip_v {
        xml.push_str(r#" flipV="1""#);
    }
    xml.push_str(&format!(
        r#"><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/><a:chOff x="{}" y="{}"/><a:chExt cx="{}" cy="{}"/></a:xfrm>"#,
        transform.offset.x_emu,
        transform.offset.y_emu,
        transform.extent.width_emu,
        transform.extent.height_emu,
        transform.child_offset.x_emu,
        transform.child_offset.y_emu,
        transform.child_extent.width_emu,
        transform.child_extent.height_emu
    ));
    xml
}

/// `a:custGeom` with one subpath.
///
/// One subpath because that is what the model carries: the importer admits a
/// single-subpath `a:custGeom` and reports anything wider, so writing a second
/// `a:path` would be writing geometry no model ever held.
fn custom_geometry_xml(path: &ShapePath) -> String {
    let mut xml = String::from(
        "<a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l=\"0\" t=\"0\" r=\"r\" b=\"b\"/><a:pathLst><a:path",
    );
    // `@w`/`@h` are the path's own coordinate space; zero means the points are
    // absolute EMU, and the attributes are omitted in that case because writing
    // `w="0"` would declare a degenerate space rather than no space.
    if path.width_emu != 0 {
        xml.push_str(&format!(r#" w="{}""#, path.width_emu));
    }
    if path.height_emu != 0 {
        xml.push_str(&format!(r#" h="{}""#, path.height_emu));
    }
    xml.push('>');
    for command in &path.commands {
        match command {
            ShapePathCommand::MoveTo { point } => {
                xml.push_str(&format!("<a:moveTo>{}</a:moveTo>", point_xml(*point)));
            }
            ShapePathCommand::LineTo { point } => {
                xml.push_str(&format!("<a:lnTo>{}</a:lnTo>", point_xml(*point)));
            }
            ShapePathCommand::CubicBezTo {
                control1,
                control2,
                point,
            } => {
                xml.push_str(&format!(
                    "<a:cubicBezTo>{}{}{}</a:cubicBezTo>",
                    point_xml(*control1),
                    point_xml(*control2),
                    point_xml(*point)
                ));
            }
            ShapePathCommand::QuadBezTo { control, point } => {
                xml.push_str(&format!(
                    "<a:quadBezTo>{}{}</a:quadBezTo>",
                    point_xml(*control),
                    point_xml(*point)
                ));
            }
            ShapePathCommand::Close => xml.push_str("<a:close/>"),
        }
    }
    xml.push_str("</a:path></a:pathLst></a:custGeom>");
    xml
}

/// An `a:pt` inside a path command.
fn point_xml(point: PointEmu) -> String {
    format!(r#"<a:pt x="{}" y="{}"/>"#, point.x_emu, point.y_emu)
}

/// A shape or background fill.
///
/// Only the solid case is written, and the gradient case is NOT approximated: the
/// presentation importer reads no `a:gradFill` on a slide shape, so a gradient
/// reaching here came from a hand-built model rather than a file. It is written as
/// its first stop's colour, with that stated here rather than discovered — the
/// alternative, writing no fill at all, turns a filled shape into an unfilled one,
/// which reads as deliberate.
fn fill_xml(fill: &Fill) -> String {
    match fill {
        Fill::Solid(color) => format!("<a:solidFill>{}</a:solidFill>", srgb_xml(*color)),
        Fill::Gradient { stops, .. } => {
            let color = stops.first().map_or(
                Rgba {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                |stop| stop.color,
            );
            format!("<a:solidFill>{}</a:solidFill>", srgb_xml(color))
        }
    }
}

/// `a:ln`.
fn outline_xml(stroke: &ShapeStroke) -> String {
    line_properties_xml("a:ln", stroke)
}

/// A `CT_LineProperties` under any tag name.
///
/// `a:ln` on a `p:spPr` and `a:lnL`..`a:lnB` on an `a:tcPr` are the SAME element
/// type under four different names, so there is one writer for all five rather
/// than a table-only copy that would diverge the first time a dash or an
/// arrowhead was added to one of them.
pub(crate) fn line_properties_xml(tag: &str, stroke: &ShapeStroke) -> String {
    let mut xml = format!(r#"<{tag} w="{}">"#, stroke.width_emu);
    xml.push_str(&format!(
        "<a:solidFill>{}</a:solidFill>",
        srgb_xml(stroke.color)
    ));
    if let Some(dash) = stroke.dash {
        xml.push_str(&format!(r#"<a:prstDash val="{}"/>"#, dash_token(dash)));
    }
    // `a:headEnd`/`a:tailEnd` follow the dash in `CT_LineProperties`.
    for (tag, end) in [
        ("a:headEnd", stroke.head_end.as_ref()),
        ("a:tailEnd", stroke.tail_end.as_ref()),
    ] {
        if let Some(end) = end {
            xml.push_str(&format!(r#"<{tag} type="{}"/>"#, line_end_token(end.kind)));
        }
    }
    xml.push_str(&format!("</{tag}>"));
    xml
}

/// An `a:tcPr`'s own fill, which is the same `a:solidFill` a shape writes.
pub(crate) fn cell_fill_xml(fill: &Fill) -> String {
    fill_xml(fill)
}

/// An `a:srgbClr`, with its alpha folded back out into an `a:alpha` child.
///
/// The model stores alpha in 8 bits, so a `ST_Percentage` written back is
/// quantised: 63% arrives as 161/255 and leaves as 63137. That is the whole
/// model's colour precision rather than anything this element does, and it is
/// stable — a reopen reads 161 again.
fn srgb_xml(color: Rgba) -> String {
    let hex = format!("{:02X}{:02X}{:02X}", color.r, color.g, color.b);
    if color.a == u8::MAX {
        format!(r#"<a:srgbClr val="{hex}"/>"#)
    } else {
        let alpha = (u32::from(color.a) * 100_000 + 127) / 255;
        format!(r#"<a:srgbClr val="{hex}"><a:alpha val="{alpha}"/></a:srgbClr>"#)
    }
}

/// A `StyleColor`, which may be a concrete colour, a theme slot, or the formal
/// parameter `a:phClr`.
///
/// Written as what it IS rather than resolved: `a:phClr` means "whatever colour
/// the referencing shape supplies", and substituting a concrete colour here would
/// give every styled shape the same one. A scheme colour is written back as the
/// scheme reference it was, so a deck's theme still governs it.
pub(crate) fn style_color_xml(color: &StyleColor) -> String {
    match color {
        StyleColor::Fixed(rgba) => srgb_xml(*rgba),
        StyleColor::Placeholder(transform) => {
            // `a:phClr` with its transforms. The model has ONE placeholder variant
            // and no scheme-slot variant, so an `a:schemeClr val="accent1"` on a
            // slide does not reach here — the presentation importer reports it
            // rather than modelling it, and writing a slot this model cannot hold
            // would be inventing one.
            let transforms = transform_children(transform);
            if transforms.is_empty() {
                r#"<a:schemeClr val="phClr"/>"#.to_owned()
            } else {
                format!(r#"<a:schemeClr val="phClr">{transforms}</a:schemeClr>"#)
            }
        }
    }
}

/// The colour-transform children of a scheme colour, in thousandths of a percent.
///
/// `EG_ColorTransform` is a repeatable CHOICE, so the file states no required
/// order and this is simply one canonical order — chosen once here so writing a
/// deck twice gives identical bytes. It is deliberately not the order the fold
/// APPLIES them in (`lumMod, lumOff, satMod, tint, shade, alpha`, which is what
/// `casual_doc_model::v1::fold_color_modifiers` documents): each modifier lands in
/// its own field on the way in, so the order it was written in cannot reach the
/// arithmetic, and reordering these five would churn every exported deck's bytes
/// for nothing.
///
/// `a:satMod` is written here as of the shared-model theme lane. Before it, this
/// comment said the modifier "has no field — the model carries five transforms,
/// not six"; it carries six now, the importer reads it, and the shared fold
/// applies it, so a saturation modulation survives a round trip instead of being
/// a loss this writer had no value for.
fn transform_children(transform: &ColorTransform) -> String {
    let mut xml = String::new();
    for (tag, value) in [
        ("a:tint", transform.tint),
        ("a:shade", transform.shade),
        ("a:alpha", transform.alpha),
        ("a:lumMod", transform.lum_mod),
        ("a:lumOff", transform.lum_off),
        ("a:satMod", transform.sat_mod),
    ] {
        if let Some(value) = value {
            xml.push_str(&format!(r#"<{tag} val="{value}"/>"#));
        }
    }
    xml
}

/// Maps a [`DashStyle`] to its `a:prstDash@val` (`ST_PresetLineDashVal`) token.
///
/// The same table the DOCX writer uses. Duplicated rather than shared because that
/// one is private to `casual-doc-export` and this crate must not depend on the
/// document exporter to write a slide; if a third consumer appears, the table
/// should move into the model beside the enum.
const fn dash_token(dash: DashStyle) -> &'static str {
    match dash {
        DashStyle::Solid => "solid",
        DashStyle::Dot => "dot",
        DashStyle::Dash => "dash",
        DashStyle::LargeDash => "lgDash",
        DashStyle::DashDot => "dashDot",
        DashStyle::LargeDashDot => "lgDashDot",
        DashStyle::LargeDashDotDot => "lgDashDotDot",
        DashStyle::SystemDash => "sysDash",
        DashStyle::SystemDot => "sysDot",
        DashStyle::SystemDashDot => "sysDashDot",
        DashStyle::SystemDashDotDot => "sysDashDotDot",
    }
}

/// Maps a [`LineEndKind`] to its `a:headEnd`/`a:tailEnd` `@type`
/// (`ST_LineEndType`) token.
const fn line_end_token(kind: LineEndKind) -> &'static str {
    match kind {
        LineEndKind::None => "none",
        LineEndKind::Triangle => "triangle",
        LineEndKind::Stealth => "stealth",
        LineEndKind::Diamond => "diamond",
        LineEndKind::Oval => "oval",
        LineEndKind::Arrow => "arrow",
    }
}
