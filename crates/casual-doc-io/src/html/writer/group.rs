//! Drawings: a group — or a lone shape, which the model holds as a group of
//! one — as an inline SVG, its children mapped from the group's child space.

use casual_doc_layout::page::AnchorContent;
use casual_doc_layout::paint_values::{shape_content, text_box_frame};
use casual_doc_layout::units::{Point, Rect, Size, Twip};
use casual_doc_model::v1::{
    Extent, GroupChild, GroupTransform, MediaId, PointEmu, TextBoxVerticalAnchor,
    WordprocessingGroup,
};

use super::{Cx, Flow, Writer, emu, link_target};
use crate::html::css::Declarations;
use crate::html::{base64, drawing, enforce, escape_attribute};
use crate::{AdapterError, ModelOutcome};

impl Writer<'_> {
    /// A group — and a lone shape, which the model holds as a group of one — as
    /// an inline SVG: each shape drawn from the geometry the page evaluates,
    /// each picture as an `<image>`, each text box as its frame with its text
    /// as HTML in a `<foreignObject>`, nested groups mapped through their own
    /// child space.
    pub(super) fn group(
        &mut self,
        group: &WordprocessingGroup,
        cx: Cx<'_>,
    ) -> Result<(), AdapterError> {
        if cx.depth >= self.limits.max_nesting_depth {
            return Err(AdapterError::new(
                "limit html_nesting_depth exceeded while walking a grouped drawing",
            ));
        }
        let (width, height) = (
            drawing::emu_px(group.extent.width_emu),
            drawing::emu_px(group.extent.height_emu),
        );
        if width <= 0.0 || height <= 0.0 {
            self.losses
                .record("html.group_shape", ModelOutcome::Omitted);
            return Ok(());
        }
        let mut css = Declarations::default();
        css.set("max-width", "100%");
        css.set("height", "auto");
        css.set("overflow", "visible");
        css.set("vertical-align", "baseline");
        let transform = pose(
            group.transform.rotation,
            group.transform.flip_h,
            group.transform.flip_v,
        );
        if !transform.is_empty() {
            css.set("transform", transform);
        }
        if let Some(anchor) = &group.anchor {
            self.place(anchor, &mut css);
        }
        let link = group
            .hyperlink
            .as_ref()
            .map(|link| link_target(&link.target));
        if let Some(link) = &link {
            self.push("<a href=\"")?;
            self.push(&escape_attribute(link))?;
            self.push("\">")?;
        }
        self.push(&format!(
            "<svg class=\"drawing\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" style=\""
        ))?;
        self.push(&escape_attribute(&css.to_css()))?;
        self.push("\">")?;
        let mapper = Mapper::new(&group.transform, 0.0, 0.0, width, height);
        self.group_children(&group.children, &mapper, cx)?;
        self.push("</svg>")?;
        if link.is_some() {
            self.push("</a>")?;
        }
        Ok(())
    }

    fn group_children(
        &mut self,
        children: &[GroupChild],
        mapper: &Mapper,
        cx: Cx<'_>,
    ) -> Result<(), AdapterError> {
        let deeper = Cx {
            depth: cx.depth + 1,
            layer: None,
        };
        for child in children {
            match child {
                GroupChild::Shape(shape) => {
                    let (x, y, w, h) = mapper.rect(shape.offset, shape.extent);
                    let rect = twip_rect(x, y, w, h);
                    let content = shape_content(self.document, shape, rect);
                    let mut svg = String::new();
                    drawing::content(
                        &content,
                        rect,
                        &mut drawing::Ids(&mut self.drawing_ids),
                        &mut svg,
                    );
                    if shape_has_line_ends(&content) {
                        self.losses
                            .record("html.drawing_line_end", ModelOutcome::Degraded);
                    }
                    self.posed(
                        &svg,
                        (x, y, w, h),
                        shape.rotation,
                        shape.flip_h,
                        shape.flip_v,
                    )?;
                }
                GroupChild::Picture(picture) => {
                    let (x, y, w, h) = mapper.rect(picture.offset, picture.extent);
                    let Some(uri) = self.picture_uri(picture.media)? else {
                        continue;
                    };
                    let svg = format!(
                        "<image href=\"{}\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" preserveAspectRatio=\"none\"/>",
                        escape_attribute(&uri)
                    );
                    self.posed(
                        &svg,
                        (x, y, w, h),
                        picture.rotation,
                        picture.flip_h,
                        picture.flip_v,
                    )?;
                }
                GroupChild::TextBox(text_box) => {
                    let (x, y, w, h) = mapper.rect(text_box.offset, text_box.extent);
                    let rect = twip_rect(x, y, w, h);
                    let mut svg = String::new();
                    drawing::content(
                        &text_box_frame(text_box, rect),
                        rect,
                        &mut drawing::Ids(&mut self.drawing_ids),
                        &mut svg,
                    );
                    self.push(&svg)?;
                    let insets = text_box.body_properties.insets;
                    let justify = match text_box.body_properties.vertical_anchor {
                        TextBoxVerticalAnchor::Top => "flex-start",
                        TextBoxVerticalAnchor::Center => "center",
                        TextBoxVerticalAnchor::Bottom => "flex-end",
                    };
                    self.push(&format!(
                        "<foreignObject x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\">\
                         <div xmlns=\"http://www.w3.org/1999/xhtml\" style=\"box-sizing:border-box;width:100%;height:100%;\
                         display:flex;flex-direction:column;justify-content:{justify};padding:{} {} {} {}\">",
                        emu(i64::from(insets.top_emu)),
                        emu(i64::from(insets.right_emu)),
                        emu(i64::from(insets.bottom_emu)),
                        emu(i64::from(insets.left_emu))
                    ))?;
                    let mut flow = Flow::default();
                    self.blocks(&text_box.blocks, deeper, &mut flow)?;
                    self.push("</div></foreignObject>")?;
                }
                GroupChild::Group(nested) => {
                    if deeper.depth >= self.limits.max_nesting_depth {
                        return Err(AdapterError::new(
                            "limit html_nesting_depth exceeded while walking a grouped drawing",
                        ));
                    }
                    let (x, y, w, h) =
                        mapper.rect(nested.transform.offset, nested.transform.extent);
                    let transform = svg_pose(
                        (x, y, w, h),
                        nested.transform.rotation,
                        nested.transform.flip_h,
                        nested.transform.flip_v,
                    );
                    self.push(&format!("<g{transform}>"))?;
                    let inner = Mapper::new(&nested.transform, x, y, w, h);
                    self.group_children(&nested.children, &inner, deeper)?;
                    self.push("</g>")?;
                }
            }
        }
        Ok(())
    }

    /// `svg`, turned and mirrored about the centre of `rect` as the drawing is.
    fn posed(
        &mut self,
        svg: &str,
        rect: (f64, f64, f64, f64),
        rotation: Option<i32>,
        flip_h: bool,
        flip_v: bool,
    ) -> Result<(), AdapterError> {
        let transform = svg_pose(rect, rotation, flip_h, flip_v);
        if transform.is_empty() {
            return self.push(svg);
        }
        self.push(&format!("<g{transform}>"))?;
        self.push(svg)?;
        self.push("</g>")
    }

    /// A picture's bytes as a `data:` URI, or `None` (reported) when they are
    /// absent or over the per-picture ceiling.
    pub(super) fn picture_uri(&mut self, media: MediaId) -> Result<Option<String>, AdapterError> {
        let reference = self.definitions.media.get(&media);
        let bytes = reference.and_then(|entry| self.resources.get(&entry.part_name));
        let Some((entry, bytes)) = reference.zip(bytes) else {
            self.losses
                .record("html.picture_bytes_absent", ModelOutcome::Degraded);
            return Ok(None);
        };
        if bytes.len() > self.limits.max_embedded_bytes {
            self.losses
                .record("html.picture_over_embedding_limit", ModelOutcome::Degraded);
            return Ok(None);
        }
        enforce(
            "html_output_bytes",
            self.body.len().saturating_add(bytes.len() / 3 * 4),
            self.limits.max_output_bytes,
        )?;
        Ok(Some(format!(
            "data:{};base64,{}",
            entry.media_type,
            base64(bytes)
        )))
    }
}

/// Maps a group's child space (EMU) onto the box it is drawn in (CSS pixels).
struct Mapper {
    x: f64,
    y: f64,
    scale_x: f64,
    scale_y: f64,
    child_x: i64,
    child_y: i64,
}

impl Mapper {
    fn new(transform: &GroupTransform, x: f64, y: f64, width: f64, height: f64) -> Self {
        let scale = |own: i64, painted: f64| {
            if own > 0 {
                painted / own as f64
            } else {
                1.0 / 9525.0
            }
        };
        Self {
            x,
            y,
            scale_x: scale(transform.child_extent.width_emu, width),
            scale_y: scale(transform.child_extent.height_emu, height),
            child_x: transform.child_offset.x_emu,
            child_y: transform.child_offset.y_emu,
        }
    }

    /// A child's box, in pixels: `(x, y, width, height)`.
    fn rect(&self, offset: PointEmu, extent: Extent) -> (f64, f64, f64, f64) {
        let round = |value: f64| (value * 100.0).round() / 100.0;
        (
            round(self.x + (offset.x_emu - self.child_x) as f64 * self.scale_x),
            round(self.y + (offset.y_emu - self.child_y) as f64 * self.scale_y),
            round(extent.width_emu as f64 * self.scale_x),
            round(extent.height_emu as f64 * self.scale_y),
        )
    }
}

/// A pixel box as the twip rectangle the page's geometry is evaluated in.
fn twip_rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    let twip = |value: f64| Twip((value * 15.0).round() as i32);
    Rect::new(
        Point::new(twip(x), twip(y)),
        Size::new(twip(width), twip(height)),
    )
}

/// A drawing's rotation (60,000ths of a degree) and mirroring as a CSS
/// transform about its own centre.
fn pose(rotation: Option<i32>, flip_h: bool, flip_v: bool) -> String {
    let mut parts = Vec::new();
    if let Some(rotation) = rotation.filter(|rotation| *rotation != 0) {
        parts.push(format!("rotate({}deg)", f64::from(rotation) / 60_000.0));
    }
    if flip_h {
        parts.push("scaleX(-1)".to_owned());
    }
    if flip_v {
        parts.push("scaleY(-1)".to_owned());
    }
    parts.join(" ")
}

/// The same pose as an SVG `transform` attribute about the centre of `rect`,
/// or nothing when the drawing is neither turned nor mirrored.
fn svg_pose(
    rect: (f64, f64, f64, f64),
    rotation: Option<i32>,
    flip_h: bool,
    flip_v: bool,
) -> String {
    let rotation = rotation.filter(|rotation| *rotation != 0);
    if rotation.is_none() && !flip_h && !flip_v {
        return String::new();
    }
    let (x, y, w, h) = rect;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let mut transform = format!(" transform=\"translate({cx} {cy})");
    if let Some(rotation) = rotation {
        transform.push_str(&format!(" rotate({})", f64::from(rotation) / 60_000.0));
    }
    if flip_h || flip_v {
        transform.push_str(&format!(
            " scale({} {})",
            if flip_h { -1 } else { 1 },
            if flip_v { -1 } else { 1 }
        ));
    }
    transform.push_str(&format!(" translate({} {})\"", -cx, -cy));
    transform
}

/// Whether a drawn line or path has arrowheads, which the SVG does not draw.
fn shape_has_line_ends(content: &AnchorContent) -> bool {
    match content {
        AnchorContent::Line {
            head_end, tail_end, ..
        }
        | AnchorContent::Path {
            head_end, tail_end, ..
        } => head_end.is_some() || tail_end.is_some(),
        _ => false,
    }
}
