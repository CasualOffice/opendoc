//! Legacy VML (`urn:schemas-microsoft-com:vml`) shape parsing into a neutral,
//! renderer-agnostic intermediate (`VmlDrawing`).
//!
//! # Why this module exists
//!
//! VML-primary documents (older Word output, and every document produced by the
//! Aspose/Docx4j-style pipelines that emit `w:pict` instead of `w:drawing`)
//! carry their graphics as `v:*` shapes inside `w:pict`. Today the importer maps
//! only `v:imagedata@r:id` (as an inline picture with no position or size) and
//! drops every positioned `v:rect`/`v:line`/`v:oval`/`v:shape`/`v:group`, so a
//! VML-primary document renders with no horizon rules, no callout boxes, no
//! header/footer text boxes, and no positioned images — the exact "images
//! vanished, no horizon line, no header text boxes" failure reported for the
//! staged Chinese SDS (`SDS_ANTI-T..._ZH.docx`: ~32 `w:pict` carrying `v:rect`
//! rules, `v:group`ed callout boxes, and `v:shape`/`type="#_x0000_t202"` header
//! and footer text boxes). See `docs/46-RENDERING-FIDELITY-GAP-ANALYSIS.md` §F5.
//!
//! # What this module does (and does not) do
//!
//! It parses a VML fragment (the content of a `w:pict`, or any run of sibling
//! `v:*` elements) into a flat `Vec<VmlDrawing>`. Each `VmlDrawing` carries an
//! absolute [`VmlPosition`] in twips, a [`VmlShapeKind`], resolved [`VmlFill`]
//! and [`VmlStroke`], an optional media relationship id (`v:imagedata@r:id`),
//! and an optional [`VmlTextbox`] marker holding the raw `w:txbxContent` XML so
//! a later pass can flow it through the shared block pipeline. `v:group`
//! coordinate systems (`coordorigin`/`coordsize` + the group box) are flattened:
//! every child shape is emitted with its position already transformed into
//! absolute twips, which is exactly what a float/paint layer needs. Parsing is
//! best-effort and infallible — a malformed fragment yields the shapes parsed so
//! far rather than an error, matching the "render what we can" fallback (a VML
//! document renders nothing today, so any recovered shape is strictly better).
//!
//! This module does **not** depend on the document model, layout, or paint. It
//! records neutral position/alignment, wrap, appearance, and text-box metadata;
//! `body.rs` maps that intermediate onto the shared drawing/text-box model and
//! routes `w:txbxContent` through the normal recursive block importer.
//!
//! # Integration spec — how `VmlDrawing` maps onto the float layer (P1F-VML)
//!
//! The body importer maps each `VmlDrawing` onto the floating-object layer as
//! follows:
//!
//! * **Anchor / z-order.** [`VmlPosition::left`]/[`VmlPosition::top`] are the
//!   page-absolute (or, for a `page`/`margin`-relative frame, frame-relative)
//!   twip offsets, and [`VmlPosition::z_index`] is the float layer's z-key —
//!   `VmlPosition::behind_doc()` (a negative z-index) maps to the anchored
//!   drawing's `behindDoc` flag. [`VmlPosition::h_relative`]/
//!   [`VmlPosition::v_relative`] map onto the anchor's `relativeFrom` frames.
//!   Relative alignments, wrapping mode, and four text-clearance distances map
//!   to the corresponding shared anchor fields.
//! * **`Rect`/`RoundRect`/`Line`/`Oval`/`Shape`** → the float layer's shape
//!   paint: a filled/stroked rectangle (with the corner radius for `RoundRect`),
//!   a stroked segment for `Line` (endpoints from [`VmlShapeKind::Line`], else
//!   the box diagonal), an ellipse for `Oval`, and the bounding box (or, later,
//!   the `path`) for a generic `Shape`. [`VmlFill`]/[`VmlStroke`] give the RGBA
//!   fill, RGBA stroke, stroke width (twips), and the on/off toggles.
//! * **`image_rid`** → a positioned image: resolve the relationship id through
//!   the same media index the inline `v:imagedata` path uses, then place the
//!   media at the VML box (position + size), instead of the current inline,
//!   size-less mapping.
//! * **`textbox`** → flow [`VmlTextbox::content_xml`] through the shared block
//!   pipeline (the uniform-flow invariant — the identical block path the body,
//!   headers/footers, and cells use). Header/footer boxes keep their absolute
//!   placement. Body boxes float only for the local paragraph/text/line-relative
//!   top-and-bottom case the current reflow engine can honor; unsafe page-level
//!   overlays remain inline. Insets, vertical text anchor, shape autofit, fill,
//!   and stroke survive either path.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

// --- unit conversion -------------------------------------------------------

const TWIPS_PER_POINT: f64 = 20.0; // 1pt = 20 twips
const TWIPS_PER_INCH: f64 = 1440.0; // 1in = 1440 twips
const TWIPS_PER_CM: f64 = 1440.0 / 2.54; // 1cm
const TWIPS_PER_MM: f64 = 1440.0 / 25.4; // 1mm
const TWIPS_PER_PIXEL: f64 = 1440.0 / 96.0; // 1px @96dpi = 15 twips
const TWIPS_PER_PICA: f64 = 240.0; // 1pc = 12pt
const EMU_PER_TWIP: f64 = 635.0; // 914400 EMU/in / 1440 twips/in

/// A CSS/VML length: a magnitude plus its unit. VML style lengths are usually
/// points, but `in`/`cm`/`mm`/`px`/`pc`/`emu`/twips and bare coordinate numbers
/// all occur; [`Len::to_twips`] normalizes every unit to twips.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Len {
    value: f64,
    unit: Unit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Unit {
    Point,
    Inch,
    Cm,
    Mm,
    Pixel,
    Pica,
    Emu,
    Twip,
    /// A unitless number. In an absolute context this is treated as pixels (the
    /// CSS default); inside a `v:group` it is a coordinate in the group's
    /// `coordsize` space and is transformed rather than unit-converted.
    Bare,
}

impl Len {
    /// The length in twips, rounding to the nearest whole twip. A bare number is
    /// treated as pixels (the CSS default for a unitless length).
    fn to_twips(self) -> i64 {
        let twips = match self.unit {
            Unit::Point => self.value * TWIPS_PER_POINT,
            Unit::Inch => self.value * TWIPS_PER_INCH,
            Unit::Cm => self.value * TWIPS_PER_CM,
            Unit::Mm => self.value * TWIPS_PER_MM,
            Unit::Pixel | Unit::Bare => self.value * TWIPS_PER_PIXEL,
            Unit::Pica => self.value * TWIPS_PER_PICA,
            Unit::Emu => self.value / EMU_PER_TWIP,
            Unit::Twip => self.value,
        };
        round_twips(twips)
    }
}

fn round_twips(value: f64) -> i64 {
    if value.is_finite() {
        value.round() as i64
    } else {
        0
    }
}

/// Parses a CSS/VML length token (e.g. `456.55pt`, `.48001pt`, `1in`, `-15.3pt`,
/// `9131`) into a [`Len`]. Surrounding whitespace is tolerated. Returns `None`
/// for an empty or non-numeric token.
fn parse_len(token: &str) -> Option<Len> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    // Split the leading numeric run (sign, digits, decimal point) from the unit
    // suffix. Exponent notation is not used by VML style lengths.
    let split = token
        .char_indices()
        .find(|(_, c)| !matches!(c, '0'..='9' | '.' | '-' | '+'))
        .map(|(idx, _)| idx)
        .unwrap_or(token.len());
    let (number, suffix) = token.split_at(split);
    let value: f64 = number.parse().ok()?;
    let unit = match suffix.trim() {
        "" => Unit::Bare,
        "pt" => Unit::Point,
        "in" => Unit::Inch,
        "cm" => Unit::Cm,
        "mm" => Unit::Mm,
        "px" => Unit::Pixel,
        "pc" => Unit::Pica,
        "emu" => Unit::Emu,
        "twip" | "twips" => Unit::Twip,
        _ => return None,
    };
    Some(Len { value, unit })
}

// --- neutral intermediate types -------------------------------------------

/// A solid RGBA color parsed from a VML `fillcolor`/`strokecolor` or a
/// `v:fill`/`v:stroke` `color`. A `v:fill` gradient's stops are each a
/// [`VmlColor`] (see [`VmlGradientStop`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmlColor {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel; `255` is opaque (the default when no opacity is given).
    pub a: u8,
}

/// The reference frame a VML position is measured from
/// (`mso-position-horizontal-relative` / `-vertical-relative`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmlRelFrame {
    /// The page margin box.
    Margin,
    /// The physical page.
    Page,
    /// The surrounding text / column.
    Text,
    /// The current character position (inline anchor, horizontal).
    Char,
    /// The current line (inline anchor, vertical).
    Line,
    /// The text column.
    Column,
    /// The paragraph.
    Paragraph,
    /// The physical left margin strip.
    LeftMargin,
    /// The physical right margin strip.
    RightMargin,
    /// The physical top margin strip.
    TopMargin,
    /// The physical bottom margin strip.
    BottomMargin,
    /// The mirrored inside margin area.
    InsideMargin,
    /// The mirrored outside margin area.
    OutsideMargin,
    /// Any frame not modeled above (verbatim token preserved by the follow-up).
    Other,
}

impl VmlRelFrame {
    fn parse(token: &str) -> Self {
        match token.trim() {
            "margin" => Self::Margin,
            "page" => Self::Page,
            "text" => Self::Text,
            "char" => Self::Char,
            "line" => Self::Line,
            "column" => Self::Column,
            "paragraph" => Self::Paragraph,
            "left-margin" => Self::LeftMargin,
            "right-margin" => Self::RightMargin,
            "top-margin" => Self::TopMargin,
            "bottom-margin" => Self::BottomMargin,
            "inner-margin-area" => Self::InsideMargin,
            "outer-margin-area" => Self::OutsideMargin,
            _ => Self::Other,
        }
    }
}

/// Relative horizontal placement within a VML reference frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmlHorizontalAlign {
    /// Flush left.
    Left,
    /// Centered.
    Center,
    /// Flush right.
    Right,
    /// Mirrored inside edge.
    Inside,
    /// Mirrored outside edge.
    Outside,
}

impl VmlHorizontalAlign {
    fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "right" => Some(Self::Right),
            "inside" => Some(Self::Inside),
            "outside" => Some(Self::Outside),
            // `absolute` selects the authored margin-left offset.
            "absolute" => None,
            _ => None,
        }
    }
}

/// Relative vertical placement within a VML reference frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmlVerticalAlign {
    /// Flush top.
    Top,
    /// Centered.
    Center,
    /// Flush bottom.
    Bottom,
    /// Mirrored inside edge.
    Inside,
    /// Mirrored outside edge.
    Outside,
}

impl VmlVerticalAlign {
    fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "top" => Some(Self::Top),
            "center" => Some(Self::Center),
            "bottom" => Some(Self::Bottom),
            "inside" => Some(Self::Inside),
            "outside" => Some(Self::Outside),
            // `absolute` selects the authored margin-top offset.
            "absolute" => None,
            _ => None,
        }
    }
}

/// An absolute VML box, all lengths in twips. `left`/`top` are `None` for an
/// inline shape (one with no absolute anchor, e.g. `mso-position-*-relative:char`
/// with no offset), in which case only `width`/`height` are meaningful.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VmlPosition {
    /// Left edge offset from the reference frame, in twips.
    pub left: Option<i64>,
    /// Top edge offset from the reference frame, in twips.
    pub top: Option<i64>,
    /// Box width in twips.
    pub width: Option<i64>,
    /// Box height in twips.
    pub height: Option<i64>,
    /// The `z-index`. Negative means the shape sits behind the document text.
    pub z_index: Option<i32>,
    /// The horizontal reference frame.
    pub h_relative: Option<VmlRelFrame>,
    /// The vertical reference frame.
    pub v_relative: Option<VmlRelFrame>,
    /// Relative horizontal alignment. `None` means use the absolute `left`.
    pub h_align: Option<VmlHorizontalAlign>,
    /// Relative vertical alignment. `None` means use the absolute `top`.
    pub v_align: Option<VmlVerticalAlign>,
}

impl VmlPosition {
    /// Whether the shape is painted behind the document text (a negative
    /// `z-index`, VML's encoding of `behindDoc`).
    pub fn behind_doc(&self) -> bool {
        matches!(self.z_index, Some(z) if z < 0)
    }
}

/// VML text wrapping around a positioned shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmlWrapMode {
    /// Rectangular side wrapping.
    Square,
    /// Contour wrapping.
    Tight,
    /// Through-contour wrapping.
    Through,
    /// Text only above and below.
    TopAndBottom,
    /// No exclusion.
    None,
}

impl VmlWrapMode {
    fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "square" => Some(Self::Square),
            "tight" => Some(Self::Tight),
            "through" => Some(Self::Through),
            "topandbottom" | "top-and-bottom" => Some(Self::TopAndBottom),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

/// Wrap mode and independent text-clearance distances for a VML shape.
///
/// `mode == None` means the source did not declare a wrapping mode; it is
/// distinct from an explicit [`VmlWrapMode::None`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VmlWrap {
    /// The authored wrapping mode.
    pub mode: Option<VmlWrapMode>,
    /// Top, bottom, left, and right clearance in twips.
    pub distances_twips: [Option<i64>; 4],
}

/// A shape's fill: whether it is filled, its primary flat color, and — for a
/// `v:fill type="gradient"`/`"gradientRadial"` — the parsed [`VmlGradient`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmlFill {
    /// Whether the shape is filled (`filled`, default `true`).
    pub on: bool,
    /// The primary fill color, if a `fillcolor`/`v:fill@color` was given. It is
    /// the flat-color fallback when [`VmlFill::gradient`] is `None`, and the first
    /// stop's color when a gradient is present.
    pub color: Option<VmlColor>,
    /// The parsed gradient, present only when the fill declared a gradient `type`
    /// and yielded at least two stops. When present the shape paints as a gradient
    /// through the same shared path DrawingML gradients use; otherwise it falls
    /// back to the flat [`VmlFill::color`].
    pub gradient: Option<VmlGradient>,
}

/// A parsed VML gradient fill (`v:fill type="gradient"`/`"gradientRadial"`): its
/// ordered color stops and geometry, shaped to map directly onto the shared
/// model `Fill::Gradient`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmlGradient {
    /// The gradient stops in ascending position order; always at least two.
    pub stops: Vec<VmlGradientStop>,
    /// Linear (with a converted sweep angle) or radial geometry.
    pub kind: VmlGradientKind,
}

/// One VML gradient stop: a position and the resolved color painted there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmlGradientStop {
    /// The stop position in per-100000 units (`0` = start, `100000` = end),
    /// matching the shared model's `GradientStop::position`.
    pub position: i32,
    /// The resolved stop color.
    pub color: VmlColor,
}

/// The geometry of a VML gradient.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmlGradientKind {
    /// A linear sweep. `angle` is in 60000ths of a degree, clockwise from the
    /// positive x-axis — the DrawingML `a:lin@ang` convention the model and render
    /// backend already use — converted from VML's own `angle` attribute.
    Linear {
        /// The sweep angle in 60000ths of a degree (DrawingML convention).
        angle: i32,
    },
    /// A radial fill (`type="gradientRadial"`), collapsed to a concentric fill
    /// centered on the box (matching the model's `GradientKind::Radial`).
    Radial,
}

/// A shape's stroke: whether it is stroked, its RGBA color, and its width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmlStroke {
    /// Whether the shape is stroked (`stroked`, default `true`).
    pub on: bool,
    /// The stroke color, if a `strokecolor`/`v:stroke@color` was given.
    pub color: Option<VmlColor>,
    /// The stroke width in twips, if a `strokeweight` was given.
    pub weight_twips: Option<i64>,
}

/// A marker that a shape carried a `v:textbox`/`w:txbxContent`. The content is
/// **not** flowed here — the raw `w:txbxContent` XML is preserved so a follow-up
/// can route it through the shared block pipeline and place it at the VML box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmlTextbox {
    /// The internal margins from `v:textbox@inset` (left, top, right, bottom),
    /// in twips; each entry is `None` when that inset was absent.
    pub inset_twips: [Option<i64>; 4],
    /// Vertical placement of text inside the box (`v-text-anchor`).
    pub vertical_anchor: Option<VmlTextAnchor>,
    /// Whether the shape grows to contain the text (`mso-fit-shape-to-text`).
    pub fit_shape_to_text: bool,
    /// The raw inner XML of `w:txbxContent`, if present.
    pub content_xml: Option<String>,
}

/// The supported vertical portion of VML's `v-text-anchor` vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmlTextAnchor {
    /// Top-aligned, including top baseline/center variants.
    Top,
    /// Vertically centered.
    Middle,
    /// Bottom-aligned, including bottom baseline/center variants.
    Bottom,
}

impl VmlTextAnchor {
    fn parse(token: &str) -> Option<Self> {
        let token = token.trim().to_ascii_lowercase();
        if token.starts_with("top") {
            Some(Self::Top)
        } else if token.starts_with("middle") {
            Some(Self::Middle)
        } else if token.starts_with("bottom") {
            Some(Self::Bottom)
        } else {
            None
        }
    }
}

/// The geometric kind of a VML shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VmlShapeKind {
    /// `v:rect` — a rectangle covering the position box.
    Rect,
    /// `v:roundrect` — a rounded rectangle.
    RoundRect {
        /// The corner radius in twips, if derivable from `arcsize` and the box.
        corner_radius_twips: Option<i64>,
    },
    /// `v:line` — a straight segment. Endpoints are absolute twips when `from`/
    /// `to` are given; otherwise the box diagonal is the segment.
    Line {
        /// The start point `(x, y)` in twips, if `from` was given.
        from: Option<(i64, i64)>,
        /// The end point `(x, y)` in twips, if `to` was given.
        to: Option<(i64, i64)>,
    },
    /// `v:oval` — an ellipse inscribed in the position box.
    Oval,
    /// `v:shape` (or `v:polyline`/`v:curve`) — a generic shape. Rendered as its
    /// bounding box until the `path` is honored.
    Shape {
        /// The raw `path` attribute, if present.
        path: Option<String>,
        /// The `coordsize` `(w, h)` the path is expressed in, if present.
        coordsize: Option<(i64, i64)>,
    },
}

/// The alignment of an `o:hr` horizontal rule (`o:hralign`), default left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VmlHrAlign {
    /// Flush with the content's leading edge (default).
    #[default]
    Left,
    /// Centered in the content width.
    Center,
    /// Flush with the content's trailing edge.
    Right,
}

/// The horizontal-rule marker carried by a `v:rect` with `o:hr="t"` (Word's
/// "Insert → Horizontal Line"). Such a rule spans the full content width (its
/// CSS `width` is ignored), is `height` twips thick, and is filled with its
/// `fillcolor`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VmlHr {
    /// Alignment within the content width (`o:hralign`).
    pub align: VmlHrAlign,
    /// Width as a fraction of the content width in per-mille (`o:hrpct`,
    /// `1000` = full width); `None` when absent (full width).
    pub pct_permille: Option<u16>,
}

/// One parsed VML shape: an absolute box, its geometry, fill/stroke, an optional
/// image relationship, and an optional text-box marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmlDrawing {
    /// The shape's `id`, if any (diagnostic / stable identity).
    pub id: Option<String>,
    /// The shape geometry.
    pub kind: VmlShapeKind,
    /// The absolute position and size, in twips.
    pub position: VmlPosition,
    /// The fill.
    pub fill: VmlFill,
    /// The stroke.
    pub stroke: VmlStroke,
    /// Text wrapping and clearance around the shape.
    pub wrap: VmlWrap,
    /// The media relationship id from `v:imagedata@r:id`, if present.
    pub image_rid: Option<String>,
    /// The text-box marker, if the shape carried a `v:textbox`.
    pub textbox: Option<VmlTextbox>,
    /// The horizontal-rule marker, if the shape carried `o:hr="t"`.
    pub hr: Option<VmlHr>,
}

// --- style parsing ---------------------------------------------------------

/// The parsed `style` attribute: an ordered list of `property:value` pairs.
struct StyleProps {
    entries: Vec<(String, String)>,
}

impl StyleProps {
    fn parse(style: &str) -> Self {
        let mut entries = Vec::new();
        for decl in style.split(';') {
            let decl = decl.trim();
            if decl.is_empty() {
                continue;
            }
            if let Some((key, value)) = decl.split_once(':') {
                entries.push((key.trim().to_ascii_lowercase(), value.trim().to_string()));
            }
        }
        Self { entries }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn len(&self, key: &str) -> Option<Len> {
        self.get(key).and_then(parse_len)
    }
}

impl VmlWrap {
    fn from_style(style: &StyleProps) -> Self {
        Self {
            mode: style.get("mso-wrap-mode").and_then(VmlWrapMode::parse),
            distances_twips: [
                style.len("mso-wrap-distance-top").map(Len::to_twips),
                style.len("mso-wrap-distance-bottom").map(Len::to_twips),
                style.len("mso-wrap-distance-left").map(Len::to_twips),
                style.len("mso-wrap-distance-right").map(Len::to_twips),
            ],
        }
    }

    fn inherit(mut self, parent: Option<Self>) -> Self {
        let Some(parent) = parent else {
            return self;
        };
        if self.mode.is_none() {
            self.mode = parent.mode;
        }
        for (distance, inherited) in self.distances_twips.iter_mut().zip(parent.distances_twips) {
            if distance.is_none() {
                *distance = inherited;
            }
        }
        self
    }

    fn apply_element(&mut self, element: &BytesStart<'_>) {
        if let Some(mode) = attr(element, b"type")
            .as_deref()
            .and_then(VmlWrapMode::parse)
        {
            self.mode = Some(mode);
        }
    }
}

// --- attribute helpers -----------------------------------------------------

/// Reads an attribute by local name, unescaping XML character references. Kept
/// local to this module so the parser has no coupling to shared importer files.
fn attr(element: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    for attribute in element.attributes() {
        let attribute = attribute.ok()?;
        if attribute.key.local_name().as_ref() == name {
            let raw = std::str::from_utf8(attribute.value.as_ref()).ok()?;
            return quick_xml::escape::unescape(raw)
                .ok()
                .map(|value| value.into_owned());
        }
    }
    None
}

/// Parses a VML boolean. VML uses `t`/`f` as well as `true`/`false`/`0`/`1`.
/// Only the explicit false spellings are false; anything else (including a bare
/// present attribute) is true.
fn parse_bool(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "f" | "false" | "0" | "off" | "no"
    )
}

/// Parses a VML color: `#rrggbb`, `#rgb`, or a small set of named colors.
/// Returns an opaque [`VmlColor`]; opacity is layered on separately.
fn parse_color(value: &str) -> Option<VmlColor> {
    let value = value.trim();
    // A VML color may carry a trailing named modifier (e.g. `#ff0000 lighten`);
    // take the first whitespace-delimited token.
    let token = value.split_whitespace().next()?;
    if let Some(hex) = token.strip_prefix('#') {
        return parse_hex_color(hex);
    }
    named_color(&token.to_ascii_lowercase())
}

fn parse_hex_color(hex: &str) -> Option<VmlColor> {
    let expand = |s: &str| -> Option<[u8; 3]> {
        match s.len() {
            6 => Some([
                u8::from_str_radix(&s[0..2], 16).ok()?,
                u8::from_str_radix(&s[2..4], 16).ok()?,
                u8::from_str_radix(&s[4..6], 16).ok()?,
            ]),
            3 => {
                let dup = |c: &str| u8::from_str_radix(&format!("{c}{c}"), 16).ok();
                Some([dup(&hex[0..1])?, dup(&hex[1..2])?, dup(&hex[2..3])?])
            }
            _ => None,
        }
    };
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let [r, g, b] = expand(hex)?;
    Some(VmlColor { r, g, b, a: 255 })
}

fn named_color(name: &str) -> Option<VmlColor> {
    let rgb = match name {
        "black" => (0x00, 0x00, 0x00),
        "white" => (0xff, 0xff, 0xff),
        "red" => (0xff, 0x00, 0x00),
        "green" => (0x00, 0x80, 0x00),
        "lime" => (0x00, 0xff, 0x00),
        "blue" => (0x00, 0x00, 0xff),
        "yellow" => (0xff, 0xff, 0x00),
        "gray" | "grey" => (0x80, 0x80, 0x80),
        "silver" => (0xc0, 0xc0, 0xc0),
        "maroon" => (0x80, 0x00, 0x00),
        "navy" => (0x00, 0x00, 0x80),
        "olive" => (0x80, 0x80, 0x00),
        "purple" => (0x80, 0x00, 0x80),
        "teal" => (0x00, 0x80, 0x80),
        "aqua" | "cyan" => (0x00, 0xff, 0xff),
        "fuchsia" | "magenta" => (0xff, 0x00, 0xff),
        _ => return None,
    };
    Some(VmlColor {
        r: rgb.0,
        g: rgb.1,
        b: rgb.2,
        a: 255,
    })
}

/// Parses a VML opacity token (`0.5`, `50%`, or a `65536`-based `32768f`) to an
/// alpha byte.
fn parse_opacity(value: &str) -> Option<u8> {
    let value = value.trim();
    let fraction = if let Some(num) = value.strip_suffix('f') {
        num.parse::<f64>().ok()? / 65536.0
    } else if let Some(num) = value.strip_suffix('%') {
        num.parse::<f64>().ok()? / 100.0
    } else {
        value.parse::<f64>().ok()?
    };
    Some((fraction.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn parse_pair_i64(value: &str) -> Option<(i64, i64)> {
    let (x, y) = value.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// Converts a VML `v:fill@angle` (degrees) to the model/DrawingML sweep angle in
/// 60000ths of a degree (the `a:lin@ang` convention: clockwise from the positive
/// x-axis). VML's fill angle is `0` for a horizontal left→right sweep and runs
/// opposite the DrawingML axis, so the equivalent DrawingML angle is `180° + vml`
/// normalized to `[0°, 360°)`. Cross-checked against the VML reference example: a
/// `45°` fill puts `color2` in the top-left corner and `color` in the
/// bottom-right, i.e. a `225°` DrawingML sweep (`stop@0`→`stop@1` pointing up-left).
fn vml_fill_angle_to_model(vml_degrees: f64) -> i32 {
    let degrees = if vml_degrees.is_finite() {
        (180.0 + vml_degrees).rem_euclid(360.0)
    } else {
        180.0
    };
    (degrees * 60_000.0).round() as i32
}

/// Parses a VML `v:fill@colors` list (`"0 #ff0000;.5 lime;1 #0000ff"`): a
/// `;`-separated list of `position color` pairs. Malformed entries are skipped.
fn parse_gradient_colors(list: &str) -> Vec<(i32, VmlColor)> {
    let mut out = Vec::new();
    for entry in list.split(';') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let mut parts = entry.split_whitespace();
        let Some(position) = parts.next().and_then(parse_gradient_position) else {
            continue;
        };
        let Some(color) = parts.next().and_then(parse_color) else {
            continue;
        };
        out.push((position, color));
    }
    out
}

/// Parses a VML gradient stop position to per-100000 units: a fraction (`0`..`1`),
/// a percentage (`50%`), or a `65536`-based `f` value (`32768f`).
fn parse_gradient_position(token: &str) -> Option<i32> {
    let token = token.trim();
    let fraction = if let Some(pct) = token.strip_suffix('%') {
        pct.parse::<f64>().ok()? / 100.0
    } else if let Some(f) = token.strip_suffix('f') {
        f.parse::<f64>().ok()? / 65536.0
    } else {
        token.parse::<f64>().ok()?
    };
    Some((fraction.clamp(0.0, 1.0) * 100_000.0).round() as i32)
}

// --- group coordinate transform -------------------------------------------

/// A `v:group`'s resolved affine transform from its local coordinate space to
/// absolute twips, plus the z-order / reference frames its descendants inherit.
///
/// A local coordinate `x` maps to absolute twips `off_x + x * scale_x` (and
/// likewise for `y`). `off_*` is `None` when the group has no absolute anchor
/// (an inline group), in which case descendants keep absolute `left`/`top` of
/// `None` but still receive scaled `width`/`height`.
#[derive(Clone, Copy)]
struct GroupCtx {
    off_x: Option<f64>,
    scale_x: f64,
    off_y: Option<f64>,
    scale_y: f64,
    z_index: Option<i32>,
    h_relative: Option<VmlRelFrame>,
    v_relative: Option<VmlRelFrame>,
    h_align: Option<VmlHorizontalAlign>,
    v_align: Option<VmlVerticalAlign>,
    wrap: VmlWrap,
}

impl GroupCtx {
    fn map_x(&self, local: f64) -> Option<f64> {
        self.off_x.map(|off| off + local * self.scale_x)
    }

    fn map_y(&self, local: f64) -> Option<f64> {
        self.off_y.map(|off| off + local * self.scale_y)
    }
}

/// Builds a group's transform from its `style` and `coordorigin`/`coordsize`,
/// composing through the parent group (if the group is itself nested).
fn build_group_ctx(
    element: &BytesStart<'_>,
    style: &StyleProps,
    parent: Option<&GroupCtx>,
) -> GroupCtx {
    // The group box in absolute twips: from the style directly at the top level,
    // or via the parent transform when nested (child coords are parent-local).
    let (abs_left, abs_top, width_twips, height_twips) = match parent {
        None => (
            abs_left_twips(style).map(|v| v as f64),
            abs_top_twips(style).map(|v| v as f64),
            style.len("width").map(|l| l.to_twips() as f64),
            style.len("height").map(|l| l.to_twips() as f64),
        ),
        Some(p) => {
            let raw = |k: &str| style.len(k).map(|l| l.value);
            (
                raw("left").and_then(|v| p.map_x(v)),
                raw("top").and_then(|v| p.map_y(v)),
                raw("width").map(|v| v * p.scale_x),
                raw("height").map(|v| v * p.scale_y),
            )
        }
    };

    let (origin_x, origin_y) = attr(element, b"coordorigin")
        .as_deref()
        .and_then(parse_pair_i64)
        .unwrap_or((0, 0));
    // VML's default coordsize is 1000x1000; a zero dimension is degenerate.
    let (coord_w, coord_h) = attr(element, b"coordsize")
        .as_deref()
        .and_then(parse_pair_i64)
        .unwrap_or((1000, 1000));

    let scale_x = match width_twips {
        Some(w) if coord_w != 0 => w / coord_w as f64,
        _ => 1.0,
    };
    let scale_y = match height_twips {
        Some(h) if coord_h != 0 => h / coord_h as f64,
        _ => 1.0,
    };

    GroupCtx {
        off_x: abs_left.map(|l| l - origin_x as f64 * scale_x),
        scale_x,
        off_y: abs_top.map(|t| t - origin_y as f64 * scale_y),
        scale_y,
        z_index: style
            .get("z-index")
            .and_then(|z| z.trim().parse::<i32>().ok())
            .or_else(|| parent.and_then(|p| p.z_index)),
        h_relative: style
            .get("mso-position-horizontal-relative")
            .map(VmlRelFrame::parse)
            .or_else(|| parent.and_then(|p| p.h_relative)),
        v_relative: style
            .get("mso-position-vertical-relative")
            .map(VmlRelFrame::parse)
            .or_else(|| parent.and_then(|p| p.v_relative)),
        h_align: style
            .get("mso-position-horizontal")
            .and_then(VmlHorizontalAlign::parse)
            .or_else(|| parent.and_then(|p| p.h_align)),
        v_align: style
            .get("mso-position-vertical")
            .and_then(VmlVerticalAlign::parse)
            .or_else(|| parent.and_then(|p| p.v_align)),
        wrap: VmlWrap::from_style(style).inherit(parent.map(|p| p.wrap)),
    }
}

/// The absolute left edge in twips at the top level: `margin-left` wins over a
/// bare `left` (which, absent a group, is an absolute offset).
fn abs_left_twips(style: &StyleProps) -> Option<i64> {
    style
        .len("margin-left")
        .or_else(|| style.len("left"))
        .map(Len::to_twips)
}

fn abs_top_twips(style: &StyleProps) -> Option<i64> {
    style
        .len("margin-top")
        .or_else(|| style.len("top"))
        .map(Len::to_twips)
}

// --- shape accumulation ----------------------------------------------------

#[derive(Clone, Copy)]
enum ShapeLocal {
    Rect,
    RoundRect,
    Line,
    Oval,
    Shape,
}

/// A shape under construction between its start and end tags (or built and
/// finalized immediately for a self-closing shape).
struct ShapeBuilder {
    local: ShapeLocal,
    id: Option<String>,
    style: StyleProps,
    filled_attr: Option<bool>,
    fill_color: Option<VmlColor>,
    fill_opacity: Option<u8>,
    /// The `v:fill@type` (lowercased), e.g. `gradient`/`gradientradial`.
    fill_type: Option<String>,
    /// The second gradient color (`v:fill@color2`), the stop at position `1`.
    fill_color2: Option<VmlColor>,
    /// The opacity applied to `color2` (`v:fill@opacity2`).
    fill_opacity2: Option<u8>,
    /// The gradient sweep angle in degrees (`v:fill@angle`, VML convention).
    fill_angle: Option<f64>,
    /// The raw intermediate-stop list (`v:fill@colors`).
    fill_colors: Option<String>,
    stroked_attr: Option<bool>,
    stroke_color: Option<VmlColor>,
    stroke_weight: Option<Len>,
    stroke_child_on: Option<bool>,
    image_rid: Option<String>,
    path_attr: Option<String>,
    coordsize_attr: Option<(i64, i64)>,
    arcsize: Option<f64>,
    from: Option<String>,
    to: Option<String>,
    textbox: Option<VmlTextbox>,
    hr: Option<VmlHr>,
    wrap: VmlWrap,
}

impl ShapeBuilder {
    fn new(local: ShapeLocal, element: &BytesStart<'_>) -> Self {
        let style = StyleProps::parse(&attr(element, b"style").unwrap_or_default());
        Self {
            local,
            id: attr(element, b"id"),
            filled_attr: attr(element, b"filled").as_deref().map(parse_bool),
            fill_color: attr(element, b"fillcolor").as_deref().and_then(parse_color),
            fill_opacity: None,
            // Gradient descriptors live on the `<v:fill>` child, not the shape
            // element (whose `type` names a `v:shapetype`, not a fill), so they are
            // captured in `apply_fill`.
            fill_type: None,
            fill_color2: None,
            fill_opacity2: None,
            fill_angle: None,
            fill_colors: None,
            stroked_attr: attr(element, b"stroked").as_deref().map(parse_bool),
            stroke_color: attr(element, b"strokecolor")
                .as_deref()
                .and_then(parse_color),
            stroke_weight: attr(element, b"strokeweight")
                .as_deref()
                .and_then(parse_len),
            stroke_child_on: None,
            image_rid: None,
            path_attr: attr(element, b"path"),
            coordsize_attr: attr(element, b"coordsize")
                .as_deref()
                .and_then(parse_pair_i64),
            arcsize: attr(element, b"arcsize").as_deref().and_then(parse_arcsize),
            from: attr(element, b"from"),
            to: attr(element, b"to"),
            textbox: None,
            hr: parse_hr(element),
            wrap: VmlWrap::from_style(&style),
            style,
        }
    }

    /// Applies a `v:fill` child (color / opacity fallbacks, plus the gradient
    /// descriptors that only appear on this element: `type`, `color2`, `angle`,
    /// and the `colors` multi-stop list).
    fn apply_fill(&mut self, element: &BytesStart<'_>) {
        if self.fill_color.is_none() {
            self.fill_color = attr(element, b"color").as_deref().and_then(parse_color);
        }
        if let Some(op) = attr(element, b"opacity").as_deref().and_then(parse_opacity) {
            self.fill_opacity = Some(op);
        }
        if self.fill_type.is_none() {
            self.fill_type = attr(element, b"type").map(|t| t.trim().to_ascii_lowercase());
        }
        if self.fill_color2.is_none() {
            self.fill_color2 = attr(element, b"color2").as_deref().and_then(parse_color);
        }
        if let Some(op) = attr(element, b"opacity2")
            .as_deref()
            .and_then(parse_opacity)
        {
            self.fill_opacity2 = Some(op);
        }
        if self.fill_angle.is_none() {
            self.fill_angle = attr(element, b"angle")
                .as_deref()
                .and_then(|token| token.trim().parse::<f64>().ok());
        }
        if self.fill_colors.is_none() {
            self.fill_colors = attr(element, b"colors");
        }
    }

    /// Applies a `v:stroke` child (color / weight / on-off fallbacks).
    fn apply_stroke(&mut self, element: &BytesStart<'_>) {
        if self.stroke_color.is_none() {
            self.stroke_color = attr(element, b"color").as_deref().and_then(parse_color);
        }
        if self.stroke_weight.is_none() {
            self.stroke_weight = attr(element, b"weight").as_deref().and_then(parse_len);
        }
        if let Some(on) = attr(element, b"on").as_deref().map(parse_bool) {
            self.stroke_child_on = Some(on);
        }
    }

    fn apply_wrap(&mut self, element: &BytesStart<'_>) {
        self.wrap.apply_element(element);
    }

    /// Builds the typed [`VmlGradient`] when the fill declared a gradient `type`
    /// and yields at least two stops: the primary color (already opacity-adjusted)
    /// at position `0`, any `colors="…"` intermediate stops, and `color2` at
    /// position `100000`. Returns `None` for a non-gradient fill, or one too
    /// sparse to form a gradient, so the caller keeps the flat-color fallback.
    fn build_gradient(&self, primary: Option<VmlColor>) -> Option<VmlGradient> {
        let kind = match self.fill_type.as_deref() {
            Some("gradient") => VmlGradientKind::Linear {
                angle: vml_fill_angle_to_model(self.fill_angle.unwrap_or(0.0)),
            },
            Some("gradientradial") => VmlGradientKind::Radial,
            _ => return None,
        };
        // Push the explicit endpoint colors first, then the intermediate list, so
        // that when a `colors` entry duplicates an endpoint position the explicit
        // endpoint wins (stable sort + `dedup_by_key` keep the first of each run).
        let mut stops: Vec<VmlGradientStop> = Vec::new();
        if let Some(color) = primary {
            stops.push(VmlGradientStop { position: 0, color });
        }
        if let Some(mut color) = self.fill_color2 {
            if let Some(a) = self.fill_opacity2 {
                color.a = a;
            }
            stops.push(VmlGradientStop {
                position: 100_000,
                color,
            });
        }
        if let Some(list) = self.fill_colors.as_deref() {
            for (position, color) in parse_gradient_colors(list) {
                stops.push(VmlGradientStop { position, color });
            }
        }
        stops.sort_by_key(|stop| stop.position);
        stops.dedup_by_key(|stop| stop.position);
        if stops.len() < 2 {
            return None;
        }
        Some(VmlGradient { stops, kind })
    }

    fn finalize(self, group: Option<&GroupCtx>) -> VmlDrawing {
        let position = self.resolve_position(group);
        let color = self.fill_color.map(|mut c| {
            if let Some(a) = self.fill_opacity {
                c.a = a;
            }
            c
        });
        let fill = VmlFill {
            on: self.filled_attr.unwrap_or(true),
            gradient: self.build_gradient(color),
            color,
        };
        let stroke_on = self.stroke_child_on.unwrap_or(true) && self.stroked_attr.unwrap_or(true);
        let stroke = VmlStroke {
            on: stroke_on,
            color: self.stroke_color,
            weight_twips: self.stroke_weight.map(Len::to_twips),
        };
        let kind = self.resolve_kind(&position, group);
        VmlDrawing {
            id: self.id,
            kind,
            position,
            fill,
            stroke,
            wrap: self.wrap.inherit(group.map(|ctx| ctx.wrap)),
            image_rid: self.image_rid,
            textbox: self.textbox,
            hr: self.hr,
        }
    }

    /// Resolves the absolute box, applying the enclosing group transform when
    /// the shape is a group child (its `left`/`top` are group-local).
    fn resolve_position(&self, group: Option<&GroupCtx>) -> VmlPosition {
        let (left, top, width, height) = match group {
            None => (
                abs_left_twips(&self.style),
                abs_top_twips(&self.style),
                self.style.len("width").map(Len::to_twips),
                self.style.len("height").map(Len::to_twips),
            ),
            Some(ctx) => {
                let raw = |k: &str| self.style.len(k).map(|l| l.value);
                (
                    raw("left").and_then(|v| ctx.map_x(v)).map(round_twips),
                    raw("top").and_then(|v| ctx.map_y(v)).map(round_twips),
                    raw("width").map(|v| round_twips(v * ctx.scale_x)),
                    raw("height").map(|v| round_twips(v * ctx.scale_y)),
                )
            }
        };
        VmlPosition {
            left,
            top,
            width,
            height,
            z_index: self
                .style
                .get("z-index")
                .and_then(|z| z.trim().parse::<i32>().ok())
                .or_else(|| group.and_then(|g| g.z_index)),
            h_relative: self
                .style
                .get("mso-position-horizontal-relative")
                .map(VmlRelFrame::parse)
                .or_else(|| group.and_then(|g| g.h_relative)),
            v_relative: self
                .style
                .get("mso-position-vertical-relative")
                .map(VmlRelFrame::parse)
                .or_else(|| group.and_then(|g| g.v_relative)),
            h_align: self
                .style
                .get("mso-position-horizontal")
                .and_then(VmlHorizontalAlign::parse)
                .or_else(|| group.and_then(|g| g.h_align)),
            v_align: self
                .style
                .get("mso-position-vertical")
                .and_then(VmlVerticalAlign::parse)
                .or_else(|| group.and_then(|g| g.v_align)),
        }
    }

    fn resolve_kind(&self, position: &VmlPosition, group: Option<&GroupCtx>) -> VmlShapeKind {
        match self.local {
            ShapeLocal::Rect => VmlShapeKind::Rect,
            ShapeLocal::Oval => VmlShapeKind::Oval,
            ShapeLocal::RoundRect => VmlShapeKind::RoundRect {
                corner_radius_twips: corner_radius(self.arcsize, position),
            },
            ShapeLocal::Line => VmlShapeKind::Line {
                from: self.resolve_endpoint(self.from.as_deref(), position, group, false),
                to: self.resolve_endpoint(self.to.as_deref(), position, group, true),
            },
            ShapeLocal::Shape => VmlShapeKind::Shape {
                path: self.path_attr.clone(),
                coordsize: self.coordsize_attr,
            },
        }
    }

    /// Resolves a `v:line` endpoint. A bare coordinate inside a group is mapped
    /// through the group transform; a unit-bearing token converts directly. When
    /// `from`/`to` is absent the box diagonal is used (`false` → top-left corner,
    /// `true` → bottom-right corner).
    fn resolve_endpoint(
        &self,
        token: Option<&str>,
        position: &VmlPosition,
        group: Option<&GroupCtx>,
        far: bool,
    ) -> Option<(i64, i64)> {
        match token {
            Some(pair) => {
                let (xs, ys) = pair.split_once(',')?;
                let x = parse_len(xs)?;
                let y = parse_len(ys)?;
                let map = |len: Len, axis_x: bool| -> i64 {
                    match (group, len.unit) {
                        (Some(ctx), Unit::Bare) => {
                            let mapped = if axis_x {
                                ctx.map_x(len.value)
                            } else {
                                ctx.map_y(len.value)
                            };
                            mapped.map(round_twips).unwrap_or_else(|| len.to_twips())
                        }
                        _ => len.to_twips(),
                    }
                };
                Some((map(x, true), map(y, false)))
            }
            None => {
                let left = position.left?;
                let top = position.top?;
                if far {
                    Some((
                        left + position.width.unwrap_or(0),
                        top + position.height.unwrap_or(0),
                    ))
                } else {
                    Some((left, top))
                }
            }
        }
    }
}

/// Parses the horizontal-rule marker from a shape's attributes: present only when
/// `o:hr="t"`. `o:hralign` selects the alignment (default left); `o:hrpct` is the
/// width as a fraction of the content width in per-mille (`1000` = full width),
/// carried through when present.
fn parse_hr(element: &BytesStart<'_>) -> Option<VmlHr> {
    if !attr(element, b"hr")
        .as_deref()
        .map(parse_bool)
        .unwrap_or(false)
    {
        return None;
    }
    let align = match attr(element, b"hralign").as_deref().map(str::trim) {
        Some("center") => VmlHrAlign::Center,
        Some("right") => VmlHrAlign::Right,
        // `left` and any unrecognized value fall back to VML's default (left).
        _ => VmlHrAlign::Left,
    };
    let pct_permille = attr(element, b"hrpct")
        .as_deref()
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|pct| *pct > 0.0)
        .map(|pct| pct.round().clamp(1.0, u16::MAX as f64) as u16);
    Some(VmlHr {
        align,
        pct_permille,
    })
}

/// `arcsize` is a fraction of the smaller box dimension, either a decimal
/// (`0.1`) or a `65536`-based integer with an `f` suffix (`3277f`).
fn parse_arcsize(value: &str) -> Option<f64> {
    let value = value.trim();
    if let Some(num) = value.strip_suffix('f') {
        Some(num.parse::<f64>().ok()? / 65536.0)
    } else {
        value.parse::<f64>().ok()
    }
}

fn corner_radius(arcsize: Option<f64>, position: &VmlPosition) -> Option<i64> {
    let fraction = arcsize?;
    let w = position.width?;
    let h = position.height?;
    Some(round_twips(fraction * w.min(h) as f64))
}

fn parse_inset(value: &str) -> [Option<i64>; 4] {
    let mut out = [None; 4];
    for (slot, token) in out.iter_mut().zip(value.split(',')) {
        *slot = parse_len(token).map(Len::to_twips);
    }
    out
}

// --- entry point -----------------------------------------------------------

/// Parses a VML fragment (a `w:pict`'s content, or any run of sibling `v:*`
/// elements) into a flat list of [`VmlDrawing`]s, with every `v:group` flattened
/// to absolute-twip child positions. Best-effort: a malformed fragment returns
/// the shapes parsed so far.
pub fn parse_vml_pict(fragment: &str) -> Vec<VmlDrawing> {
    let bytes = fragment.as_bytes();
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_end_names = false;
    let mut buffer = Vec::new();
    let mut out = Vec::new();
    let mut groups: Vec<GroupCtx> = Vec::new();
    let mut shape: Option<ShapeBuilder> = None;
    // Depth of a `v:shapetype` subtree being skipped (its `v:stroke`/`v:path`
    // template children must not leak onto the next real shape).
    let mut shapetype_depth = 0_usize;

    // A read error ends the `while let` (best-effort: keep what we parsed).
    while let Ok(event) = reader.read_event_into(&mut buffer) {
        match event {
            Event::Eof => break,
            Event::Start(element) => {
                let local = element.local_name();
                let name = local.as_ref();
                if shapetype_depth > 0 {
                    shapetype_depth += 1;
                } else if name == b"shapetype" {
                    shapetype_depth = 1;
                } else {
                    on_open(name, &element, &mut groups, &mut shape, true);
                    if name == b"txbxContent" {
                        capture_txbx(&mut reader, &mut buffer, bytes, &mut shape);
                    }
                }
            }
            Event::Empty(element) => {
                let local = element.local_name();
                let name = local.as_ref();
                if shapetype_depth == 0 {
                    on_open(name, &element, &mut groups, &mut shape, false);
                    on_close(name, &mut groups, &mut shape, &mut out);
                }
            }
            Event::End(element) => {
                let local = element.local_name();
                let name = local.as_ref();
                if shapetype_depth > 0 {
                    shapetype_depth -= 1;
                } else {
                    on_close(name, &mut groups, &mut shape, &mut out);
                }
            }
            _ => {}
        }
        buffer.clear();
    }
    // Flush an unclosed final shape (defensive; well-formed input closes it).
    if let Some(builder) = shape.take() {
        out.push(builder.finalize(groups.last()));
    }
    out
}

fn shape_local(name: &[u8]) -> Option<ShapeLocal> {
    match name {
        b"rect" => Some(ShapeLocal::Rect),
        b"roundrect" => Some(ShapeLocal::RoundRect),
        b"line" => Some(ShapeLocal::Line),
        b"oval" => Some(ShapeLocal::Oval),
        b"shape" | b"polyline" | b"curve" => Some(ShapeLocal::Shape),
        _ => None,
    }
}

fn on_open(
    name: &[u8],
    element: &BytesStart<'_>,
    groups: &mut Vec<GroupCtx>,
    shape: &mut Option<ShapeBuilder>,
    is_start: bool,
) {
    if name == b"group" {
        let style = StyleProps::parse(&attr(element, b"style").unwrap_or_default());
        let ctx = build_group_ctx(element, &style, groups.last());
        // Only a Start group nests; an empty group has no children (rare).
        if is_start {
            groups.push(ctx);
        }
        return;
    }
    if let Some(local) = shape_local(name) {
        *shape = Some(ShapeBuilder::new(local, element));
        return;
    }
    if name == b"wrap" {
        if let Some(builder) = shape.as_mut() {
            builder.apply_wrap(element);
        } else if let Some(group) = groups.last_mut() {
            group.wrap.apply_element(element);
        }
        return;
    }
    let Some(builder) = shape.as_mut() else {
        return;
    };
    match name {
        b"fill" => builder.apply_fill(element),
        b"stroke" => builder.apply_stroke(element),
        b"imagedata" => {
            if builder.image_rid.is_none() {
                builder.image_rid = attr(element, b"id");
            }
        }
        b"textbox" => {
            let style = StyleProps::parse(&attr(element, b"style").unwrap_or_default());
            let vertical_anchor = style
                .get("v-text-anchor")
                .map(str::to_owned)
                .or_else(|| attr(element, b"v-text-anchor"))
                .as_deref()
                .and_then(VmlTextAnchor::parse);
            let inset = attr(element, b"inset")
                .map(|value| parse_inset(&value))
                .unwrap_or([None; 4]);
            builder.textbox = Some(VmlTextbox {
                inset_twips: inset,
                vertical_anchor,
                fit_shape_to_text: style.get("mso-fit-shape-to-text").is_some_and(parse_bool),
                content_xml: None,
            });
        }
        _ => {}
    }
}

fn on_close(
    name: &[u8],
    groups: &mut Vec<GroupCtx>,
    shape: &mut Option<ShapeBuilder>,
    out: &mut Vec<VmlDrawing>,
) {
    if name == b"group" {
        groups.pop();
        return;
    }
    if shape_local(name).is_some()
        && let Some(builder) = shape.take()
    {
        out.push(builder.finalize(groups.last()));
    }
}

/// Captures the raw inner XML of the currently-open `w:txbxContent` into the
/// live shape's text-box marker, consuming events up to its matching end tag.
fn capture_txbx(
    reader: &mut Reader<&[u8]>,
    buffer: &mut Vec<u8>,
    bytes: &[u8],
    shape: &mut Option<ShapeBuilder>,
) {
    let start = reader.buffer_position() as usize;
    let mut depth = 1_usize;
    let end = loop {
        let before = reader.buffer_position() as usize;
        buffer.clear();
        match reader.read_event_into(buffer) {
            Ok(Event::Start(e)) if e.local_name().as_ref() == b"txbxContent" => depth += 1,
            Ok(Event::End(e)) if e.local_name().as_ref() == b"txbxContent" => {
                depth -= 1;
                if depth == 0 {
                    break before;
                }
            }
            Ok(Event::Eof) | Err(_) => break before,
            _ => {}
        }
    };
    let content = bytes
        .get(start..end)
        .and_then(|slice| std::str::from_utf8(slice).ok())
        .map(str::to_string);
    if let Some(builder) = shape.as_mut() {
        match builder.textbox.as_mut() {
            Some(textbox) => textbox.content_xml = content,
            None => {
                builder.textbox = Some(VmlTextbox {
                    inset_twips: [None; 4],
                    vertical_anchor: None,
                    fit_shape_to_text: false,
                    content_xml: content,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_len_units_and_edges() {
        assert_eq!(parse_len("456.55pt").unwrap().to_twips(), 9131);
        assert_eq!(parse_len(".48001pt").unwrap().to_twips(), 10);
        assert_eq!(parse_len("1in").unwrap().to_twips(), 1440);
        assert_eq!(parse_len("2.54cm").unwrap().to_twips(), 1440);
        assert_eq!(parse_len("25.4mm").unwrap().to_twips(), 1440);
        assert_eq!(parse_len("96px").unwrap().to_twips(), 1440);
        assert_eq!(parse_len("1pc").unwrap().to_twips(), 240);
        assert_eq!(parse_len("635emu").unwrap().to_twips(), 1);
        assert_eq!(parse_len("-15.3pt").unwrap().to_twips(), -306);
        assert_eq!(parse_len("  12pt ").unwrap().to_twips(), 240);
        assert!(parse_len("").is_none());
        assert!(parse_len("abc").is_none());
        assert!(parse_len("10furlongs").is_none());
    }

    #[test]
    fn style_parser_handles_whitespace_and_empty_decls() {
        let style = StyleProps::parse(" position:absolute ; left: 1in ; ;width:2in; ");
        assert_eq!(style.get("position"), Some("absolute"));
        assert_eq!(style.len("left").unwrap().to_twips(), 1440);
        assert_eq!(style.len("width").unwrap().to_twips(), 2880);
        assert!(style.get("missing").is_none());
        // A declaration with no colon is ignored, not panicked on.
        let messy = StyleProps::parse("junk;top:5pt");
        assert_eq!(messy.len("top").unwrap().to_twips(), 100);
    }

    #[test]
    fn rect_horizontal_rule_from_sds() {
        // A real horizon-rule rect from SDS_ANTI-T..._ZH.docx (document.xml).
        let xml = r##"<w:pict><v:rect style="position:absolute;margin-left:69.503998pt;margin-top:15.339811pt;width:456.55pt;height:.48001pt;mso-position-horizontal-relative:page;mso-position-vertical-relative:paragraph;z-index:-15728640" id="docshape10" filled="true" fillcolor="#000000" stroked="false"><v:fill type="solid"/><w10:wrap type="topAndBottom"/></v:rect></w:pict>"##;
        let drawings = parse_vml_pict(xml);
        assert_eq!(drawings.len(), 1);
        let d = &drawings[0];
        assert_eq!(d.id.as_deref(), Some("docshape10"));
        assert_eq!(d.kind, VmlShapeKind::Rect);
        assert_eq!(d.position.left, Some(1390));
        assert_eq!(d.position.top, Some(307));
        assert_eq!(d.position.width, Some(9131));
        assert_eq!(d.position.height, Some(10));
        assert_eq!(d.position.z_index, Some(-15_728_640));
        assert!(d.position.behind_doc());
        assert_eq!(d.position.h_relative, Some(VmlRelFrame::Page));
        assert_eq!(d.position.v_relative, Some(VmlRelFrame::Paragraph));
        assert!(d.fill.on);
        assert_eq!(
            d.fill.color,
            Some(VmlColor {
                r: 0,
                g: 0,
                b: 0,
                a: 255
            })
        );
        assert!(!d.stroke.on);
        // A manually-drawn rule rect (no `o:hr`) is not an `o:hr` horizontal rule.
        assert!(d.hr.is_none());
    }

    #[test]
    fn rect_with_o_hr_parses_the_horizontal_rule_marker() {
        // Word's "Insert → Horizontal Line": an `o:hr` rect, centered, grey.
        let xml = r##"<v:rect style="width:0.0pt;height:1.5pt" o:hr="t" o:hrstd="t" o:hralign="center" fillcolor="#A0A0A0" stroked="f"/>"##;
        let d = &parse_vml_pict(xml)[0];
        let hr = d.hr.expect("o:hr=\"t\" is a horizontal-rule marker");
        assert_eq!(hr.align, VmlHrAlign::Center);
        assert!(hr.pct_permille.is_none(), "no o:hrpct → full width");
        assert_eq!(d.position.height, Some(30), "1.5pt == 30 twips thick");
    }

    #[test]
    fn rect_with_o_hr_percent_and_right_align() {
        let xml = r##"<v:rect style="height:2pt" o:hr="t" o:hralign="right" o:hrpct="750" fillcolor="#808080"/>"##;
        let hr = parse_vml_pict(xml)[0].hr.expect("o:hr marker");
        assert_eq!(hr.align, VmlHrAlign::Right);
        assert_eq!(hr.pct_permille, Some(750));
    }

    #[test]
    fn rect_filled_but_not_stroked() {
        // Grey fill, no stroke (footer bar rect shape, standalone here).
        let xml = r##"<v:rect style="position:absolute;margin-left:10pt;margin-top:10pt;width:50pt;height:5pt" id="bar" fillcolor="#e4e4e4" stroked="f"><v:fill type="solid"/></v:rect>"##;
        let d = &parse_vml_pict(xml)[0];
        assert!(d.fill.on);
        assert_eq!(
            d.fill.color,
            Some(VmlColor {
                r: 0xe4,
                g: 0xe4,
                b: 0xe4,
                a: 255
            })
        );
        assert!(!d.stroke.on);
        assert!(d.stroke.color.is_none());
    }

    #[test]
    fn line_horizontal_rule_endpoints() {
        let xml = r##"<v:line style="position:absolute;z-index:5" from="0pt,10pt" to="200pt,10pt" strokecolor="#FF0000" strokeweight="1.5pt"/>"##;
        let d = &parse_vml_pict(xml)[0];
        assert_eq!(
            d.kind,
            VmlShapeKind::Line {
                from: Some((0, 200)),
                to: Some((4000, 200)),
            }
        );
        assert!(!d.position.behind_doc());
        assert!(d.stroke.on);
        assert_eq!(
            d.stroke.color,
            Some(VmlColor {
                r: 0xff,
                g: 0,
                b: 0,
                a: 255
            })
        );
        assert_eq!(d.stroke.weight_twips, Some(30));
    }

    #[test]
    fn line_falls_back_to_box_diagonal() {
        let xml = r##"<v:line style="position:absolute;margin-left:10pt;margin-top:20pt;width:100pt;height:40pt"/>"##;
        let d = &parse_vml_pict(xml)[0];
        assert_eq!(
            d.kind,
            VmlShapeKind::Line {
                from: Some((200, 400)),
                to: Some((2200, 1200)),
            }
        );
    }

    #[test]
    fn shape_with_imagedata_carries_rid_and_box() {
        let xml = r##"<v:shape style="position:absolute;margin-left:10pt;margin-top:20pt;width:100pt;height:50pt" type="#_x0000_t75" id="img"><v:imagedata r:id="rId7" o:title=""/></v:shape>"##;
        let d = &parse_vml_pict(xml)[0];
        assert_eq!(d.image_rid.as_deref(), Some("rId7"));
        assert_eq!(d.position.left, Some(200));
        assert_eq!(d.position.top, Some(400));
        assert_eq!(d.position.width, Some(2000));
        assert_eq!(d.position.height, Some(1000));
        assert!(matches!(d.kind, VmlShapeKind::Shape { .. }));
    }

    #[test]
    fn positive_z_index_is_in_front() {
        let xml = r##"<v:rect style="position:absolute;margin-left:1pt;margin-top:1pt;width:1pt;height:1pt;z-index:7"/>"##;
        let d = &parse_vml_pict(xml)[0];
        assert_eq!(d.position.z_index, Some(7));
        assert!(!d.position.behind_doc());
    }

    #[test]
    fn group_flattens_children_to_absolute_twips() {
        // The real footer group from SDS footer1.xml: two stacked rects forming a
        // bar. coordsize == group box in twips, so the transform is identity plus
        // the coordorigin offset.
        let xml = r##"<w:pict><v:group style="position:absolute;margin-left:69.503998pt;margin-top:793.416016pt;width:456.55pt;height:13.25pt;mso-position-vertical-relative:page;z-index:-16119808" id="g" coordorigin="1390,15868" coordsize="9131,265"><v:rect style="position:absolute;left:1390;top:15882;width:9131;height:251" id="a" filled="true" fillcolor="#e4e4e4" stroked="false"><v:fill type="solid"/></v:rect><v:rect style="position:absolute;left:1390;top:15868;width:9131;height:15" id="b" filled="true" fillcolor="#000000" stroked="false"><v:fill type="solid"/></v:rect></v:group></w:pict>"##;
        let drawings = parse_vml_pict(xml);
        assert_eq!(drawings.len(), 2);
        let a = &drawings[0];
        assert_eq!(a.id.as_deref(), Some("a"));
        assert_eq!(a.position.left, Some(1390));
        assert_eq!(a.position.top, Some(15882));
        assert_eq!(a.position.width, Some(9131));
        assert_eq!(a.position.height, Some(251));
        // Children inherit the group's z-index / vertical frame.
        assert_eq!(a.position.z_index, Some(-16_119_808));
        assert!(a.position.behind_doc());
        assert_eq!(a.position.v_relative, Some(VmlRelFrame::Page));
        let b = &drawings[1];
        assert_eq!(b.position.top, Some(15868));
        assert_eq!(
            b.fill.color,
            Some(VmlColor {
                r: 0,
                g: 0,
                b: 0,
                a: 255
            })
        );
    }

    #[test]
    fn textbox_records_raw_content_without_flowing_it() {
        let xml = r##"<v:shape style="position:absolute;margin-left:10pt;margin-top:10pt;width:100pt;height:30pt" type="#_x0000_t202" id="tb" filled="false" stroked="false"><v:textbox inset="0,0,0,0"><w:txbxContent><w:p><w:r><w:t>Header text</w:t></w:r></w:p></w:txbxContent></v:textbox><w10:wrap type="none"/></v:shape>"##;
        let d = &parse_vml_pict(xml)[0];
        let tb = d.textbox.as_ref().expect("textbox marker present");
        assert_eq!(tb.inset_twips, [Some(0), Some(0), Some(0), Some(0)]);
        let content = tb.content_xml.as_deref().expect("raw content captured");
        assert!(content.contains("<w:t>Header text</w:t>"));
        // The content is NOT flowed into shapes here.
        assert!(!d.fill.on);
        assert!(!d.stroke.on);
    }

    #[test]
    fn textbox_preserves_alignment_wrap_distances_insets_anchor_and_autofit() {
        let xml = r##"<v:shape style="position:absolute;width:100pt;height:30pt;mso-position-horizontal:center;mso-position-horizontal-relative:left-margin;mso-position-vertical:bottom;mso-position-vertical-relative:bottom-margin;mso-wrap-distance-top:1pt;mso-wrap-distance-bottom:2pt;mso-wrap-distance-left:3pt;mso-wrap-distance-right:4pt" type="#_x0000_t202"><v:textbox inset="1pt,2pt,3pt,4pt" style="v-text-anchor:middle;mso-fit-shape-to-text:t"><w:txbxContent><w:p/></w:txbxContent></v:textbox><w10:wrap type="topAndBottom"/></v:shape>"##;
        let drawing = &parse_vml_pict(xml)[0];
        assert_eq!(drawing.position.h_relative, Some(VmlRelFrame::LeftMargin));
        assert_eq!(drawing.position.v_relative, Some(VmlRelFrame::BottomMargin));
        assert_eq!(drawing.position.h_align, Some(VmlHorizontalAlign::Center));
        assert_eq!(drawing.position.v_align, Some(VmlVerticalAlign::Bottom));
        assert_eq!(drawing.wrap.mode, Some(VmlWrapMode::TopAndBottom));
        assert_eq!(
            drawing.wrap.distances_twips,
            [Some(20), Some(40), Some(60), Some(80)]
        );
        let textbox = drawing.textbox.as_ref().expect("textbox is parsed");
        assert_eq!(
            textbox.inset_twips,
            [Some(20), Some(40), Some(60), Some(80)]
        );
        assert_eq!(textbox.vertical_anchor, Some(VmlTextAnchor::Middle));
        assert!(textbox.fit_shape_to_text);
    }

    #[test]
    fn group_wrap_metadata_is_inherited_by_child_shapes() {
        let xml = r##"<v:group style="position:absolute;margin-left:1pt;margin-top:2pt;width:10pt;height:10pt;mso-wrap-mode:square;mso-wrap-distance-left:3pt" coordsize="200,200"><w10:wrap type="topAndBottom"/><v:rect style="left:0;top:0;width:200;height:200"/></v:group>"##;
        let drawing = &parse_vml_pict(xml)[0];
        assert_eq!(drawing.wrap.mode, Some(VmlWrapMode::TopAndBottom));
        assert_eq!(drawing.wrap.distances_twips[2], Some(60));
    }

    #[test]
    fn shapetype_template_does_not_leak_into_following_shape() {
        // A `v:shapetype` defines a stroke; the following text-box shape sets
        // stroked="false". The template's stroke must not resurrect the stroke.
        let xml = r##"<w:pict><v:shapetype id="_x0000_t202" coordsize="21600,21600" path="m,l,21600r21600,l21600,xe"><v:stroke joinstyle="miter"/><v:path gradientshapeok="t"/></v:shapetype><v:shape style="position:absolute;margin-left:5pt;margin-top:5pt;width:50pt;height:20pt" type="#_x0000_t202" id="s" filled="false" stroked="false"><v:textbox inset="0,0,0,0"><w:txbxContent><w:p/></w:txbxContent></v:textbox></v:shape></w:pict>"##;
        let drawings = parse_vml_pict(xml);
        assert_eq!(drawings.len(), 1);
        assert!(!drawings[0].stroke.on);
        assert_eq!(drawings[0].id.as_deref(), Some("s"));
    }

    #[test]
    fn roundrect_corner_radius_from_arcsize() {
        let xml = r##"<v:roundrect style="position:absolute;margin-left:0pt;margin-top:0pt;width:100pt;height:50pt" arcsize="0.25" fillcolor="red"/>"##;
        let d = &parse_vml_pict(xml)[0];
        // radius = 0.25 * min(2000, 1000) twips = 250.
        assert_eq!(
            d.kind,
            VmlShapeKind::RoundRect {
                corner_radius_twips: Some(250)
            }
        );
        assert_eq!(
            d.fill.color,
            Some(VmlColor {
                r: 0xff,
                g: 0,
                b: 0,
                a: 255
            })
        );
    }

    #[test]
    fn fill_opacity_sets_alpha() {
        let xml = r##"<v:rect style="position:absolute;margin-left:0pt;margin-top:0pt;width:1pt;height:1pt" fillcolor="#000000"><v:fill opacity="0.5"/></v:rect>"##;
        let d = &parse_vml_pict(xml)[0];
        assert_eq!(d.fill.color.map(|c| c.a), Some(128));
    }

    #[test]
    fn diagonal_path_shape_from_sds_keeps_path_and_box() {
        // The path-outline shape inside the SDS callout group (document.xml).
        let xml = r##"<v:group style="position:absolute;margin-left:88.463997pt;margin-top:19.430771pt;width:439.9pt;height:75.4pt;z-index:-15728128" coordorigin="1769,389" coordsize="8798,1508"><v:shape style="position:absolute;left:1769;top:388;width:8798;height:1508" id="docshape12" path="m1779,993l1769,993xe" filled="true" fillcolor="#000000" stroked="false"><v:path arrowok="t"/><v:fill type="solid"/></v:shape></v:group>"##;
        let d = &parse_vml_pict(xml)[0];
        match &d.kind {
            VmlShapeKind::Shape { path, .. } => {
                assert!(path.as_deref().unwrap().starts_with("m1779,993"));
            }
            other => panic!("expected Shape, got {other:?}"),
        }
        // width 8798 local * (439.9pt→8798twips / 8798) = 8798 twips.
        assert_eq!(d.position.width, Some(8798));
        assert!(d.position.left.is_some());
    }

    #[test]
    fn malformed_fragment_returns_parsed_prefix() {
        let xml = r##"<v:rect style="position:absolute;margin-left:1pt;margin-top:1pt;width:1pt;height:1pt"/><v:oval "##;
        // The truncated oval is dropped; the well-formed rect survives.
        let drawings = parse_vml_pict(xml);
        assert_eq!(drawings.len(), 1);
        assert_eq!(drawings[0].kind, VmlShapeKind::Rect);
    }

    #[test]
    fn fill_angle_converts_from_vml_to_drawingml_convention() {
        // VML `0` = horizontal left→right; the DrawingML equivalent is 180°.
        assert_eq!(vml_fill_angle_to_model(0.0), 180 * 60_000);
        // VML `90` → 270° (a vertical sweep in the model's convention).
        assert_eq!(vml_fill_angle_to_model(90.0), 270 * 60_000);
        // VML `45` → 225° (the reference example: color2 top-left, color bottom-right).
        assert_eq!(vml_fill_angle_to_model(45.0), 225 * 60_000);
        // Angles normalize into [0°, 360°): VML `270` → 450° → 90°.
        assert_eq!(vml_fill_angle_to_model(270.0), 90 * 60_000);
        assert_eq!(vml_fill_angle_to_model(-180.0), 0);
    }

    #[test]
    fn two_color_linear_gradient_parses_stops_kind_and_angle() {
        // `fillcolor` is the first stop; `v:fill@color2` the second; the flat
        // `color` fallback stays the primary color.
        let xml = r##"<v:rect style="position:absolute;margin-left:0pt;margin-top:0pt;width:100pt;height:20pt" fillcolor="#ff0000"><v:fill type="gradient" color2="#0000ff" angle="0"/></v:rect>"##;
        let d = &parse_vml_pict(xml)[0];
        let g = d.fill.gradient.as_ref().expect("gradient parsed");
        assert_eq!(
            g.kind,
            VmlGradientKind::Linear {
                angle: 180 * 60_000
            }
        );
        assert_eq!(
            g.stops,
            vec![
                VmlGradientStop {
                    position: 0,
                    color: VmlColor {
                        r: 0xff,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                },
                VmlGradientStop {
                    position: 100_000,
                    color: VmlColor {
                        r: 0,
                        g: 0,
                        b: 0xff,
                        a: 255
                    },
                },
            ]
        );
        // The flat fallback color is preserved (first stop) for any flat-only path.
        assert_eq!(d.fill.color.map(|c| (c.r, c.g, c.b)), Some((0xff, 0, 0)));
    }

    #[test]
    fn radial_gradient_maps_to_radial_kind() {
        let xml = r##"<v:oval style="position:absolute;margin-left:0pt;margin-top:0pt;width:60pt;height:60pt" fillcolor="red"><v:fill type="gradientRadial" color2="blue"/></v:oval>"##;
        let g = parse_vml_pict(xml)[0]
            .fill
            .gradient
            .clone()
            .expect("radial gradient parsed");
        assert_eq!(g.kind, VmlGradientKind::Radial);
        assert_eq!(g.stops.len(), 2);
        assert_eq!(g.stops[0].position, 0);
        assert_eq!(g.stops[1].position, 100_000);
    }

    #[test]
    fn multi_stop_colors_list_becomes_intermediate_stops() {
        // A `colors="…"` list supplies three ordered stops on its own.
        let xml = r##"<v:rect style="position:absolute;margin-left:0pt;margin-top:0pt;width:10pt;height:10pt" filled="true"><v:fill type="gradient" colors="0 #ff0000;.5 lime;100% #0000ff"/></v:rect>"##;
        let g = parse_vml_pict(xml)[0]
            .fill
            .gradient
            .clone()
            .expect("multi-stop gradient parsed");
        assert_eq!(
            g.stops,
            vec![
                VmlGradientStop {
                    position: 0,
                    color: VmlColor {
                        r: 0xff,
                        g: 0,
                        b: 0,
                        a: 255
                    },
                },
                VmlGradientStop {
                    position: 50_000,
                    color: VmlColor {
                        r: 0,
                        g: 0xff,
                        b: 0,
                        a: 255
                    },
                },
                VmlGradientStop {
                    position: 100_000,
                    color: VmlColor {
                        r: 0,
                        g: 0,
                        b: 0xff,
                        a: 255
                    },
                },
            ]
        );
    }

    #[test]
    fn explicit_endpoint_color_wins_over_duplicate_colors_entry() {
        // `fillcolor`/`color2` define the endpoints; a `colors` entry duplicating
        // position 0 or 1 must not override them.
        let xml = r##"<v:rect style="position:absolute;margin-left:0pt;margin-top:0pt;width:10pt;height:10pt" fillcolor="#ff0000"><v:fill type="gradient" color2="#0000ff" colors="0 #00ff00;1 #00ff00;.5 #ffffff"/></v:rect>"##;
        let g = parse_vml_pict(xml)[0].fill.gradient.clone().unwrap();
        assert_eq!(g.stops.len(), 3);
        // Endpoints are the explicit fillcolor (red) and color2 (blue), not the
        // green `colors` entries at the same positions.
        assert_eq!(g.stops[0].position, 0);
        assert_eq!((g.stops[0].color.r, g.stops[0].color.b), (0xff, 0));
        assert_eq!(g.stops[2].position, 100_000);
        assert_eq!((g.stops[2].color.r, g.stops[2].color.b), (0, 0xff));
        // The mid-stop from the list survives.
        assert_eq!(g.stops[1].position, 50_000);
    }

    #[test]
    fn gradient_opacity_sets_stop_alpha() {
        let xml = r##"<v:rect style="position:absolute;margin-left:0pt;margin-top:0pt;width:10pt;height:10pt" fillcolor="#000000"><v:fill type="gradient" color2="#ffffff" opacity="0.5" opacity2="25%"/></v:rect>"##;
        let g = parse_vml_pict(xml)[0].fill.gradient.clone().unwrap();
        assert_eq!(g.stops[0].color.a, 128);
        assert_eq!(g.stops[1].color.a, 64);
    }

    #[test]
    fn solid_fill_has_no_gradient() {
        let xml = r##"<v:rect style="position:absolute;margin-left:0pt;margin-top:0pt;width:10pt;height:10pt" fillcolor="#123456"><v:fill type="solid"/></v:rect>"##;
        let d = &parse_vml_pict(xml)[0];
        assert!(d.fill.gradient.is_none());
        assert_eq!(
            d.fill.color.map(|c| (c.r, c.g, c.b)),
            Some((0x12, 0x34, 0x56))
        );
    }

    #[test]
    fn gradient_with_a_single_color_falls_back_to_flat() {
        // A gradient `type` with only one color cannot form a gradient; the flat
        // color fallback is kept instead.
        let xml = r##"<v:rect style="position:absolute;margin-left:0pt;margin-top:0pt;width:10pt;height:10pt" fillcolor="#abcdef"><v:fill type="gradient"/></v:rect>"##;
        let d = &parse_vml_pict(xml)[0];
        assert!(d.fill.gradient.is_none());
        assert_eq!(
            d.fill.color.map(|c| (c.r, c.g, c.b)),
            Some((0xab, 0xcd, 0xef))
        );
    }
}
