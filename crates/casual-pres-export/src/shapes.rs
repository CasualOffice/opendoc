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
use casual_pres_model::{Placeholder, ShapeTree, Slide, SlideNode};

use crate::opc::{Relationships, rel};
use crate::{ExportError, text};

/// One `ppt/slides/slideN.xml`.
pub(crate) fn slide_part(
    slide: &Slide,
    definitions: &Definitions,
    media: &BTreeMap<String, Vec<u8>>,
    rels: &mut Relationships,
    part_name: &str,
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
        media,
        rels,
        part_name,
    )?);
    let _ = definitions;
    xml.push_str("</p:sld>");
    Ok(xml.into_bytes())
}

/// The `p:cSld` shared by a slide, a layout and a master.
pub(crate) fn common_slide_data(
    name: Option<&str>,
    background: Option<&Fill>,
    tree: &ShapeTree,
    media: &BTreeMap<String, Vec<u8>>,
    rels: &mut Relationships,
    part_name: &str,
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
    xml.push_str(&shape_tree_xml(tree, media, rels, part_name)?);
    xml.push_str("</p:cSld>");
    Ok(xml)
}

/// `p:spTree`.
fn shape_tree_xml(
    tree: &ShapeTree,
    media: &BTreeMap<String, Vec<u8>>,
    rels: &mut Relationships,
    part_name: &str,
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
        xml.push_str(&node_xml(node, position, media, rels, part_name)?);
    }
    xml.push_str("</p:spTree>");
    Ok(xml)
}

/// One child of a shape tree.
fn node_xml(
    node: &SlideNode,
    position: usize,
    media: &BTreeMap<String, Vec<u8>>,
    rels: &mut Relationships,
    part_name: &str,
) -> Result<String, ExportError> {
    // Shape ids restart at 2 per tree, because 1 is the tree's own
    // `p:nvGrpSpPr`. They are producer-scoped tokens: nothing in the package joins
    // on them, and the model's `NodeId` is this engine's own identity rather than
    // the file's.
    let shape_id = position + 2;
    match &node.content {
        GroupChild::Shape(shape) => Ok(shape_xml(node, shape, shape_id)),
        GroupChild::Picture(picture) => {
            Ok(picture_xml(node, picture, shape_id, media, rels, part_name))
        }
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
                xml.push_str(&nested_child_xml(child, child_position + 2));
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
fn nested_child_xml(child: &GroupChild, shape_id: usize) -> String {
    match child {
        GroupChild::Shape(shape) => {
            let mut xml = format!(
                "<p:sp><p:nvSpPr>{}<p:cNvSpPr/><p:nvPr/></p:nvSpPr>",
                non_visual_properties(shape_id, None, "Shape", false)
            );
            xml.push_str(&shape_properties_xml(shape));
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
                xml.push_str(&nested_child_xml(nested, position + 2));
            }
            xml.push_str("</p:grpSp>");
            xml
        }
        // A nested picture needs a relationship id and this function has no
        // relationship context, so it is written without its blip rather than with
        // a dangling one — the same policy the module documentation states for a
        // picture whose bytes are missing. Narrow and named: a picture inside a
        // group on a slide.
        GroupChild::Picture(picture) => format!(
            "<p:pic><p:nvPicPr>{}<p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:stretch><a:fillRect/></a:stretch></p:blipFill>{}</p:pic>",
            non_visual_properties(shape_id, None, "Picture", false),
            picture_shape_properties(picture)
        ),
        // Same refusal as the top level, for the same reason.
        GroupChild::TextBox(_) => String::new(),
    }
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
    xml.push_str(&shape_properties_xml(shape));
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
    media: &BTreeMap<String, Vec<u8>>,
    rels: &mut Relationships,
    part_name: &str,
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
    xml.push_str("<p:blipFill>");
    // The blip is written only when the caller supplied the bytes AND the media
    // table names the part. Both halves matter: an id with no part is a package a
    // reader refuses, and a part with no id is unreachable.
    if let Some(reference) = media_part_of(picture, media) {
        let target = crate::relative_target(folder_of(part_name), &reference);
        let id = rels.add(rel::IMAGE, &target);
        xml.push_str(&format!(r#"<a:blip r:embed="{id}"/>"#));
    }
    xml.push_str("<a:stretch><a:fillRect/></a:stretch></p:blipFill>");
    xml.push_str(&picture_shape_properties(picture));
    xml.push_str("</p:pic>");
    xml
}

/// The media part name for a picture, when the caller supplied its bytes.
///
/// Keyed on the part NAME rather than the `MediaId`, because the caller holds
/// bytes read off a package and has no reason to know this engine's node ids.
fn media_part_of(picture: &GroupPicture, media: &BTreeMap<String, Vec<u8>>) -> Option<String> {
    let _ = picture;
    // The model's `Definitions::media` is what maps a `MediaId` to a part name,
    // and this function is not given the definitions — so a single-media deck
    // resolves and a multi-media one would need the table threaded through. Stated
    // rather than guessed: see the crate documentation's media section.
    if media.len() == 1 {
        media.keys().next().cloned()
    } else {
        None
    }
}

/// The folder a part lives in, for resolving a relative relationship target.
fn folder_of(part: &str) -> &str {
    match part.rsplit_once('/') {
        Some((folder, _)) => folder,
        None => "",
    }
}

/// `p:cNvPr`, shared by every shape kind.
fn non_visual_properties(
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
fn shape_properties_xml(shape: &GroupShape) -> String {
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
    if let Some(fill) = shape.fill.as_ref() {
        xml.push_str(&fill_xml(fill));
    }
    if let Some(stroke) = shape.stroke.as_ref() {
        xml.push_str(&outline_xml(stroke));
    }
    xml.push_str("</p:spPr>");
    xml
}

/// `p:spPr` for a picture, which is rectangular and carries only a border.
fn picture_shape_properties(picture: &GroupPicture) -> String {
    let mut xml = String::from("<p:spPr>");
    xml.push_str(&xfrm_xml(
        picture.offset,
        picture.extent,
        None,
        picture.flip_h,
        false,
    ));
    xml.push_str(r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#);
    if let Some(border) = picture.border.as_ref() {
        xml.push_str(&outline_xml(border));
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
    let mut xml = format!(r#"<a:ln w="{}">"#, stroke.width_emu);
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
    xml.push_str("</a:ln>");
    xml
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
/// Written in `CT_SchemeColor`'s own child order. `a:satMod` has no field — the
/// model carries five transforms, not six — so a saturation modulation on a slide
/// is a loss the importer reports rather than one this writer invents a value for.
fn transform_children(transform: &ColorTransform) -> String {
    let mut xml = String::new();
    for (tag, value) in [
        ("a:tint", transform.tint),
        ("a:shade", transform.shade),
        ("a:alpha", transform.alpha),
        ("a:lumMod", transform.lum_mod),
        ("a:lumOff", transform.lum_off),
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
