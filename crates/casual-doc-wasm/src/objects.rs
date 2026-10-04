//! Floating-object POSITION, GROUPING, z-order and transform — the host side.
//!
//! This module is the engine edge a host draws Word's Layout dialog and its
//! Arrange group from. Everything a position dialog can express is expressed
//! here **in the OOXML model** (`wp:positionH`/`wp:positionV`
//! `@relativeFrom` + `wp:posOffset`/`wp:align`), not in an invented one, so a
//! document authored here and a document authored in Word describe a position
//! the same way and a round trip changes nothing.
//!
//! # Why this exists
//!
//! `setObjectAnchorPosition` — the only positioning entry point before this —
//! hardcodes `relativeFrom="page"` with an absolute `posOffset` on both axes.
//! That is right for a free pointer drag and wrong for everything else: a host
//! could not say "centred on the margin", "0.5 inch below the paragraph", or
//! "in line with text", and a document that arrived saying one of those was
//! rewritten to a page offset the moment anybody nudged it. Reading the
//! position back was not possible at all, so a position dialog could not even
//! show what the object currently is.
//!
//! # Complexity
//!
//! Every command here is **O(document), ONE walk** — it resolves the owning
//! paragraph, clones that paragraph's inlines, rewrites them, and commits. None
//! of it is on the keystroke path (`docs/107` §4 keeps per-keystroke work O(1)
//! in document size), and none of it calls a lookup-by-id inside a loop over
//! ids: where several objects are involved (grouping) the walk collects them
//! all at once and the loop runs over what the walk already carried.
//!
//! # The closed operation set
//!
//! Nothing here adds an `Operation` variant (ADR-030 I2 keeps the op set closed
//! at 53 as an OT seam). Anchors commit as `SetAnchor`; everything structural —
//! grouping, ungrouping, z-order, rotation, flip, inline↔floating conversion —
//! commits as `SetInlines` on the owning paragraph, which is how
//! `moveGroupChildBy` already expresses an intra-group move.

use casual_doc_edit::{Operation, Pos, find_paragraph_any};
// Its own line, not folded into a sorted block: a shared `use` list is where
// parallel lanes collide (rustfmt is set to Preserve).
use casual_doc_edit::refused;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AnchorHorizontal, AnchorVertical, AnchoredDrawing, BlockNode, DrawingAnchor, Extent,
    GroupChild, GroupPicture, GroupTextBox, GroupTransform, HorizontalAlign, HorizontalAnchor,
    HorizontalPosition, InlineNode, MAX_EMU, Paragraph, ParagraphProperties, PointEmu,
    ShapeGeometry, TextBox, TextBoxBodyProperties, VerticalAlign, VerticalAnchor, VerticalPosition,
    WordprocessingGroup, WrapDistances, WrapMode,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::{EMU_PER_TWIP_I64, EditResult, HistoryKind, WasmDocument, to_js};

// ---------------------------------------------------------------------------
// The object-geometry refusal family
//
// `webapp/src/edit_errors.mjs` passes a refusal through verbatim when it is
// MARKED and replaces every other one with a single generic sentence — "That
// edit isn't supported for this selection yet". Every refusal in this family was
// a bare `String`, so two dozen different causes — an object in the line of
// text, a group whose members have no size of their own, a position dialog asked
// to restack — all reached the reader as that one unactionable sentence, about a
// selection that is usually fine. This is the family the owner hit while
// dragging shapes.
//
// Each entry carries a sentence a reader can act on and a stable dotted code a
// host routes through its own catalogue, so a non-English reader gets the
// specific reason too (`casual_doc_edit::refusal`).
//
// They are `const`s rather than inline `refused!` calls because most have
// several call sites and some are shared with `lib.rs`: one CAUSE must reach the
// reader as one sentence, and a code is a published routing key, so the same
// code must never carry two sentences.
// `no_object_geometry_refusal_reaches_a_reader_without_a_code` fails the build
// if a bare literal joins the family again.
// ---------------------------------------------------------------------------

/// The node a geometry command names is not a selectable object at all.
pub(crate) const NOT_AN_OBJECT: &str = refused!(
    "object.not-selectable",
    "This isn't a picture, shape or text box, so there is nothing to change here."
);

/// Named for ungrouping, but it is not a group.
const NOT_A_GROUP: &str = refused!(
    "object.not-a-group",
    "This isn't a group, so there is nothing to ungroup."
);

/// A group that sits in the line of text, where the anchor every group command
/// edits does not exist.
pub(crate) const GROUP_NOT_FLOATING: &str = refused!(
    "object.group-not-floating",
    "This group is in the line of text rather than floating, so it has no anchor to \
     change. Give it a text wrap first."
);

/// A group with no members — nothing to ungroup, and no content bounds.
const GROUP_EMPTY: &str = refused!(
    "object.group-empty",
    "This group has no members, so there is nothing to ungroup."
);

/// An object inside a hyperlink, a content control or another group: grouping
/// or ungrouping it would have to rewrite the thing around it, which is a
/// different edit.
const GROUP_IN_WRAPPER: &str = refused!(
    "object.group-in-wrapper",
    "An object inside a link, a content control or another group cannot be grouped or \
     ungrouped on its own. Take it out of the one around it first."
);

/// A member of a group has no measurable child geometry, so no position for it
/// can be computed.
pub(crate) const GROUP_UNBOUNDED: &str = refused!(
    "object.group-unbounded",
    "A group here has no member with a size of its own, so its position cannot be \
     worked out."
);

/// One of the objects named is not laid out on a page, so grouping would have to
/// move it without being asked.
const GROUP_UNPLACED: &str = refused!(
    "object.group-unplaced",
    "One of these objects is not laid out on the page yet, so it cannot be grouped."
);

/// Part of a group is not laid out, so ungrouping cannot keep its members where
/// they are drawn.
const GROUP_PARTLY_UNPLACED: &str = refused!(
    "object.group-partly-unplaced",
    "Part of this group is not laid out on a page, so ungrouping it would move its \
     members."
);

/// Grouping needs the members to share an anchor paragraph; a group has one
/// anchor and cannot keep two.
const GROUP_NOT_SIBLING: &str = refused!(
    "object.group-not-sibling",
    "Objects can only be grouped when they are anchored to the same paragraph."
);

/// Grouping an object that sits in the line of text.
///
/// The sentence says "in line with text" because that is Word's own name for
/// the state, and `grouping_refuses_with_a_reason_a_host_can_show` asserts the
/// reason names it.
const GROUP_INLINE: &str = refused!(
    "object.group-inline",
    "Only floating objects can be grouped. An object that sits in line with text has to \
     be given a text wrap first."
);

/// A group has one anchor, so members laid out on different pages cannot all
/// keep their position.
const GROUP_ACROSS_PAGES: &str = refused!(
    "object.group-across-pages",
    "Objects on different pages cannot be grouped."
);

/// Grouping needs two objects.
const GROUP_TOO_FEW: &str = refused!(
    "object.group-too-few",
    "Select at least two objects to group."
);

/// One of the ids handed to a grouping command no longer resolves.
const GROUP_MEMBER_MISSING: &str = refused!(
    "object.group-member-missing",
    "One of these objects is no longer in the document, so they cannot be grouped."
);

/// The subject is not a shape inside a group — the gesture that moves a group
/// member, or adds text to a shape.
pub(crate) const NOT_A_GROUP_CHILD: &str = refused!(
    "object.not-group-child",
    "This isn't a shape inside a group, so that change doesn't apply to it."
);

/// A lone shape whose group has no anchor: there is no position to drag.
pub(crate) const LONE_SHAPE_INLINE: &str = refused!(
    "object.lone-shape-inline",
    "This shape sits in the line of text, so it has no position to drag. Give it a text \
     wrap first."
);

/// A freeform shape asked to hold text: its custom geometry has no text body
/// this build can flow without flattening the outline to a rectangle.
const FREEFORM_NO_TEXT: &str = refused!(
    "object.freeform-no-text",
    "A freeform shape cannot hold text yet — its custom outline would be flattened to a \
     rectangle. Put a text box over it instead."
);

/// Restacking something that is not in the floating z-order.
const NOT_RESTACKABLE: &str = refused!(
    "object.not-restackable",
    "Only a floating object can be brought forward or sent back; an object in the line \
     of text stacks with the text around it."
);

/// The z-order rank space is exhausted.
const TOO_MANY_FLOATS: &str = refused!(
    "object.too-many-floats",
    "There are too many floating objects here to restack another."
);

/// A rotation that is not a number.
const ROTATION_NOT_FINITE: &str = refused!(
    "object.rotation-not-finite",
    "A rotation has to be a number of degrees."
);

/// A position axis given both an alignment and an offset.
const POSITION_H_AMBIGUOUS: &str = refused!(
    "object.position-h-ambiguous",
    "A horizontal position is either an alignment or an offset, not both."
);

/// A position axis given both an alignment and an offset.
const POSITION_V_AMBIGUOUS: &str = refused!(
    "object.position-v-ambiguous",
    "A vertical position is either an alignment or an offset, not both."
);

/// The position surface asked to put an object back in the line of text, which
/// is a different command.
const POSITION_NOT_INLINE: &str = refused!(
    "object.position-not-inline",
    "Use Wrap Text, In Line with Text, to put this object back into the text; the \
     position controls only move a floating object."
);

/// Positioning an object that has no anchor of its own.
const POSITION_NOT_FLOATING: &str = refused!(
    "object.position-not-floating",
    "Only a floating object has a position of its own. Give this one a text wrap first."
);

/// The position surface asked to restack, which is a different command.
const POSITION_NOT_RESTACK: &str = refused!(
    "object.position-not-restack",
    "Use Bring Forward or Send Backward to change which object is in front; the \
     position controls do not restack."
);

/// Floating a picture whose size the document never recorded: the float would
/// have to invent one.
const PICTURE_NO_SIZE: &str = refused!(
    "object.picture-no-size",
    "This picture has no size recorded in the document, so it cannot be made floating \
     without changing how it looks."
);

/// An object that is not laid out on a page, so it has no geometry to replace.
pub(crate) const NOT_PLACED: &str = refused!(
    "object.not-placed",
    "This object is not laid out on a page yet, so its position and size cannot be set."
);

/// A group whose members cannot be scaled exactly, so the whole group cannot be.
pub(crate) const GROUP_RESIZE_INEXACT: &str = refused!(
    "object.group-resize-inexact",
    "This group cannot be resized exactly — one of its members has no size of its own, \
     or sizes itself to its text. Resize its members individually."
);

/// A group with no measurable rectangle on the page.
pub(crate) const GROUP_DEGENERATE: &str = refused!(
    "object.group-degenerate",
    "This group has no measurable size on the page, so it cannot be scaled."
);

/// A floating object whose shape has no exact size inverse.
pub(crate) const FLOAT_RESIZE_INEXACT: &str = refused!(
    "object.float-resize-inexact",
    "This object's size cannot be set exactly, so the document was left as it was."
);

/// An in-line object whose shape has no exact size inverse.
pub(crate) const INLINE_RESIZE_INEXACT: &str = refused!(
    "object.inline-resize-inexact",
    "This in-line object's size cannot be set exactly, so the document was left as it \
     was."
);

/// An in-line object dragged by a size handle that also moved it.
pub(crate) const INLINE_RESIZE_MOVED: &str = refused!(
    "object.inline-resize-moved",
    "An object in the line of text can be resized but not moved — the text decides \
     where it sits. Give it a text wrap to move it freely."
);

/// Moving something that has no anchor to move.
pub(crate) const NOT_FLOATING_MOVE: &str = refused!(
    "object.not-floating-move",
    "Only a floating object can be moved freely. Give this one a text wrap first."
);

/// Changing the wrap of something that has no wrap.
pub(crate) const NOT_FLOATING_WRAP: &str = refused!(
    "object.not-floating-wrap",
    "Only a floating object has text wrapping to change."
);

// ---------------------------------------------------------------------------
// The capability reasons
//
// A `false` in `ObjectCapabilities` used to be able to say nothing at all: the
// payload carries ten booleans, the host gates on bare truthiness, and the
// reader therefore met a control that did nothing and no explanation — `SKILL`
// §10's dead control, once per unavailable capability per carrier. These are the
// sentences `capability_refusals` publishes beside the bits.
//
// They are declared HERE, beside the command refusals above, because several of
// them ARE the command refusal: an inline object's unavailable move publishes
// `NOT_FLOATING_MOVE`, which is the same string `setObjectAnchorPosition` throws
// if the host tries anyway. One sentence for one fact, in one place, is what
// stops the chrome and the engine disagreeing about why.

/// An object whose model carries no rotation or flip. Also the `canRotate`
/// reason: a carrier whose rotation no pass paints must not be offered a handle,
/// and the reader is owed the reason rather than a missing grip.
pub(crate) const NO_ROTATION: &str = refused!(
    "object.no-rotation",
    "This object has no rotation or flip to change."
);

/// Alternative text on a carrier with no field for it.
pub(crate) const NO_ALT_TEXT: &str = refused!(
    "object.no-alt-text",
    "Alternative text is stored on a picture here, and this object is not one — its \
     description is not modelled yet."
);

/// Cropping something with no source image to trim.
pub(crate) const NOT_CROPPABLE: &str = refused!(
    "object.not-croppable",
    "Cropping trims a picture's source image, and this object has none."
);

/// A fill on something that is not a shape.
pub(crate) const NOT_FILLABLE: &str = refused!(
    "object.not-fillable",
    "A fill colour belongs to a shape, and this object is not one."
);

/// An outline on something that is not a shape.
pub(crate) const NOT_STROKEABLE: &str = refused!(
    "object.not-strokeable",
    "An outline belongs to a shape, and this object is not one."
);

/// Typing into something with no text story of its own.
pub(crate) const NO_TEXT_STORY: &str = refused!(
    "object.no-text-story",
    "This object holds no text of its own to edit."
);

/// Removing something whose removal this build cannot invert exactly.
pub(crate) const NOT_DELETABLE: &str = refused!(
    "object.not-deletable",
    "This object cannot be removed on its own in this document."
);

/// The last resort, and a value that must never actually be published: it exists
/// so an eleventh capability added without a reason degrades to a sentence
/// instead of panicking inside a getter, and
/// `every_unavailable_capability_says_why` fails the build when it appears.
pub(crate) const CAPABILITY_UNAVAILABLE: &str = refused!(
    "object.capability-unavailable",
    "That change doesn't apply to this object."
);

// ---------------------------------------------------------------------------
// OOXML tokens
//
// One table per enum, both directions, so a token this build writes is a token
// this build reads. A host never sees an invented name: these ARE the
// `ST_RelFromH` / `ST_RelFromV` / `ST_AlignH` / `ST_AlignV` / `wp:wrap*` names.
// ---------------------------------------------------------------------------

/// The `ST_RelFromH` token for a horizontal reference.
const fn horizontal_anchor_token(anchor: HorizontalAnchor) -> &'static str {
    match anchor {
        HorizontalAnchor::Page => "page",
        HorizontalAnchor::Margin => "margin",
        HorizontalAnchor::Column => "column",
        HorizontalAnchor::Character => "character",
        HorizontalAnchor::LeftMargin => "leftMargin",
        HorizontalAnchor::RightMargin => "rightMargin",
        HorizontalAnchor::InsideMargin => "insideMargin",
        HorizontalAnchor::OutsideMargin => "outsideMargin",
    }
}

/// The inverse of [`horizontal_anchor_token`].
fn parse_horizontal_anchor(token: &str) -> Option<HorizontalAnchor> {
    Some(match token {
        "page" => HorizontalAnchor::Page,
        "margin" => HorizontalAnchor::Margin,
        "column" => HorizontalAnchor::Column,
        "character" => HorizontalAnchor::Character,
        "leftMargin" => HorizontalAnchor::LeftMargin,
        "rightMargin" => HorizontalAnchor::RightMargin,
        "insideMargin" => HorizontalAnchor::InsideMargin,
        "outsideMargin" => HorizontalAnchor::OutsideMargin,
        _ => return None,
    })
}

/// The `ST_RelFromV` token for a vertical reference.
const fn vertical_anchor_token(anchor: VerticalAnchor) -> &'static str {
    match anchor {
        VerticalAnchor::Page => "page",
        VerticalAnchor::Margin => "margin",
        VerticalAnchor::Paragraph => "paragraph",
        VerticalAnchor::Line => "line",
        VerticalAnchor::TopMargin => "topMargin",
        VerticalAnchor::BottomMargin => "bottomMargin",
        VerticalAnchor::InsideMargin => "insideMargin",
        VerticalAnchor::OutsideMargin => "outsideMargin",
    }
}

/// The inverse of [`vertical_anchor_token`].
fn parse_vertical_anchor(token: &str) -> Option<VerticalAnchor> {
    Some(match token {
        "page" => VerticalAnchor::Page,
        "margin" => VerticalAnchor::Margin,
        "paragraph" => VerticalAnchor::Paragraph,
        "line" => VerticalAnchor::Line,
        "topMargin" => VerticalAnchor::TopMargin,
        "bottomMargin" => VerticalAnchor::BottomMargin,
        "insideMargin" => VerticalAnchor::InsideMargin,
        "outsideMargin" => VerticalAnchor::OutsideMargin,
        _ => return None,
    })
}

/// The `ST_AlignH` token for a horizontal alignment.
const fn horizontal_align_token(align: HorizontalAlign) -> &'static str {
    match align {
        HorizontalAlign::Left => "left",
        HorizontalAlign::Center => "center",
        HorizontalAlign::Right => "right",
        HorizontalAlign::Inside => "inside",
        HorizontalAlign::Outside => "outside",
    }
}

/// The inverse of [`horizontal_align_token`].
fn parse_horizontal_align(token: &str) -> Option<HorizontalAlign> {
    Some(match token {
        "left" => HorizontalAlign::Left,
        "center" => HorizontalAlign::Center,
        "right" => HorizontalAlign::Right,
        "inside" => HorizontalAlign::Inside,
        "outside" => HorizontalAlign::Outside,
        _ => return None,
    })
}

/// The `ST_AlignV` token for a vertical alignment.
const fn vertical_align_token(align: VerticalAlign) -> &'static str {
    match align {
        VerticalAlign::Top => "top",
        VerticalAlign::Center => "center",
        VerticalAlign::Bottom => "bottom",
        VerticalAlign::Inside => "inside",
        VerticalAlign::Outside => "outside",
    }
}

/// The inverse of [`vertical_align_token`].
fn parse_vertical_align(token: &str) -> Option<VerticalAlign> {
    Some(match token {
        "top" => VerticalAlign::Top,
        "center" => VerticalAlign::Center,
        "bottom" => VerticalAlign::Bottom,
        "inside" => VerticalAlign::Inside,
        "outside" => VerticalAlign::Outside,
        _ => return None,
    })
}

/// The `wp:wrap*` token for a wrap mode.
const fn wrap_token(wrap: WrapMode) -> &'static str {
    match wrap {
        WrapMode::Square => "square",
        WrapMode::Tight => "tight",
        WrapMode::Through => "through",
        WrapMode::TopAndBottom => "topAndBottom",
        WrapMode::None => "none",
    }
}

/// The inverse of [`wrap_token`].
fn parse_wrap(token: &str) -> Option<WrapMode> {
    Some(match token {
        "square" => WrapMode::Square,
        "tight" => WrapMode::Tight,
        "through" => WrapMode::Through,
        "topAndBottom" => WrapMode::TopAndBottom,
        "none" => WrapMode::None,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// The position payload
// ---------------------------------------------------------------------------

/// One axis of a read position: the reference plus EITHER an alignment or an
/// offset, never both — which is exactly what `wp:positionH`/`wp:positionV`
/// carries.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AxisRead {
    relative_from: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    align: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    offset_emu: Option<f64>,
}

/// The exclusion distances, named as the schema names them.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DistancesJson {
    top_emu: f64,
    bottom_emu: f64,
    start_emu: f64,
    end_emu: f64,
}

/// What [`WasmDocument::object_position`] answers.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PositionRead {
    /// `false` for an object in the run flow ("In line with text").
    floating: bool,
    /// `true` when the object is a CHILD of a group, whose position its parent
    /// decides. A host disables the position dialog on this rather than on an
    /// empty answer, which it could not tell apart from "not an object".
    #[serde(skip_serializing_if = "core::ops::Not::not")]
    group_child: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    horizontal: Option<AxisRead>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vertical: Option<AxisRead>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wrap: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    behind_doc: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wrap_distances: Option<DistancesJson>,
    /// The `wp:anchor@relativeHeight` stacking key, when the producer set one.
    #[serde(skip_serializing_if = "Option::is_none")]
    z_order: Option<u32>,
}

/// One axis of a requested position. Both members optional: a host that is
/// changing only the reference keeps the placement, and vice versa.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AxisWrite {
    relative_from: Option<String>,
    align: Option<String>,
    offset_emu: Option<f64>,
}

/// What [`WasmDocument::set_object_position`] accepts.
///
/// Every field is optional and an omitted field is LEFT ALONE, so a dialog
/// that only changes the vertical reference does not silently restate — and
/// thereby overwrite — everything else.
///
/// `floating` and `zOrder` are accepted but are read-only: a value that
/// disagrees with the object is an error naming the entry point that does
/// change it. That makes the read payload a valid write payload without a
/// round trip quietly discarding half of it.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PositionWrite {
    floating: Option<bool>,
    horizontal: Option<AxisWrite>,
    vertical: Option<AxisWrite>,
    wrap: Option<String>,
    behind_doc: Option<bool>,
    wrap_distances: Option<DistancesJson>,
    z_order: Option<u32>,
}

/// The read view of one axis.
fn read_horizontal(axis: AnchorHorizontal) -> AxisRead {
    let (align, offset) = match axis.position {
        HorizontalPosition::Align(align) => (Some(horizontal_align_token(align)), None),
        #[allow(clippy::cast_precision_loss)] // EMU offsets are far below 2^53
        HorizontalPosition::Offset(emu) => (None, Some(emu as f64)),
    };
    AxisRead {
        relative_from: horizontal_anchor_token(axis.relative_from),
        align,
        offset_emu: offset,
    }
}

/// The read view of one axis.
fn read_vertical(axis: AnchorVertical) -> AxisRead {
    let (align, offset) = match axis.position {
        VerticalPosition::Align(align) => (Some(vertical_align_token(align)), None),
        #[allow(clippy::cast_precision_loss)] // EMU offsets are far below 2^53
        VerticalPosition::Offset(emu) => (None, Some(emu as f64)),
    };
    AxisRead {
        relative_from: vertical_anchor_token(axis.relative_from),
        align,
        offset_emu: offset,
    }
}

/// An offset in EMU, rejected rather than truncated when it leaves the domain
/// the model validates against.
fn offset_emu(value: f64, label: &str) -> Result<i64, String> {
    if !value.is_finite() {
        return Err(format!("{label} is not a finite number"));
    }
    let rounded = value.round();
    #[allow(clippy::cast_precision_loss)] // the comparison only needs the magnitude
    if rounded.abs() > MAX_EMU as f64 {
        return Err(format!("{label} is outside the representable EMU range"));
    }
    #[allow(clippy::cast_possible_truncation)] // bounded immediately above
    Ok(rounded as i64)
}

/// A non-negative exclusion distance.
fn distance_emu(value: f64, label: &str) -> Result<i64, String> {
    let emu = offset_emu(value, label)?;
    if emu < 0 {
        return Err(format!("{label} cannot be negative"));
    }
    Ok(emu)
}

/// Applies a requested horizontal axis onto the object's current one.
///
/// Supplying both an alignment and an offset is an error rather than a silent
/// precedence rule: `wp:positionH` holds one or the other, so a host that sent
/// both does not agree with the schema about what it asked for.
fn apply_horizontal(
    current: AnchorHorizontal,
    want: &AxisWrite,
) -> Result<AnchorHorizontal, String> {
    let relative_from = match want.relative_from.as_deref() {
        Some(token) => parse_horizontal_anchor(token)
            .ok_or_else(|| format!("unknown horizontal reference {token:?}"))?,
        None => current.relative_from,
    };
    let position = match (want.align.as_deref(), want.offset_emu) {
        (Some(_), Some(_)) => {
            return Err(POSITION_H_AMBIGUOUS.to_owned());
        }
        (Some(token), None) => HorizontalPosition::Align(
            parse_horizontal_align(token)
                .ok_or_else(|| format!("unknown horizontal alignment {token:?}"))?,
        ),
        (None, Some(value)) => {
            HorizontalPosition::Offset(offset_emu(value, "the horizontal offset")?)
        }
        (None, None) => current.position,
    };
    Ok(AnchorHorizontal {
        relative_from,
        position,
    })
}

/// Applies a requested vertical axis onto the object's current one.
fn apply_vertical(current: AnchorVertical, want: &AxisWrite) -> Result<AnchorVertical, String> {
    let relative_from = match want.relative_from.as_deref() {
        Some(token) => parse_vertical_anchor(token)
            .ok_or_else(|| format!("unknown vertical reference {token:?}"))?,
        None => current.relative_from,
    };
    let position = match (want.align.as_deref(), want.offset_emu) {
        (Some(_), Some(_)) => {
            return Err(POSITION_V_AMBIGUOUS.to_owned());
        }
        (Some(token), None) => VerticalPosition::Align(
            parse_vertical_align(token)
                .ok_or_else(|| format!("unknown vertical alignment {token:?}"))?,
        ),
        (None, Some(value)) => VerticalPosition::Offset(offset_emu(value, "the vertical offset")?),
        (None, None) => current.position,
    };
    Ok(AnchorVertical {
        relative_from,
        position,
    })
}

// ---------------------------------------------------------------------------
// Model traversal
//
// One shape of walk, used by every command below: find the paragraph holding a
// top-level object, hand its inlines to a mutator, commit `SetInlines`.
// ---------------------------------------------------------------------------

/// The anchor of a top-level object within `inlines`, and whether the object is
/// there at all. `Some(None)` is "found, and inline"; `None` is "not here".
fn top_level_anchor(inlines: &[InlineNode], object: NodeId) -> Option<Option<DrawingAnchor>> {
    for inline in inlines {
        match inline {
            InlineNode::Drawing(drawing) if drawing.id == object => return Some(None),
            InlineNode::AnchoredDrawing(drawing) if drawing.id == object => {
                return Some(Some(drawing.anchor.clone()));
            }
            InlineNode::TextBox(text_box) if text_box.id == object => {
                return Some(text_box.anchor.clone());
            }
            InlineNode::Group(group) if group.id == object => return Some(group.anchor.clone()),
            InlineNode::Hyperlink(link) => {
                if let Some(found) = top_level_anchor(&link.inlines, object) {
                    return Some(found);
                }
            }
            InlineNode::Revision(revision) => {
                if let Some(found) = top_level_anchor(&revision.inlines, object) {
                    return Some(found);
                }
            }
            InlineNode::Sdt(sdt) => {
                if let Some(found) = top_level_anchor(&sdt.inlines, object) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// The stacking key of a top-level floating object.
fn top_level_z(inlines: &[InlineNode], object: NodeId) -> Option<u32> {
    for inline in inlines {
        match inline {
            InlineNode::AnchoredDrawing(drawing) if drawing.id == object => {
                return drawing.relative_height;
            }
            InlineNode::TextBox(text_box) if text_box.id == object => {
                return text_box.relative_height;
            }
            InlineNode::Group(group) if group.id == object => return group.relative_height,
            InlineNode::Hyperlink(link) => {
                if let Some(found) = top_level_z(&link.inlines, object) {
                    return Some(found);
                }
            }
            InlineNode::Revision(revision) => {
                if let Some(found) = top_level_z(&revision.inlines, object) {
                    return Some(found);
                }
            }
            InlineNode::Sdt(sdt) => {
                if let Some(found) = top_level_z(&sdt.inlines, object) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// Runs `edit` against the top-level object `object` wherever it sits in
/// `inlines` (descending through the hyperlink / revision / SDT wrappers a
/// floating object can be nested in), reporting whether it was found.
fn with_top_level<R>(
    inlines: &mut [InlineNode],
    object: NodeId,
    edit: &mut impl FnMut(&mut InlineNode) -> R,
) -> Option<R> {
    for inline in inlines.iter_mut() {
        let matched = match inline {
            InlineNode::Drawing(drawing) => drawing.id == object,
            InlineNode::AnchoredDrawing(drawing) => drawing.id == object,
            InlineNode::TextBox(text_box) => text_box.id == object,
            InlineNode::Group(group) => group.id == object,
            _ => false,
        };
        if matched {
            return Some(edit(inline));
        }
        let nested = match inline {
            InlineNode::Hyperlink(link) => Some(&mut link.inlines),
            InlineNode::Revision(revision) => Some(&mut revision.inlines),
            InlineNode::Sdt(sdt) => Some(&mut sdt.inlines),
            _ => None,
        };
        if let Some(nested) = nested
            && let Some(result) = with_top_level(nested, object, edit)
        {
            return Some(result);
        }
    }
    None
}

/// The id of a top-level object inline, when that is what `inline` is.
const fn top_level_object_id(inline: &InlineNode) -> Option<NodeId> {
    match inline {
        InlineNode::Drawing(drawing) => Some(drawing.id),
        InlineNode::AnchoredDrawing(drawing) => Some(drawing.id),
        InlineNode::TextBox(text_box) => Some(text_box.id),
        InlineNode::Group(group) => Some(group.id),
        _ => None,
    }
}

/// The anchor a newly-floated object gets: Word's default when an inline object
/// is given any wrap other than "in line with text" — square wrap, positioned
/// where it already was relative to its column and its paragraph.
fn default_float_anchor() -> DrawingAnchor {
    DrawingAnchor {
        horizontal: AnchorHorizontal {
            relative_from: HorizontalAnchor::Column,
            position: HorizontalPosition::Offset(0),
        },
        vertical: AnchorVertical {
            relative_from: VerticalAnchor::Paragraph,
            position: VerticalPosition::Offset(0),
        },
        wrap: WrapMode::Square,
        wrap_text: None,
        wrap_distances: WrapDistances::default(),
        behind_doc: false,
        wrap_polygon: None,
    }
}

// ---------------------------------------------------------------------------
// Grouping
// ---------------------------------------------------------------------------

/// One member of a pending group: the inline that will become a child, and the
/// page-space box layout actually placed it at.
struct Member {
    inline: InlineNode,
    left_emu: i64,
    top_emu: i64,
    width_emu: i64,
    height_emu: i64,
}

/// Turns a top-level floating inline into the group child that carries it
/// without loss, positioned at `offset` in the new group's child space.
///
/// A picture becomes a `pic:pic`, a text box a `wps:wsp`, a group a nested
/// `wpg:grpSp`. Nothing is dropped: the anchor and the stacking key are the
/// only fields that do not survive, and those are the group's job now.
fn into_group_child(inline: InlineNode, offset: PointEmu, extent: Extent) -> Option<GroupChild> {
    Some(match inline {
        InlineNode::AnchoredDrawing(drawing) => {
            let drawing = *drawing;
            GroupChild::Picture(GroupPicture {
                id: drawing.id,
                media: drawing.media,
                offset,
                extent: drawing.extent,
                descr: drawing.descr,
                crop: drawing.crop,
                opacity: drawing.opacity,
                hyperlink: drawing.hyperlink,
                border: drawing.border,
                flip_h: drawing.flip_h,
                flip_v: drawing.flip_v,
                rotation: drawing.rotation,
            })
        }
        InlineNode::TextBox(text_box) => {
            let text_box = *text_box;
            GroupChild::TextBox(GroupTextBox {
                id: text_box.id,
                offset,
                extent: text_box.extent.unwrap_or(extent),
                // A top-level `TextBox` models no preset geometry, so the
                // honest conversion is the rectangle it was being drawn as —
                // the same shape import gives a lone `wps:wsp` and export
                // writes back for one. Inventing a geometry here would make
                // grouping change how a box looks.
                geometry: ShapeGeometry::Rectangle,
                preset: None,
                adjustments: Vec::new(),
                blocks: text_box.blocks,
                fill: text_box.fill,
                border: text_box.border,
                body_properties: text_box.body_properties,
                hyperlink: text_box.hyperlink,
                flip_h: false,
                flip_v: false,
                rotation: None,
            })
        }
        InlineNode::Group(group) => {
            let mut group = *group;
            group.anchor = None;
            group.relative_height = None;
            group.transform.offset = offset;
            group.transform.extent = group.extent;
            GroupChild::Group(Box::new(group))
        }
        _ => return None,
    })
}

/// Turns a group child back into the top-level floating inline that carries it
/// without loss, anchored at the given page-local EMU position.
///
/// A shape has no top-level inline of its own — `InlineNode` has no `Shape`
/// variant — so it comes back as a group of one, which is the carrier
/// `insertShape` already uses and the shape a lone autoshape imports as. A
/// rotated or flipped text box comes back the same way, because a top-level
/// `TextBox` models neither: wrapping it is what makes ungroup LOSSLESS rather
/// than quietly straightening a tilted callout.
fn out_of_group_child(
    child: GroupChild,
    anchor: DrawingAnchor,
    relative_height: Option<u32>,
    group_id: impl FnOnce() -> Result<NodeId, String>,
) -> Result<InlineNode, String> {
    Ok(match child {
        GroupChild::Picture(picture) => InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
            id: picture.id,
            media: picture.media,
            extent: picture.extent,
            anchor,
            descr: picture.descr,
            relative_height,
            crop: picture.crop,
            opacity: picture.opacity,
            hyperlink: picture.hyperlink,
            border: picture.border,
            flip_h: picture.flip_h,
            flip_v: picture.flip_v,
            rotation: picture.rotation,
        })),
        GroupChild::TextBox(text_box)
            if !text_box.flip_h && !text_box.flip_v && text_box.rotation.is_none() =>
        {
            InlineNode::TextBox(Box::new(TextBox {
                hyperlink: text_box.hyperlink,
                id: text_box.id,
                anchor: Some(anchor),
                relative_height,
                extent: Some(text_box.extent),
                fill: text_box.fill,
                border: text_box.border,
                body_properties: text_box.body_properties,
                blocks: text_box.blocks,
            }))
        }
        GroupChild::Group(nested) => {
            let mut group = *nested;
            group.anchor = Some(anchor);
            group.relative_height = relative_height;
            group.extent = group.transform.extent;
            // The offset is NOT reset: the caller has already set it to the
            // group's declared-box-to-content inset, which is what keeps the
            // content on the anchor.
            InlineNode::Group(Box::new(group))
        }
        // A shape, or a transformed text box: wrapped in a group of one.
        other => {
            let extent = group_child_extent(&other);
            let mut child = other;
            set_group_child_offset(&mut child, PointEmu { x_emu: 0, y_emu: 0 });
            InlineNode::Group(Box::new(WordprocessingGroup {
                id: group_id()?,
                anchor: Some(anchor),
                relative_height,
                extent,
                transform: GroupTransform {
                    offset: PointEmu { x_emu: 0, y_emu: 0 },
                    extent,
                    child_offset: PointEmu { x_emu: 0, y_emu: 0 },
                    child_extent: extent,
                    flip_h: false,
                    flip_v: false,
                    rotation: None,
                },
                hyperlink: None,
                children: vec![child],
            }))
        }
    })
}

/// A child's own box size, in its parent group's child space.
const fn group_child_extent(child: &GroupChild) -> Extent {
    match child {
        GroupChild::Picture(picture) => picture.extent,
        GroupChild::TextBox(text_box) => text_box.extent,
        GroupChild::Shape(shape) => shape.extent,
        GroupChild::Group(group) => group.transform.extent,
    }
}

/// Moves a child's top-left within its parent group's child space.
fn set_group_child_offset(child: &mut GroupChild, offset: PointEmu) {
    match child {
        GroupChild::Picture(picture) => picture.offset = offset,
        GroupChild::TextBox(text_box) => text_box.offset = offset,
        GroupChild::Shape(shape) => shape.offset = offset,
        GroupChild::Group(group) => group.transform.offset = offset,
    }
}

/// An EMU magnitude from a float measurement, clamped into the model's domain.
fn round_emu(value: f64) -> i64 {
    if !value.is_finite() {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    {
        (value.round() as i64).clamp(-MAX_EMU, MAX_EMU)
    }
}

/// Child space to parent space, per axis — `a:ext / a:chExt`, the factor layout
/// maps a group's children by. `1.0` on a degenerate axis, which is the
/// identity layout already assumes there.
///
/// The inverse of `casual_doc_wasm`'s page-to-child `child_space_scale`, and
/// named for the direction it goes so the two cannot be confused at a call site.
fn parent_space_scale(transform: &GroupTransform) -> (f64, f64) {
    let axis = |parent: i64, child: i64| {
        if child > 0 && parent > 0 {
            #[allow(clippy::cast_precision_loss)] // EMU magnitudes are below 2^53
            {
                parent as f64 / child as f64
            }
        } else {
            1.0
        }
    };
    (
        axis(transform.extent.width_emu, transform.child_extent.width_emu),
        axis(
            transform.extent.height_emu,
            transform.child_extent.height_emu,
        ),
    )
}

/// The page-space top-left, in EMU, that layout painted `child` at.
///
/// A leaf child carries its own identity into the placed anchors, so it is a
/// direct lookup. A NESTED group paints no anchor of its own, so its origin is
/// the top-left of its own descendants — which is the same union layout would
/// compute for it.
///
/// `placed` is the already-collected box list; this never re-reads the layout,
/// so a group of *n* children costs one pass, not *n*.
fn child_page_origin(child: &GroupChild, placed: &[(NodeId, i64, i64)]) -> Option<(i64, i64)> {
    fn leaf(id: NodeId, placed: &[(NodeId, i64, i64)]) -> Option<(i64, i64)> {
        placed
            .iter()
            .find(|(subject, _, _)| *subject == id)
            .map(|(_, left, top)| (*left, *top))
    }
    match child {
        GroupChild::Picture(picture) => leaf(picture.id, placed),
        GroupChild::TextBox(text_box) => leaf(text_box.id, placed),
        GroupChild::Shape(shape) => leaf(shape.id, placed),
        GroupChild::Group(nested) => {
            let mut corner: Option<(i64, i64)> = None;
            for inner in &nested.children {
                let found = child_page_origin(inner, placed)?;
                corner =
                    Some(corner.map_or(found, |(left, top)| (left.min(found.0), top.min(found.1))));
            }
            corner
        }
    }
}

/// `child` with its own geometry scaled out of its parent group's child space
/// into the space that parent sat in.
///
/// Only the SIZE is rewritten: the offset is discarded by the caller, which
/// anchors the child at the page position layout painted it at.
fn scaled_group_child(mut child: GroupChild, (sx, sy): (f64, f64)) -> GroupChild {
    if (sx - 1.0).abs() < f64::EPSILON && (sy - 1.0).abs() < f64::EPSILON {
        return child;
    }
    let scale = |extent: Extent| Extent {
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        width_emu: ((extent.width_emu as f64 * sx).round() as i64).clamp(0, MAX_EMU),
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        height_emu: ((extent.height_emu as f64 * sy).round() as i64).clamp(0, MAX_EMU),
    };
    match &mut child {
        GroupChild::Picture(picture) => picture.extent = scale(picture.extent),
        GroupChild::TextBox(text_box) => text_box.extent = scale(text_box.extent),
        GroupChild::Shape(shape) => shape.extent = scale(shape.extent),
        GroupChild::Group(nested) => {
            // Scaling the nested group's PARENT-space box while leaving its own
            // child space alone is what compounds the two scales, so its own
            // children keep the size they were drawn at as well.
            nested.transform.extent = scale(nested.transform.extent);
            nested.extent = nested.transform.extent;
        }
    }
    child
}

#[wasm_bindgen]
impl WasmDocument {
    /// The full OOXML position of the object `node`, as the JSON a Layout ▸
    /// Position dialog is drawn from — or `""` if `node` is not an object.
    ///
    /// ```json
    /// {"floating":true,
    ///  "horizontal":{"relativeFrom":"column","offsetEmu":0},
    ///  "vertical":{"relativeFrom":"paragraph","align":"top"},
    ///  "wrap":"square","behindDoc":false,
    ///  "wrapDistances":{"topEmu":0,"bottomEmu":0,"startEmu":114300,"endEmu":114300},
    ///  "zOrder":3}
    /// ```
    ///
    /// An inline object answers `{"floating":false}` and nothing else, because
    /// an inline object HAS no anchor — reporting a position for it would
    /// invent one. Every token is the OOXML token (`ST_RelFromH`,
    /// `ST_RelFromV`, `ST_AlignH`, `ST_AlignV`), so a host never has to
    /// translate and a value read here can be written straight back.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = objectPosition)]
    #[must_use]
    pub fn object_position(&self, node: &str) -> String {
        let Ok(object) = node.parse::<NodeId>() else {
            return String::new();
        };
        let Some(paragraph) = self.paragraph_of_object(object) else {
            // Not a top-level object. A group CHILD is positioned by its
            // parent, so say that rather than answering nothing — a host
            // cannot tell an empty string from "no such object".
            return if self.paragraph_containing_inline_deep(object).is_some() {
                serde_json::to_string(&PositionRead {
                    floating: false,
                    group_child: true,
                    horizontal: None,
                    vertical: None,
                    wrap: None,
                    behind_doc: None,
                    wrap_distances: None,
                    z_order: None,
                })
                .unwrap_or_default()
            } else {
                String::new()
            };
        };
        let Some(source) = find_paragraph_any(&self.document, paragraph) else {
            return String::new();
        };
        let Some(anchor) = top_level_anchor(&source.inlines, object) else {
            return String::new();
        };
        let read = match anchor {
            None => PositionRead {
                floating: false,
                group_child: false,
                horizontal: None,
                vertical: None,
                wrap: None,
                behind_doc: None,
                wrap_distances: None,
                z_order: None,
            },
            #[allow(clippy::cast_precision_loss)] // EMU distances are far below 2^53
            Some(anchor) => PositionRead {
                floating: true,
                group_child: false,
                horizontal: Some(read_horizontal(anchor.horizontal)),
                vertical: Some(read_vertical(anchor.vertical)),
                wrap: Some(wrap_token(anchor.wrap)),
                behind_doc: Some(anchor.behind_doc),
                wrap_distances: Some(DistancesJson {
                    top_emu: anchor.wrap_distances.top_emu as f64,
                    bottom_emu: anchor.wrap_distances.bottom_emu as f64,
                    start_emu: anchor.wrap_distances.start_emu as f64,
                    end_emu: anchor.wrap_distances.end_emu as f64,
                }),
                z_order: top_level_z(&source.inlines, object),
            },
        };
        serde_json::to_string(&read).unwrap_or_default()
    }

    /// Sets the object `node`'s position in the OOXML model, as one undoable
    /// `SetAnchor` — the commit of Word's Layout ▸ Position dialog.
    ///
    /// `position` is the JSON [`object_position`](Self::object_position)
    /// returns, with every field optional: an omitted field keeps what the
    /// object already has, so a dialog that changes one control does not
    /// restate (and thereby overwrite) the rest. An unknown key, an unknown
    /// token, or an alignment and an offset on the same axis is an ERROR, not
    /// a value quietly ignored.
    ///
    /// # Errors
    ///
    /// When `node` is not a floating object (an inline one has no anchor to
    /// position — [`set_object_anchor_kind`](Self::set_object_anchor_kind)
    /// floats it first), when the JSON does not parse, or when a token or a
    /// magnitude is out of range.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = setObjectPosition)]
    pub fn set_object_position(
        &mut self,
        node: &str,
        position: &str,
    ) -> Result<EditResult, JsValue> {
        self.set_object_position_inner(node, position)
            .map_err(to_js)
    }

    /// [`set_object_position`](Self::set_object_position) with a plain error,
    /// so the behaviour is reachable from a native test.
    fn set_object_position_inner(
        &mut self,
        node: &str,
        position: &str,
    ) -> Result<EditResult, String> {
        let object = node
            .parse::<NodeId>()
            .map_err(|_| "invalid node id".to_owned())?;
        let want: PositionWrite =
            serde_json::from_str(position).map_err(|error| format!("bad position: {error}"))?;
        let paragraph = self
            .paragraph_of_object(object)
            .ok_or_else(|| NOT_AN_OBJECT.to_owned())?;
        let source = find_paragraph_any(&self.document, paragraph)
            .ok_or_else(|| NOT_AN_OBJECT.to_owned())?;
        let found =
            top_level_anchor(&source.inlines, object).ok_or_else(|| NOT_AN_OBJECT.to_owned())?;
        if want.floating == Some(false) {
            return Err(POSITION_NOT_INLINE.to_owned());
        }
        let mut anchor = found.ok_or_else(|| POSITION_NOT_FLOATING.to_owned())?;
        if let Some(z) = want.z_order
            && top_level_z(&source.inlines, object) != Some(z)
        {
            return Err(POSITION_NOT_RESTACK.to_owned());
        }
        if let Some(axis) = want.horizontal.as_ref() {
            anchor.horizontal = apply_horizontal(anchor.horizontal, axis)?;
        }
        if let Some(axis) = want.vertical.as_ref() {
            anchor.vertical = apply_vertical(anchor.vertical, axis)?;
        }
        if let Some(token) = want.wrap.as_deref() {
            anchor.wrap =
                parse_wrap(token).ok_or_else(|| format!("unknown wrap mode {token:?}"))?;
        }
        if let Some(behind) = want.behind_doc {
            anchor.behind_doc = behind;
        }
        if let Some(distances) = want.wrap_distances.as_ref() {
            anchor.wrap_distances = WrapDistances {
                top_emu: distance_emu(distances.top_emu, "the top wrap distance")?,
                bottom_emu: distance_emu(distances.bottom_emu, "the bottom wrap distance")?,
                start_emu: distance_emu(distances.start_emu, "the leading wrap distance")?,
                end_emu: distance_emu(distances.end_emu, "the trailing wrap distance")?,
            };
        }
        self.apply_action_caret_as(
            vec![Operation::SetAnchor {
                object,
                anchor: Box::new(anchor),
            }],
            Pos::new(object, 0),
            HistoryKind::ObjectMove,
        )
    }

    /// Word's "In line with text" versus every other wrap: converts the object
    /// `node` between the run flow and the float layer, as one undoable action.
    ///
    /// `kind` is `"inline"` or `"floating"`. Making an object inline drops its
    /// anchor and its stacking key — those describe a position the flow now
    /// decides, so keeping them would be keeping a claim the document no longer
    /// makes. Floating an inline object gives it Word's default for the
    /// conversion: square wrap, at rest against its column and its paragraph.
    ///
    /// The conversion is real, not a flag: an inline picture is a `wp:inline`
    /// and a floating one is a `wp:anchor`, two different nodes, so this
    /// rewrites the node. Calling it for the kind the object already has is a
    /// no-op that still reports success, so a host can bind it to a radio group
    /// without tracking state.
    ///
    /// # Errors
    ///
    /// When `node` is not a top-level object, or `kind` is not one of the two
    /// tokens.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = setObjectAnchorKind)]
    pub fn set_object_anchor_kind(
        &mut self,
        node: &str,
        kind: &str,
    ) -> Result<EditResult, JsValue> {
        self.set_object_anchor_kind_inner(node, kind).map_err(to_js)
    }

    /// [`set_object_anchor_kind`](Self::set_object_anchor_kind) with a plain
    /// error, so the behaviour is reachable from a native test.
    fn set_object_anchor_kind_inner(
        &mut self,
        node: &str,
        kind: &str,
    ) -> Result<EditResult, String> {
        let floating = match kind {
            "inline" => false,
            "floating" => true,
            other => return Err(format!("unknown anchor kind {other:?}")),
        };
        let object = node
            .parse::<NodeId>()
            .map_err(|_| "invalid node id".to_owned())?;
        let paragraph = self
            .paragraph_of_object(object)
            .ok_or_else(|| NOT_AN_OBJECT.to_owned())?;
        let source = find_paragraph_any(&self.document, paragraph)
            .ok_or_else(|| NOT_AN_OBJECT.to_owned())?;
        let mut inlines = source.inlines.clone();
        let changed = with_top_level(&mut inlines, object, &mut |inline| {
            convert_anchor_kind(inline, floating)
        })
        .ok_or_else(|| NOT_AN_OBJECT.to_owned())??;
        if !changed {
            // Already the requested kind. Report success without an undo entry:
            // a radio group that re-asserts the current choice should not fill
            // the undo stack with actions that changed nothing.
            return Ok(self.finish_edit(Pos::new(object, 0)));
        }
        self.apply_action_caret_as(
            vec![Operation::SetInlines {
                node: paragraph,
                inlines,
            }],
            Pos::new(object, 0),
            HistoryKind::ObjectMove,
        )
    }

    /// Whether the objects named by `nodes` (a JSON array of node ids) can be
    /// grouped, as `{"can":true}` or `{"can":false,"reason":"…"}`.
    ///
    /// The reason exists so a host ships a DISABLED Group command carrying why
    /// rather than a dead button (SKILL §10: never a dead control).
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = canGroupObjects)]
    #[must_use]
    pub fn can_group_objects(&self, nodes: &str) -> String {
        match self.group_members(nodes) {
            Ok(_) => "{\"can\":true}".to_owned(),
            Err(reason) => serde_json::json!({"can": false, "reason": reason}).to_string(),
        }
    }

    /// Word's Arrange ▸ Group: replaces the floating objects named by `nodes` (a
    /// JSON array of node ids) with one `wpg:wgp` containing them, as one
    /// undoable action.
    ///
    /// The group's box is the union of where layout ACTUALLY PLACED the
    /// members, and each child keeps its position relative to that union — so
    /// grouping moves nothing on the page, which is the whole guarantee.
    /// Children are carried in the order they occur in the paragraph, which is
    /// their paint order both before and after, so grouping does not restack
    /// them either. An identity child transform (`a:chOff` = 0, `a:chExt` =
    /// the group extent) means resizing the group scales its children, which is
    /// what Word's group handles do.
    ///
    /// Members must be floating, top-level, in the same paragraph, and on the
    /// same page. Word imposes the first three; the fourth is ours, and it is
    /// honest rather than arbitrary — a group has ONE anchor, so members that
    /// laid out on different pages cannot all keep their positions, and quietly
    /// moving one is worse than declining.
    ///
    /// # Errors
    ///
    /// With the reason [`can_group_objects`](Self::can_group_objects) reports.
    ///
    /// **O(document), ONE walk**, then O(members).
    #[wasm_bindgen(js_name = groupObjects)]
    pub fn group_objects(&mut self, nodes: &str) -> Result<EditResult, JsValue> {
        self.group_objects_inner(nodes).map_err(to_js)
    }

    /// [`group_objects`](Self::group_objects) with a plain error, so the
    /// behaviour is reachable from a native test.
    fn group_objects_inner(&mut self, nodes: &str) -> Result<EditResult, String> {
        let (paragraph, ids) = self.group_members(nodes)?;
        let source = find_paragraph_any(&self.document, paragraph)
            .ok_or_else(|| NOT_AN_OBJECT.to_owned())?;
        let mut inlines = source.inlines.clone();

        // The placed rectangles, collected in ONE pass over the object boxes —
        // not a lookup per id, which is the shape that has already cost this
        // repository an O(n²) (SKILL §8).
        let boxes: Vec<_> = self
            .object_boxes()
            .into_iter()
            .filter(|placed| ids.contains(&placed.root))
            .collect();
        if boxes.len() != ids.len() {
            return Err(GROUP_UNPLACED.to_owned());
        }

        // Document order, which is paint order, which is the child order. The
        // members are taken out of the paragraph in one pass; a member that is
        // not directly in it (inside a hyperlink or a content control) never
        // reaches here, because `group_members` already refused it.
        let anchors: Vec<Option<DrawingAnchor>> = ids
            .iter()
            .map(|id| top_level_anchor(&inlines, *id).flatten())
            .collect();
        let zs: Vec<Option<u32>> = ids.iter().map(|id| top_level_z(&inlines, *id)).collect();
        let mut first_index = usize::MAX;
        let mut members: Vec<Member> = Vec::with_capacity(ids.len());
        let mut index = 0;
        inlines.retain(|inline| {
            let taken = top_level_object_id(inline).filter(|id| ids.contains(id));
            if let Some(id) = taken {
                let placed = boxes
                    .iter()
                    .find(|placed| placed.root == id)
                    .expect("collected above");
                first_index = first_index.min(index);
                members.push(Member {
                    inline: inline.clone(),
                    left_emu: i64::from(placed.rect.origin.x.raw()) * EMU_PER_TWIP_I64,
                    top_emu: i64::from(placed.rect.origin.y.raw()) * EMU_PER_TWIP_I64,
                    width_emu: i64::from(placed.rect.size.width.raw()) * EMU_PER_TWIP_I64,
                    height_emu: i64::from(placed.rect.size.height.raw()) * EMU_PER_TWIP_I64,
                });
            }
            index += 1;
            taken.is_none()
        });
        if members.len() != ids.len() {
            return Err(GROUP_NOT_SIBLING.to_owned());
        }

        let left = members.iter().map(|m| m.left_emu).min().unwrap_or(0);
        let top = members.iter().map(|m| m.top_emu).min().unwrap_or(0);
        let right = members
            .iter()
            .map(|m| m.left_emu.saturating_add(m.width_emu))
            .max()
            .unwrap_or(0);
        let bottom = members
            .iter()
            .map(|m| m.top_emu.saturating_add(m.height_emu))
            .max()
            .unwrap_or(0);
        let extent = Extent {
            width_emu: (right - left).max(1),
            height_emu: (bottom - top).max(1),
        };

        // The group inherits the first member's wrap, exclusion distances and
        // behind-text band — Word's answer too, and the alternative (a fresh
        // default) would silently re-wrap text around objects the user only
        // asked to group.
        let anchor = anchor_at_page(
            anchors
                .into_iter()
                .flatten()
                .next()
                .unwrap_or_else(default_float_anchor),
            left,
            top,
        );
        let relative_height = zs.into_iter().flatten().max();

        let mut children = Vec::with_capacity(members.len());
        for member in members {
            // A leaf's placed corner IS its box, so its new offset is just the
            // delta. A GROUP's placed corner is where its CONTENT starts, which
            // its declared `a:off`/`a:chOff` need not agree with — a child
            // dragged out of the declared box makes them disagree — so the
            // group is translated by the difference between the two rather
            // than pinned to the content corner, which would shift everything
            // inside it by the group's own internal offset.
            let offset = match &member.inline {
                InlineNode::Group(group) => {
                    let bounds = crate::group_content_bounds(group, group.transform)
                        .ok_or_else(|| GROUP_UNBOUNDED.to_owned())?;
                    PointEmu {
                        x_emu: group.transform.offset.x_emu + member.left_emu
                            - round_emu(bounds.left)
                            - left,
                        y_emu: group.transform.offset.y_emu + member.top_emu
                            - round_emu(bounds.top)
                            - top,
                    }
                }
                _ => PointEmu {
                    x_emu: member.left_emu - left,
                    y_emu: member.top_emu - top,
                },
            };
            let child_extent = Extent {
                width_emu: member.width_emu.max(1),
                height_emu: member.height_emu.max(1),
            };
            children.push(
                into_group_child(member.inline, offset, child_extent)
                    .ok_or_else(|| GROUP_INLINE.to_owned())?,
            );
        }

        let group = WordprocessingGroup {
            id: self
                .edit_ids
                .next_id()
                .map_err(|_| "id space exhausted".to_owned())?,
            anchor: Some(anchor),
            relative_height,
            extent,
            transform: GroupTransform {
                offset: PointEmu { x_emu: 0, y_emu: 0 },
                extent,
                child_offset: PointEmu { x_emu: 0, y_emu: 0 },
                child_extent: extent,
                flip_h: false,
                flip_v: false,
                rotation: None,
            },
            hyperlink: None,
            children,
        };
        let group_id = group.id;
        let at = first_index.min(inlines.len());
        inlines.insert(at, InlineNode::Group(Box::new(group)));

        self.apply_action_caret_as(
            vec![Operation::SetInlines {
                node: paragraph,
                inlines,
            }],
            Pos::new(group_id, 0),
            HistoryKind::ObjectMove,
        )
    }

    /// Word's Arrange ▸ Ungroup: replaces the group `node` with its children as
    /// independent floating objects, each anchored where it was already
    /// painted, as one undoable action.
    ///
    /// The group's transform is APPLIED, not discarded: a child of a group
    /// scaled to half size comes out at the size it was drawn at, because that
    /// is what the reader was looking at. A nested group comes out as a group,
    /// which is what makes ungrouping a nest peel one layer at a time the way
    /// Word does. Nothing is dropped — see
    /// `out_of_group_child` for the two children that come back wrapped in a
    /// group of one rather than losing a property `InlineNode` cannot model.
    ///
    /// # Errors
    ///
    /// When `node` is not a top-level floating group, or it has no children.
    ///
    /// **O(document), ONE walk**, then O(children).
    #[wasm_bindgen(js_name = ungroupObject)]
    pub fn ungroup_object(&mut self, node: &str) -> Result<EditResult, JsValue> {
        self.ungroup_object_inner(node).map_err(to_js)
    }

    /// [`ungroup_object`](Self::ungroup_object) with a plain error, so the
    /// behaviour is reachable from a native test.
    fn ungroup_object_inner(&mut self, node: &str) -> Result<EditResult, String> {
        let object = node
            .parse::<NodeId>()
            .map_err(|_| "invalid node id".to_owned())?;
        let paragraph = self
            .paragraph_of_object(object)
            .ok_or_else(|| NOT_A_GROUP.to_owned())?;

        // Positions come from where layout PAINTED each child, not from
        // arithmetic on the group's own rect. A group root's placed rect is the
        // union of its children, so a mapping written from it drifts as soon as
        // the child transform is not the identity — which is what a resized
        // group is. One pass over the boxes, indexed by subject.
        let placed: Vec<(NodeId, i64, i64)> = self
            .object_boxes_including_group_children()
            .into_iter()
            .map(|entry| {
                (
                    entry.subject,
                    i64::from(entry.rect.origin.x.raw()) * EMU_PER_TWIP_I64,
                    i64::from(entry.rect.origin.y.raw()) * EMU_PER_TWIP_I64,
                )
            })
            .collect();

        let source =
            find_paragraph_any(&self.document, paragraph).ok_or_else(|| NOT_A_GROUP.to_owned())?;
        let mut inlines = source.inlines.clone();

        let at = inlines
            .iter()
            .position(|inline| matches!(inline, InlineNode::Group(group) if group.id == object))
            // Two different causes used to arrive here as one message: an object
            // that is not a group at all, and a group nested inside a hyperlink
            // or a content control. They need different sentences, so the model
            // is asked which it is rather than the reader being told the wrong
            // one.
            .ok_or_else(|| {
                if crate::object_group_any_surface(&self.document, object).is_some() {
                    GROUP_IN_WRAPPER.to_owned()
                } else {
                    NOT_A_GROUP.to_owned()
                }
            })?;
        let InlineNode::Group(group) = inlines.remove(at) else {
            unreachable!("matched immediately above");
        };
        let group = *group;
        let anchor = group
            .anchor
            .clone()
            .ok_or_else(|| GROUP_NOT_FLOATING.to_owned())?;
        if group.children.is_empty() {
            return Err(GROUP_EMPTY.to_owned());
        }
        // The child space the group scaled its children by. Coming out of the
        // group, each child's own extent is in THAT space, so it is scaled into
        // page space — otherwise a child of a half-scale group comes back at
        // twice the size it was being drawn at.
        let scale = parent_space_scale(&group.transform);
        let relative_height = group.relative_height;

        let mut replacements = Vec::with_capacity(group.children.len());
        for child in group.children {
            let (left, top) = child_page_origin(&child, &placed)
                .ok_or_else(|| GROUP_PARTLY_UNPLACED.to_owned())?;
            // The anchor goes on the content corner, so a nested group keeps
            // the offset between its declared box and its content — the same
            // correction grouping applies, in reverse. A leaf has no such
            // offset and rests at zero.
            let child = match child {
                GroupChild::Group(nested) => {
                    let bounds = crate::group_content_bounds(&nested, nested.transform)
                        .ok_or_else(|| GROUP_UNBOUNDED.to_owned())?;
                    let inset = PointEmu {
                        x_emu: -round_emu(
                            (bounds.left - nested.transform.offset.x_emu as f64) * scale.0,
                        ),
                        y_emu: -round_emu(
                            (bounds.top - nested.transform.offset.y_emu as f64) * scale.1,
                        ),
                    };
                    let mut nested = scaled_group_child(GroupChild::Group(nested), scale);
                    set_group_child_offset(&mut nested, inset);
                    nested
                }
                leaf => scaled_group_child(leaf, scale),
            };
            let ids = &mut self.edit_ids;
            replacements.push(out_of_group_child(
                child,
                anchor_at_page(anchor.clone(), left, top),
                relative_height,
                || ids.next_id().map_err(|_| "id space exhausted".to_owned()),
            )?);
        }

        let first = replacements
            .first()
            .and_then(top_level_object_id)
            .unwrap_or(object);
        for (index, inline) in replacements.into_iter().enumerate() {
            let at = (at + index).min(inlines.len());
            inlines.insert(at, inline);
        }

        self.apply_action_caret_as(
            vec![Operation::SetInlines {
                node: paragraph,
                inlines,
            }],
            Pos::new(first, 0),
            HistoryKind::ObjectMove,
        )
    }

    /// Word's Arrange ▸ Bring Forward / Send Backward: restacks the object
    /// `node` as one undoable action. `order` is `"front"`, `"back"`,
    /// `"forward"` or `"backward"`.
    ///
    /// The mechanism depends on where the object sits, because OOXML stacks the
    /// two cases differently and pretending otherwise would restack the wrong
    /// thing. A **top-level** floating object is ordered by
    /// `wp:anchor@relativeHeight` against the other floats in its own band —
    /// `@behindDoc` decides whether an object is below or above the text layer,
    /// and this orders WITHIN that band, so restacking never silently moves an
    /// object through the text. A **group child** is ordered by its index among
    /// its siblings, because a group paints its children in document order.
    ///
    /// Bringing to front assigns one more than the highest key in the band
    /// rather than renumbering everything, so the action touches one object.
    ///
    /// # Errors
    ///
    /// When `node` is not a restackable object, or `order` is not one of the
    /// four tokens.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = setObjectZOrder)]
    pub fn set_object_z_order(&mut self, node: &str, order: &str) -> Result<EditResult, JsValue> {
        self.set_object_z_order_inner(node, order).map_err(to_js)
    }

    /// [`set_object_z_order`](Self::set_object_z_order) with a plain error, so
    /// the behaviour is reachable from a native test.
    fn set_object_z_order_inner(&mut self, node: &str, order: &str) -> Result<EditResult, String> {
        let step = ZStep::parse(order).ok_or_else(|| format!("unknown z-order {order:?}"))?;
        let object = node
            .parse::<NodeId>()
            .map_err(|_| "invalid node id".to_owned())?;
        if let Some(result) = self.restack_group_child(object, step)? {
            return Ok(result);
        }
        self.restack_float(object, step)
    }

    /// Word's Add Text / Edit Text on a shape: gives the shape `node` a text
    /// body it keeps its geometry behind, as one undoable action. The returned
    /// caret is the new body's paragraph, so the host focuses it and the user
    /// types straight into the shape.
    ///
    /// OOXML has one element for both — a `wps:wsp` with a `wps:txbx` IS a
    /// shape with text — so this is not a wrapper or an overlay: the shape
    /// becomes a text-bearing shape, keeping its id, its preset, its
    /// adjustment guides, its fill and its outline. A star stays a star and
    /// gets words inside it.
    ///
    /// Calling it on a shape that already has text is a no-op reporting
    /// success, so a host can bind it to a double-click without tracking
    /// state.
    ///
    /// # Errors
    ///
    /// When `node` is not a shape inside a group, or when it is a FREEFORM
    /// (`a:custGeom`) shape: `GroupTextBox` models no custom path, so giving
    /// one text would silently flatten it to its bounding rectangle. It is
    /// refused with that reason rather than quietly reshaped.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = addTextToShape)]
    pub fn add_text_to_shape(&mut self, node: &str) -> Result<EditResult, JsValue> {
        self.add_text_to_shape_inner(node).map_err(to_js)
    }

    /// [`add_text_to_shape`](Self::add_text_to_shape) with a plain error, so
    /// the behaviour is reachable from a native test.
    fn add_text_to_shape_inner(&mut self, node: &str) -> Result<EditResult, String> {
        let object = node
            .parse::<NodeId>()
            .map_err(|_| "invalid node id".to_owned())?;
        let paragraph = self
            .paragraph_containing_inline_deep(object)
            .ok_or_else(|| NOT_A_GROUP_CHILD.to_owned())?;
        let source = find_paragraph_any(&self.document, paragraph)
            .ok_or_else(|| NOT_A_GROUP_CHILD.to_owned())?;
        let mut inlines = source.inlines.clone();
        let body = self
            .edit_ids
            .next_id()
            .map_err(|_| "id space exhausted".to_owned())?;
        match add_text_in_inlines(&mut inlines, object, body)? {
            AddText::Added => {}
            // Already a text-bearing shape. Report success with the existing
            // body's caret and no undo entry.
            AddText::Already(existing) => return Ok(self.finish_edit(Pos::new(existing, 0))),
            AddText::NotAShape => return Err(NOT_A_GROUP_CHILD.to_owned()),
        }
        self.apply_action_caret_as(
            vec![Operation::SetInlines {
                node: paragraph,
                inlines,
            }],
            Pos::new(body, 0),
            HistoryKind::ObjectInsert,
        )
    }

    /// The rotation and flips of the object `node`, as
    /// `{"rotationDegrees":45.0,"flipH":false,"flipV":false}` — or `""` if
    /// `node` is not an object that models them.
    ///
    /// Degrees rather than the 60000ths OOXML stores, because a host draws a
    /// rotation handle in degrees and doing the division in JS would put the
    /// unit conversion in two places.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = objectTransform)]
    #[must_use]
    pub fn object_transform(&self, node: &str) -> String {
        let Ok(object) = node.parse::<NodeId>() else {
            return String::new();
        };
        let mut found = None;
        crate::visit_paragraphs_all_surfaces(&self.document, &mut |paragraph| {
            if found.is_none() {
                found = read_transform_in_inlines(&paragraph.inlines, object);
            }
        });
        let Some((rotation, flip_h, flip_v)) = found else {
            return String::new();
        };
        serde_json::json!({
            "rotationDegrees": rotation.map(|sixty_thousandths| {
                f64::from(sixty_thousandths) / 60_000.0
            }),
            "flipH": flip_h,
            "flipV": flip_v,
        })
        .to_string()
    }

    /// Word's Rotate: sets the object `node`'s clockwise rotation in degrees
    /// about its own centre, as one undoable action. `None` clears it.
    ///
    /// Stored as OOXML's 60000ths of a degree and normalised into
    /// `[0, 360)`, so a handle dragged round three times does not author a
    /// rotation no other producer will read back the same way.
    ///
    /// # Errors
    ///
    /// When `node` is not an object whose model carries a rotation — which a
    /// top-level text box's does not, so a host rotates one by grouping it
    /// first. The refusal SAYS that rather than succeeding and doing nothing.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = setObjectRotation)]
    pub fn set_object_rotation(
        &mut self,
        node: &str,
        degrees: Option<f64>,
    ) -> Result<EditResult, JsValue> {
        self.set_object_rotation_inner(node, degrees).map_err(to_js)
    }

    /// [`set_object_rotation`](Self::set_object_rotation) with a plain error,
    /// so the behaviour is reachable from a native test.
    fn set_object_rotation_inner(
        &mut self,
        node: &str,
        degrees: Option<f64>,
    ) -> Result<EditResult, String> {
        let rotation = match degrees {
            None => None,
            Some(value) => {
                if !value.is_finite() {
                    return Err(ROTATION_NOT_FINITE.to_owned());
                }
                let normalised = value.rem_euclid(360.0) * 60_000.0;
                #[allow(clippy::cast_possible_truncation)] // < 360 * 60000 < i32::MAX
                Some(normalised.round() as i32)
            }
        };
        self.edit_transform(node, TransformEdit::Rotation(rotation))
    }

    /// Word's Flip Horizontal / Flip Vertical: mirrors the object `node` about
    /// its own axes, as one undoable action.
    ///
    /// Both flags are absolute rather than toggles, so a host binds two
    /// checkboxes to this and the engine never disagrees with what they show.
    ///
    /// # Errors
    ///
    /// When `node` is not an object whose model carries flips.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = setObjectFlip)]
    pub fn set_object_flip(
        &mut self,
        node: &str,
        flip_h: bool,
        flip_v: bool,
    ) -> Result<EditResult, JsValue> {
        self.edit_transform(node, TransformEdit::Flip { flip_h, flip_v })
            .map_err(to_js)
    }
}

/// One floating object, as the z-order pass sees it.
struct Float {
    id: NodeId,
    paragraph: NodeId,
    z: u32,
    behind_doc: bool,
}

impl WasmDocument {
    /// The paragraph whose inlines hold the top-level OBJECT `object`, across
    /// every block surface.
    ///
    /// `paragraph_containing_inline` cannot answer this: its predicate matches
    /// runs, symbols, fields and content controls, and returns `false` for a
    /// drawing, a text box or a group — the four things this module addresses.
    /// Every command here therefore resolves its paragraph through this, and
    /// through it only, so there is one answer to "where does this object live"
    /// rather than one per entry point.
    ///
    /// **O(document), ONE walk.**
    fn paragraph_of_object(&self, object: NodeId) -> Option<NodeId> {
        let mut found = None;
        crate::visit_paragraphs_all_surfaces(&self.document, &mut |paragraph| {
            if found.is_none() && top_level_anchor(&paragraph.inlines, object).is_some() {
                found = Some(paragraph.id);
            }
        });
        found
    }

    /// Validates a grouping request and answers the owning paragraph plus the
    /// member ids, in the order the host asked for them.
    ///
    /// Every refusal carries a reason a host can put on a disabled menu item.
    ///
    /// **O(document), ONE walk** for the paragraph resolution plus one layout
    /// read for the page check; never a lookup inside a loop over the ids.
    fn group_members(&self, nodes: &str) -> Result<(NodeId, Vec<NodeId>), String> {
        let names: Vec<String> = serde_json::from_str(nodes)
            .map_err(|_| "expected a JSON array of node ids".to_owned())?;
        let mut ids: Vec<NodeId> = Vec::with_capacity(names.len());
        for name in &names {
            let id = name
                .parse::<NodeId>()
                .map_err(|_| format!("invalid node id {name:?}"))?;
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        if ids.len() < 2 {
            return Err(GROUP_TOO_FEW.to_owned());
        }

        let mut paragraph = None;
        for id in &ids {
            let owner = self
                .paragraph_of_object(*id)
                .ok_or_else(|| GROUP_MEMBER_MISSING.to_owned())?;
            match paragraph {
                None => paragraph = Some(owner),
                Some(first) if first == owner => {}
                Some(_) => {
                    return Err(GROUP_NOT_SIBLING.to_owned());
                }
            }
        }
        let paragraph = paragraph.expect("at least two ids");
        let source = find_paragraph_any(&self.document, paragraph)
            .ok_or_else(|| GROUP_MEMBER_MISSING.to_owned())?;

        for id in &ids {
            if !source
                .inlines
                .iter()
                .any(|inline| top_level_object_id(inline) == Some(*id))
            {
                return Err(GROUP_IN_WRAPPER.to_owned());
            }
            match top_level_anchor(&source.inlines, *id) {
                Some(Some(_)) => {}
                Some(None) => {
                    return Err(GROUP_INLINE.to_owned());
                }
                None => return Err(GROUP_MEMBER_MISSING.to_owned()),
            }
        }

        // A group has ONE anchor, so members laid out on different pages cannot
        // all keep their position. Declining says so; grouping anyway would
        // move one without being asked.
        let mut page = None;
        let mut placed = 0;
        for object in self.object_boxes() {
            if !ids.contains(&object.root) {
                continue;
            }
            placed += 1;
            match page {
                None => page = Some(object.page),
                Some(first) if first == object.page => {}
                Some(_) => {
                    return Err(GROUP_ACROSS_PAGES.to_owned());
                }
            }
        }
        if placed != ids.len() {
            return Err(GROUP_UNPLACED.to_owned());
        }
        Ok((paragraph, ids))
    }

    /// Restacks `object` among its siblings inside a group, when that is what
    /// it is. `Ok(None)` means it is not a group child, so the caller falls
    /// through to the float band.
    ///
    /// A group paints its children in document order, so the index IS the z
    /// index and restacking is a move within one vector.
    ///
    /// **O(document), ONE walk.**
    fn restack_group_child(
        &mut self,
        object: NodeId,
        step: ZStep,
    ) -> Result<Option<EditResult>, String> {
        let Some(paragraph) = self.paragraph_containing_inline_deep(object) else {
            return Ok(None);
        };
        let source = find_paragraph_any(&self.document, paragraph)
            .ok_or_else(|| NOT_AN_OBJECT.to_owned())?;
        let mut inlines = source.inlines.clone();
        if !restack_child_in_inlines(&mut inlines, object, step) {
            return Ok(None);
        }
        self.apply_action_caret_as(
            vec![Operation::SetInlines {
                node: paragraph,
                inlines,
            }],
            Pos::new(object, 0),
            HistoryKind::ObjectMove,
        )
        .map(Some)
    }

    /// Restacks a top-level floating object within its own `@behindDoc` band by
    /// rewriting `wp:anchor@relativeHeight`.
    ///
    /// The band is renumbered to a dense `0..n` sequence in the new order
    /// rather than the target alone being bumped. That is deliberate: keys
    /// arrive from other producers duplicated, sparse, and sometimes absent
    /// (the schema lets a producer omit one), and a single-object bump cannot
    /// express "send to back" when the lowest key is already `0`. Renumbering
    /// makes every one of the four commands exact, and it is bounded by the
    /// number of FLOATING objects — not by the document — so a 1.3M-paragraph
    /// file with three floats costs three.
    ///
    /// **O(document), ONE walk**, then O(floating objects).
    fn restack_float(&mut self, object: NodeId, step: ZStep) -> Result<EditResult, String> {
        let mut floats: Vec<Float> = Vec::new();
        crate::visit_paragraphs_all_surfaces(&self.document, &mut |paragraph| {
            collect_floats(&paragraph.inlines, paragraph.id, &mut floats);
        });
        let Some(position) = floats.iter().position(|float| float.id == object) else {
            return Err(NOT_RESTACKABLE.to_owned());
        };
        let band = floats[position].behind_doc;

        // The band, in the order it currently paints: by key, then by document
        // order — which is exactly how the model documents the tiebreak.
        let mut order: Vec<usize> = (0..floats.len())
            .filter(|i| floats[*i].behind_doc == band)
            .collect();
        order.sort_by_key(|i| (floats[*i].z, *i));
        let Some(at) = order.iter().position(|i| *i == position) else {
            return Err(NOT_RESTACKABLE.to_owned());
        };
        let last = order.len() - 1;
        let target = match step {
            ZStep::Front => last,
            ZStep::Back => 0,
            ZStep::Forward => (at + 1).min(last),
            ZStep::Backward => at.saturating_sub(1),
        };
        if target == at {
            // Already there. Reporting success without an undo entry keeps
            // "Bring to Front" on an object already in front from filling the
            // undo stack with actions that changed nothing.
            return Ok(self.finish_edit(Pos::new(object, 0)));
        }
        let moved = order.remove(at);
        order.insert(target, moved);

        let mut wanted: Vec<(NodeId, NodeId, u32)> = Vec::new();
        for (rank, index) in order.into_iter().enumerate() {
            let float = &floats[index];
            let rank = u32::try_from(rank).map_err(|_| TOO_MANY_FLOATS.to_owned())?;
            if float.z != rank {
                wanted.push((float.paragraph, float.id, rank));
            }
        }
        if wanted.is_empty() {
            return Ok(self.finish_edit(Pos::new(object, 0)));
        }

        // One `SetInlines` per affected paragraph, not per object: several
        // floats routinely share one anchoring paragraph, and two operations
        // rewriting the same paragraph would make the second overwrite the
        // first.
        let mut paragraphs: Vec<NodeId> = Vec::new();
        for (paragraph, _, _) in &wanted {
            if !paragraphs.contains(paragraph) {
                paragraphs.push(*paragraph);
            }
        }
        let mut operations = Vec::with_capacity(paragraphs.len());
        for paragraph in paragraphs {
            let source = find_paragraph_any(&self.document, paragraph)
                .ok_or_else(|| NOT_AN_OBJECT.to_owned())?;
            let mut inlines = source.inlines.clone();
            for (owner, id, rank) in &wanted {
                if *owner == paragraph {
                    set_float_z(&mut inlines, *id, *rank);
                }
            }
            operations.push(Operation::SetInlines {
                node: paragraph,
                inlines,
            });
        }
        self.apply_action_caret_as(operations, Pos::new(object, 0), HistoryKind::ObjectMove)
    }

    /// Applies a rotation or a flip to `node`, wherever it sits.
    ///
    /// **O(document), ONE walk.**
    fn edit_transform(&mut self, node: &str, edit: TransformEdit) -> Result<EditResult, String> {
        let object = node
            .parse::<NodeId>()
            .map_err(|_| "invalid node id".to_owned())?;
        let paragraph = self
            .paragraph_of_object(object)
            .or_else(|| self.paragraph_containing_inline_deep(object))
            .ok_or_else(|| NOT_AN_OBJECT.to_owned())?;
        let source = find_paragraph_any(&self.document, paragraph)
            .ok_or_else(|| NOT_AN_OBJECT.to_owned())?;
        let mut inlines = source.inlines.clone();
        if !edit_transform_in_inlines(&mut inlines, object, edit) {
            return Err(NO_ROTATION.to_owned());
        }
        self.apply_action_caret_as(
            vec![Operation::SetInlines {
                node: paragraph,
                inlines,
            }],
            Pos::new(object, 0),
            HistoryKind::ObjectMove,
        )
    }
}

/// Every top-level floating object in `inlines`, with the paragraph that owns
/// it. Descends through the wrappers a float can sit in.
fn collect_floats(inlines: &[InlineNode], paragraph: NodeId, out: &mut Vec<Float>) {
    for inline in inlines {
        match inline {
            InlineNode::AnchoredDrawing(drawing) => out.push(Float {
                id: drawing.id,
                paragraph,
                z: drawing.relative_height.unwrap_or(0),
                behind_doc: drawing.anchor.behind_doc,
            }),
            InlineNode::TextBox(text_box) => {
                if let Some(anchor) = text_box.anchor.as_ref() {
                    out.push(Float {
                        id: text_box.id,
                        paragraph,
                        z: text_box.relative_height.unwrap_or(0),
                        behind_doc: anchor.behind_doc,
                    });
                }
            }
            InlineNode::Group(group) => {
                if let Some(anchor) = group.anchor.as_ref() {
                    out.push(Float {
                        id: group.id,
                        paragraph,
                        z: group.relative_height.unwrap_or(0),
                        behind_doc: anchor.behind_doc,
                    });
                }
            }
            InlineNode::Hyperlink(link) => collect_floats(&link.inlines, paragraph, out),
            InlineNode::Revision(revision) => collect_floats(&revision.inlines, paragraph, out),
            InlineNode::Sdt(sdt) => collect_floats(&sdt.inlines, paragraph, out),
            _ => {}
        }
    }
}

/// Writes a float's stacking key.
fn set_float_z(inlines: &mut [InlineNode], object: NodeId, z: u32) -> bool {
    with_top_level(inlines, object, &mut |inline| match inline {
        InlineNode::AnchoredDrawing(drawing) => {
            drawing.relative_height = Some(z);
            true
        }
        InlineNode::TextBox(text_box) => {
            text_box.relative_height = Some(z);
            true
        }
        InlineNode::Group(group) => {
            group.relative_height = Some(z);
            true
        }
        _ => false,
    })
    .unwrap_or(false)
}

/// Moves `object` among its siblings inside whichever group holds it.
fn restack_child_in_inlines(inlines: &mut [InlineNode], object: NodeId, step: ZStep) -> bool {
    for inline in inlines.iter_mut() {
        match inline {
            InlineNode::Group(group) => {
                if restack_child_in_group(group, object, step) {
                    return true;
                }
            }
            InlineNode::Hyperlink(link) => {
                if restack_child_in_inlines(&mut link.inlines, object, step) {
                    return true;
                }
            }
            InlineNode::Revision(revision) => {
                if restack_child_in_inlines(&mut revision.inlines, object, step) {
                    return true;
                }
            }
            InlineNode::Sdt(sdt) => {
                if restack_child_in_inlines(&mut sdt.inlines, object, step) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// [`restack_child_in_inlines`] within one group, descending into nested ones.
fn restack_child_in_group(group: &mut WordprocessingGroup, object: NodeId, step: ZStep) -> bool {
    if let Some(at) = group.children.iter().position(|child| match child {
        GroupChild::Picture(picture) => picture.id == object,
        GroupChild::TextBox(text_box) => text_box.id == object,
        GroupChild::Shape(shape) => shape.id == object,
        GroupChild::Group(nested) => nested.id == object,
    }) {
        let last = group.children.len() - 1;
        let target = match step {
            ZStep::Front => last,
            ZStep::Back => 0,
            ZStep::Forward => (at + 1).min(last),
            ZStep::Backward => at.saturating_sub(1),
        };
        if target != at {
            let child = group.children.remove(at);
            group.children.insert(target, child);
        }
        return true;
    }
    for child in &mut group.children {
        if let GroupChild::Group(nested) = child
            && restack_child_in_group(nested, object, step)
        {
            return true;
        }
    }
    false
}

/// What [`add_text_in_inlines`] found.
enum AddText {
    /// The shape was converted; its new body paragraph is the caller's caret.
    Added,
    /// It already had text; this is its first paragraph.
    Already(NodeId),
    /// `object` is not a shape inside a group.
    NotAShape,
}

/// Converts the group shape `object` into a text-bearing shape with one empty
/// paragraph identified by `body`, keeping everything `GroupTextBox` can model.
fn add_text_in_inlines(
    inlines: &mut [InlineNode],
    object: NodeId,
    body: NodeId,
) -> Result<AddText, String> {
    for inline in inlines.iter_mut() {
        let found = match inline {
            InlineNode::Group(group) => add_text_in_group(group, object, body)?,
            InlineNode::Hyperlink(link) => add_text_in_inlines(&mut link.inlines, object, body)?,
            InlineNode::Revision(revision) => {
                add_text_in_inlines(&mut revision.inlines, object, body)?
            }
            InlineNode::Sdt(sdt) => add_text_in_inlines(&mut sdt.inlines, object, body)?,
            _ => AddText::NotAShape,
        };
        if !matches!(found, AddText::NotAShape) {
            return Ok(found);
        }
    }
    Ok(AddText::NotAShape)
}

/// [`add_text_in_inlines`] within one group, descending into nested ones.
fn add_text_in_group(
    group: &mut WordprocessingGroup,
    object: NodeId,
    body: NodeId,
) -> Result<AddText, String> {
    for child in &mut group.children {
        match child {
            GroupChild::TextBox(text_box) if text_box.id == object => {
                let first = text_box.blocks.iter().find_map(|block| match block {
                    BlockNode::Paragraph(paragraph) => Some(paragraph.id),
                    _ => None,
                });
                return Ok(first.map_or(AddText::NotAShape, AddText::Already));
            }
            GroupChild::Shape(shape) if shape.id == object => {
                if shape.path.is_some() {
                    return Err(FREEFORM_NO_TEXT.to_owned());
                }
                *child = GroupChild::TextBox(GroupTextBox {
                    id: shape.id,
                    offset: shape.offset,
                    extent: shape.extent,
                    geometry: shape.geometry,
                    preset: shape.preset.clone(),
                    adjustments: shape.adjustments.clone(),
                    blocks: vec![BlockNode::Paragraph(Paragraph {
                        id: body,
                        properties: ParagraphProperties::default().into(),
                        inlines: Vec::new(),
                    })],
                    fill: shape.fill.clone(),
                    // `a:ln` is the same element on both; only the field name
                    // differs, so nothing about the outline is lost.
                    border: shape.stroke,
                    body_properties: TextBoxBodyProperties::default(),
                    hyperlink: shape.hyperlink.clone(),
                    flip_h: shape.flip_h,
                    flip_v: shape.flip_v,
                    rotation: shape.rotation,
                });
                return Ok(AddText::Added);
            }
            GroupChild::Group(nested) => {
                let found = add_text_in_group(nested, object, body)?;
                if !matches!(found, AddText::NotAShape) {
                    return Ok(found);
                }
            }
            _ => {}
        }
    }
    Ok(AddText::NotAShape)
}

/// Which half of the transform an edit is changing.
#[derive(Clone, Copy, Debug)]
enum TransformEdit {
    Rotation(Option<i32>),
    Flip { flip_h: bool, flip_v: bool },
}

/// A step along the z axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ZStep {
    Front,
    Back,
    Forward,
    Backward,
}

impl ZStep {
    fn parse(token: &str) -> Option<Self> {
        Some(match token {
            "front" => Self::Front,
            "back" => Self::Back,
            "forward" => Self::Forward,
            "backward" => Self::Backward,
            _ => return None,
        })
    }
}

/// `anchor` re-expressed as an absolute page-local position, keeping wrap,
/// exclusion distances and the behind-text flag.
fn anchor_at_page(mut anchor: DrawingAnchor, left_emu: i64, top_emu: i64) -> DrawingAnchor {
    anchor.horizontal = AnchorHorizontal {
        relative_from: HorizontalAnchor::Page,
        position: HorizontalPosition::Offset(left_emu.clamp(-MAX_EMU, MAX_EMU)),
    };
    anchor.vertical = AnchorVertical {
        relative_from: VerticalAnchor::Page,
        position: VerticalPosition::Offset(top_emu.clamp(-MAX_EMU, MAX_EMU)),
    };
    anchor
}

/// Converts one top-level object between inline and floating, reporting whether
/// it changed.
fn convert_anchor_kind(inline: &mut InlineNode, floating: bool) -> Result<bool, String> {
    match inline {
        InlineNode::Drawing(drawing) if floating => {
            let extent = drawing.extent.ok_or_else(|| PICTURE_NO_SIZE.to_owned())?;
            let drawing = drawing.as_ref().clone();
            *inline = InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
                id: drawing.id,
                media: drawing.media,
                extent,
                anchor: default_float_anchor(),
                descr: drawing.descr,
                relative_height: None,
                crop: drawing.crop,
                opacity: drawing.opacity,
                hyperlink: drawing.hyperlink,
                border: drawing.border,
                flip_h: drawing.flip_h,
                flip_v: drawing.flip_v,
                rotation: drawing.rotation,
            }));
            Ok(true)
        }
        InlineNode::AnchoredDrawing(drawing) if !floating => {
            let drawing = drawing.as_ref().clone();
            *inline = InlineNode::Drawing(Box::new(casual_doc_model::v1::Drawing {
                id: drawing.id,
                media: drawing.media,
                extent: Some(drawing.extent),
                descr: drawing.descr,
                crop: drawing.crop,
                opacity: drawing.opacity,
                hyperlink: drawing.hyperlink,
                border: drawing.border,
                flip_h: drawing.flip_h,
                flip_v: drawing.flip_v,
                rotation: drawing.rotation,
            }));
            Ok(true)
        }
        InlineNode::TextBox(text_box) => {
            let had = text_box.anchor.is_some();
            if had == floating {
                return Ok(false);
            }
            text_box.anchor = floating.then(default_float_anchor);
            if !floating {
                text_box.relative_height = None;
            }
            Ok(true)
        }
        InlineNode::Group(group) => {
            let had = group.anchor.is_some();
            if had == floating {
                return Ok(false);
            }
            group.anchor = floating.then(default_float_anchor);
            if !floating {
                group.relative_height = None;
            }
            Ok(true)
        }
        InlineNode::Drawing(_) | InlineNode::AnchoredDrawing(_) => Ok(false),
        _ => Err(NOT_AN_OBJECT.to_owned()),
    }
}

/// The rotation and flips of `object` if it sits in `inlines` at any depth,
/// including inside a group.
fn read_transform_in_inlines(
    inlines: &[InlineNode],
    object: NodeId,
) -> Option<(Option<i32>, bool, bool)> {
    for inline in inlines {
        match inline {
            InlineNode::Drawing(drawing) if drawing.id == object => {
                return Some((drawing.rotation, drawing.flip_h, drawing.flip_v));
            }
            InlineNode::AnchoredDrawing(drawing) if drawing.id == object => {
                return Some((drawing.rotation, drawing.flip_h, drawing.flip_v));
            }
            InlineNode::Group(group) => {
                if group.id == object {
                    return Some((
                        group.transform.rotation,
                        group.transform.flip_h,
                        group.transform.flip_v,
                    ));
                }
                if let Some(found) = read_transform_in_group(group, object) {
                    return Some(found);
                }
            }
            InlineNode::TextBox(text_box) => {
                if let Some(found) = read_transform_in_blocks(&text_box.blocks, object) {
                    return Some(found);
                }
            }
            InlineNode::Hyperlink(link) => {
                if let Some(found) = read_transform_in_inlines(&link.inlines, object) {
                    return Some(found);
                }
            }
            InlineNode::Revision(revision) => {
                if let Some(found) = read_transform_in_inlines(&revision.inlines, object) {
                    return Some(found);
                }
            }
            InlineNode::Sdt(sdt) => {
                if let Some(found) = read_transform_in_inlines(&sdt.inlines, object) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// [`read_transform_in_inlines`] within one group, descending into nested ones.
fn read_transform_in_group(
    group: &WordprocessingGroup,
    object: NodeId,
) -> Option<(Option<i32>, bool, bool)> {
    for child in &group.children {
        match child {
            GroupChild::Picture(picture) if picture.id == object => {
                return Some((picture.rotation, picture.flip_h, picture.flip_v));
            }
            GroupChild::TextBox(text_box) if text_box.id == object => {
                return Some((text_box.rotation, text_box.flip_h, text_box.flip_v));
            }
            GroupChild::Shape(shape) if shape.id == object => {
                return Some((shape.rotation, shape.flip_h, shape.flip_v));
            }
            GroupChild::TextBox(text_box) => {
                if let Some(found) = read_transform_in_blocks(&text_box.blocks, object) {
                    return Some(found);
                }
            }
            GroupChild::Group(nested) => {
                if nested.id == object {
                    return Some((
                        nested.transform.rotation,
                        nested.transform.flip_h,
                        nested.transform.flip_v,
                    ));
                }
                if let Some(found) = read_transform_in_group(nested, object) {
                    return Some(found);
                }
            }
            GroupChild::Picture(_) | GroupChild::Shape(_) => {}
        }
    }
    None
}

/// [`read_transform_in_inlines`] through the block content a text box holds.
fn read_transform_in_blocks(
    blocks: &[casual_doc_model::v1::BlockNode],
    object: NodeId,
) -> Option<(Option<i32>, bool, bool)> {
    use casual_doc_model::v1::BlockNode;
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                if let Some(found) = read_transform_in_inlines(&paragraph.inlines, object) {
                    return Some(found);
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        if let Some(found) = read_transform_in_blocks(&cell.blocks, object) {
                            return Some(found);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(found) = read_transform_in_blocks(&sdt.blocks, object) {
                    return Some(found);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

/// Applies a transform edit to `object` wherever it sits in `inlines`,
/// reporting whether it found an object that models one.
fn edit_transform_in_inlines(
    inlines: &mut [InlineNode],
    object: NodeId,
    edit: TransformEdit,
) -> bool {
    for inline in inlines.iter_mut() {
        match inline {
            InlineNode::Drawing(drawing) if drawing.id == object => {
                apply_transform(
                    edit,
                    &mut drawing.rotation,
                    &mut drawing.flip_h,
                    &mut drawing.flip_v,
                );
                return true;
            }
            InlineNode::AnchoredDrawing(drawing) if drawing.id == object => {
                apply_transform(
                    edit,
                    &mut drawing.rotation,
                    &mut drawing.flip_h,
                    &mut drawing.flip_v,
                );
                return true;
            }
            InlineNode::Group(group) => {
                if group.id == object {
                    let transform = &mut group.transform;
                    apply_transform(
                        edit,
                        &mut transform.rotation,
                        &mut transform.flip_h,
                        &mut transform.flip_v,
                    );
                    return true;
                }
                if edit_transform_in_group(group, object, edit) {
                    return true;
                }
            }
            InlineNode::TextBox(text_box) => {
                if edit_transform_in_blocks(&mut text_box.blocks, object, edit) {
                    return true;
                }
            }
            InlineNode::Hyperlink(link) => {
                if edit_transform_in_inlines(&mut link.inlines, object, edit) {
                    return true;
                }
            }
            InlineNode::Revision(revision) => {
                if edit_transform_in_inlines(&mut revision.inlines, object, edit) {
                    return true;
                }
            }
            InlineNode::Sdt(sdt) => {
                if edit_transform_in_inlines(&mut sdt.inlines, object, edit) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// [`edit_transform_in_inlines`] within one group, descending into nested ones.
fn edit_transform_in_group(
    group: &mut WordprocessingGroup,
    object: NodeId,
    edit: TransformEdit,
) -> bool {
    for child in &mut group.children {
        match child {
            GroupChild::Picture(picture) if picture.id == object => {
                apply_transform(
                    edit,
                    &mut picture.rotation,
                    &mut picture.flip_h,
                    &mut picture.flip_v,
                );
                return true;
            }
            GroupChild::Shape(shape) if shape.id == object => {
                apply_transform(
                    edit,
                    &mut shape.rotation,
                    &mut shape.flip_h,
                    &mut shape.flip_v,
                );
                return true;
            }
            GroupChild::TextBox(text_box) => {
                if text_box.id == object {
                    apply_transform(
                        edit,
                        &mut text_box.rotation,
                        &mut text_box.flip_h,
                        &mut text_box.flip_v,
                    );
                    return true;
                }
                if edit_transform_in_blocks(&mut text_box.blocks, object, edit) {
                    return true;
                }
            }
            GroupChild::Group(nested) => {
                if nested.id == object {
                    let transform = &mut nested.transform;
                    apply_transform(
                        edit,
                        &mut transform.rotation,
                        &mut transform.flip_h,
                        &mut transform.flip_v,
                    );
                    return true;
                }
                if edit_transform_in_group(nested, object, edit) {
                    return true;
                }
            }
            GroupChild::Picture(_) | GroupChild::Shape(_) => {}
        }
    }
    false
}

/// [`edit_transform_in_inlines`] through the block content a text box holds.
fn edit_transform_in_blocks(
    blocks: &mut [casual_doc_model::v1::BlockNode],
    object: NodeId,
    edit: TransformEdit,
) -> bool {
    use casual_doc_model::v1::BlockNode;
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Paragraph(paragraph) => {
                if edit_transform_in_inlines(&mut paragraph.inlines, object, edit) {
                    return true;
                }
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if edit_transform_in_blocks(&mut cell.blocks, object, edit) {
                            return true;
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if edit_transform_in_blocks(&mut sdt.blocks, object, edit) {
                    return true;
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    false
}

/// Writes one transform edit onto the three fields every transformable node
/// carries under different names.
fn apply_transform(
    edit: TransformEdit,
    rotation: &mut Option<i32>,
    flip_h: &mut bool,
    flip_v: &mut bool,
) {
    match edit {
        TransformEdit::Rotation(value) => *rotation = value,
        TransformEdit::Flip {
            flip_h: h,
            flip_v: v,
        } => {
            *flip_h = h;
            *flip_v = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_document;
    use casual_doc_layout::block::BlockFragment;
    use casual_doc_model::v1::BlockNode;

    /// A real producer's package, so a round trip is measured against Word's
    /// idea of a `wp:anchor` rather than only our own writer's.
    const RICH_DOCX: &[u8] = include_bytes!("../../../fixtures/corpus/real-producer-rich.docx");

    /// The first body paragraph, which is where the fixtures anchor their
    /// objects.
    fn first_paragraph(document: &WasmDocument) -> String {
        document
            .document
            .body()
            .iter()
            .find_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph.id.to_string()),
                _ => None,
            })
            .expect("a body paragraph")
    }

    /// Two floating text boxes on the first paragraph, placed at known
    /// page-local positions.
    fn two_boxes(document: &mut WasmDocument) -> (String, String, String) {
        let paragraph = first_paragraph(document);
        let first = document
            .insert_text_box(&paragraph, 0)
            .expect("insert the first box")
            .node;
        let second = document
            .insert_text_box(&paragraph, 0)
            .expect("insert the second box")
            .node;
        // The insert leaves the caret in the box's paragraph; the object id is
        // the box that paragraph belongs to.
        let first = owner_box(document, &first);
        let second = owner_box(document, &second);
        document
            .set_object_anchor_position(&second, 914_400.0, 914_400.0)
            .expect("place the first box");
        document
            .set_object_anchor_position(&first, 2_743_200.0, 2_743_200.0)
            .expect("place the second box");
        // Both boxes were inserted at offset 0, so the SECOND insert sits first
        // in document order — and document order is the z tiebreak and the
        // group child order, so the names are the document's, not the calls'.
        (paragraph, second, first)
    }

    /// The floating text box whose body holds the paragraph `node`.
    fn owner_box(document: &WasmDocument, node: &str) -> String {
        let wanted: NodeId = node.parse().expect("a node id");
        fn find(inlines: &[InlineNode], wanted: NodeId) -> Option<NodeId> {
            for inline in inlines {
                if let InlineNode::TextBox(text_box) = inline {
                    let holds = text_box.blocks.iter().any(|block| {
                        matches!(block, BlockNode::Paragraph(paragraph) if paragraph.id == wanted)
                    });
                    if holds {
                        return Some(text_box.id);
                    }
                }
            }
            None
        }
        for block in document.document.body() {
            if let BlockNode::Paragraph(paragraph) = block
                && let Some(found) = find(&paragraph.inlines, wanted)
            {
                return found.to_string();
            }
        }
        panic!("no text box owns {node}");
    }

    /// The grouped text box whose id is `wanted`.
    fn grouped_text_box<'a>(document: &'a WasmDocument, wanted: &str) -> Option<&'a GroupTextBox> {
        fn walk<'a>(group: &'a WordprocessingGroup, wanted: &str) -> Option<&'a GroupTextBox> {
            for child in &group.children {
                match child {
                    GroupChild::TextBox(text_box) if text_box.id.to_string() == wanted => {
                        return Some(text_box);
                    }
                    GroupChild::Group(nested) => {
                        if let Some(found) = walk(nested, wanted) {
                            return Some(found);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        for block in document.document.body() {
            if let BlockNode::Paragraph(paragraph) = block {
                for inline in &paragraph.inlines {
                    if let InlineNode::Group(group) = inline
                        && let Some(found) = walk(group, wanted)
                    {
                        return Some(found);
                    }
                }
            }
        }
        None
    }

    /// The ids of every grouped shape in the body carrying `geometry`.
    fn shapes_with(
        document: &WasmDocument,
        geometry: casual_doc_model::v1::ShapeGeometry,
    ) -> Vec<String> {
        fn walk(
            group: &WordprocessingGroup,
            geometry: casual_doc_model::v1::ShapeGeometry,
            out: &mut Vec<String>,
        ) {
            for child in &group.children {
                match child {
                    GroupChild::Shape(shape) if shape.geometry == geometry => {
                        out.push(shape.id.to_string());
                    }
                    GroupChild::Group(nested) => walk(nested, geometry, out),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        for block in document.document.body() {
            if let BlockNode::Paragraph(paragraph) = block {
                for inline in &paragraph.inlines {
                    if let InlineNode::Group(group) = inline {
                        walk(group, geometry, &mut out);
                    }
                }
            }
        }
        out
    }

    /// The single top-level group in the body, if there is one.
    fn only_group(document: &WasmDocument) -> Option<&WordprocessingGroup> {
        for block in document.document.body() {
            if let BlockNode::Paragraph(paragraph) = block {
                for inline in &paragraph.inlines {
                    if let InlineNode::Group(group) = inline {
                        return Some(group);
                    }
                }
            }
        }
        None
    }

    /// Word's position dialog is not "an offset from the page": it is a
    /// reference plus a placement, per axis. Every combination the schema
    /// allows must survive being written and read back, or a host cannot draw
    /// the dialog at all.
    #[test]
    fn a_position_is_expressed_in_the_ooxml_model_not_a_page_offset() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let (_, object, _) = two_boxes(&mut document);

        document
            .set_object_position_inner(
                &object,
                r#"{"horizontal":{"relativeFrom":"margin","align":"center"},
                    "vertical":{"relativeFrom":"paragraph","offsetEmu":457200}}"#,
            )
            .expect("centre it on the margin");

        let read: serde_json::Value = serde_json::from_str(&document.object_position(&object))
            .expect("the position reads back as JSON");
        assert_eq!(read["floating"], serde_json::json!(true));
        assert_eq!(read["horizontal"]["relativeFrom"], "margin");
        assert_eq!(read["horizontal"]["align"], "center");
        assert!(
            read["horizontal"]["offsetEmu"].is_null(),
            "an aligned axis has no offset: {read}"
        );
        assert_eq!(read["vertical"]["relativeFrom"], "paragraph");
        assert_eq!(read["vertical"]["offsetEmu"], serde_json::json!(457_200.0));

        // An omitted axis is LEFT ALONE. A dialog that changes only the wrap
        // must not silently restate the position it happens to be showing.
        document
            .set_object_position_inner(&object, r#"{"wrap":"tight"}"#)
            .expect("change only the wrap");
        let read: serde_json::Value =
            serde_json::from_str(&document.object_position(&object)).expect("JSON");
        assert_eq!(read["wrap"], "tight");
        assert_eq!(read["horizontal"]["align"], "center");
        assert_eq!(read["horizontal"]["relativeFrom"], "margin");

        // And it survives the writer: a margin-centred box reopens margin-centred.
        let bytes = document.export_docx().expect("export");
        let reopened = open_document(&bytes).expect("reopen");
        let anchors: Vec<AnchorHorizontal> = reopened
            .document
            .body()
            .iter()
            .filter_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph),
                _ => None,
            })
            .flat_map(|paragraph| paragraph.inlines.iter())
            .filter_map(|inline| match inline {
                InlineNode::TextBox(text_box) => {
                    text_box.anchor.as_ref().map(|anchor| anchor.horizontal)
                }
                _ => None,
            })
            .collect();
        assert!(
            anchors.contains(&AnchorHorizontal {
                relative_from: HorizontalAnchor::Margin,
                position: HorizontalPosition::Align(HorizontalAlign::Center),
            }),
            "the margin-relative alignment survived the DOCX round trip: {anchors:?}"
        );
    }

    /// A token this build does not know is an ERROR, not a value quietly
    /// ignored — the difference between a dialog that refuses and a document
    /// that silently disagrees with what the user chose.
    #[test]
    fn an_unknown_position_token_is_refused_rather_than_ignored() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let (_, object, _) = two_boxes(&mut document);

        let error = document
            .set_object_position_inner(&object, r#"{"horizontal":{"relativeFrom":"gutter"}}"#)
            .expect_err("an invented reference is refused");
        assert!(error.contains("gutter"), "the refusal names it: {error}");

        let error = document
            .set_object_position_inner(&object, r#"{"vertical":{"align":"top","offsetEmu":100}}"#)
            .expect_err("an axis is an alignment OR an offset");
        assert!(error.contains("not both"), "{error}");

        let error = document
            .set_object_position_inner(&object, r#"{"wrapDistance":{}}"#)
            .expect_err("an unknown key is refused");
        assert!(error.contains("position"), "{error}");
    }

    /// Word's first position choice is "In line with text", and the conversion
    /// is a different NODE, not a flag — `wp:inline` versus `wp:anchor`.
    #[test]
    fn an_object_converts_between_inline_and_floating() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let (_, object, _) = two_boxes(&mut document);

        document
            .set_object_anchor_kind_inner(&object, "inline")
            .expect("make it inline");
        let read: serde_json::Value =
            serde_json::from_str(&document.object_position(&object)).expect("JSON");
        assert_eq!(read["floating"], serde_json::json!(false));
        assert!(
            read["horizontal"].is_null(),
            "an inline object reports no anchor: {read}"
        );

        // Positioning an inline object refuses, and says what to do instead.
        //
        // This asserted `error.contains("setObjectAnchorKind")` — it pinned the
        // refusal to naming a JS METHOD, which is the one thing a refusal must
        // never show a reader (`webapp/tests/edit_errors.test.mjs`: "no engine
        // error name ever reaches the user"). What matters is that the sentence
        // names the gesture and carries a code a host can translate.
        let error = document
            .set_object_position_inner(&object, r#"{"wrap":"square"}"#)
            .expect_err("an inline object has no anchor");
        let (sentence, code) = casual_doc_edit::refusal::split(&error);
        assert_eq!(code, Some("object.position-not-floating"), "{error}");
        assert!(
            sentence.contains("wrap"),
            "it names the gesture: {sentence}"
        );
        assert!(
            !sentence.contains("setObject"),
            "and not the method: {sentence}"
        );

        document
            .set_object_anchor_kind_inner(&object, "floating")
            .expect("float it again");
        let read: serde_json::Value =
            serde_json::from_str(&document.object_position(&object)).expect("JSON");
        assert_eq!(read["floating"], serde_json::json!(true));
        assert_eq!(read["wrap"], "square");
    }

    /// The guarantee grouping has to make: nothing moves. The children come out
    /// of the group at the page positions they went in at.
    #[test]
    fn grouping_keeps_every_child_where_it_was_and_ungrouping_puts_it_back() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let (_, first, second) = two_boxes(&mut document);
        let before: Vec<Vec<i32>> =
            vec![document.object_rect(&first), document.object_rect(&second)];
        assert!(
            before.iter().all(|rect| rect.len() == 5),
            "both boxes are placed before grouping: {before:?}"
        );

        assert_eq!(
            document.can_group_objects(&format!("[\"{first}\",\"{second}\"]")),
            "{\"can\":true}"
        );
        document
            .group_objects_inner(&format!("[\"{first}\",\"{second}\"]"))
            .expect("group them");

        let group = only_group(&document).expect("a group replaced the two boxes");
        assert_eq!(group.children.len(), 2, "both members became children");
        let group_id = group.id.to_string();

        // The children are still painted where they were.
        let after: Vec<Vec<i32>> =
            vec![document.object_rect(&first), document.object_rect(&second)];
        assert_eq!(
            after, before,
            "grouping moved a child: {before:?} became {after:?}"
        );

        // It survives the writer.
        let bytes = document.export_docx().expect("export");
        let reopened = open_document(&bytes).expect("reopen");
        let group = only_group(&reopened).expect("the group came back from the DOCX");
        assert_eq!(
            group.children.len(),
            2,
            "both children survived the round trip"
        );

        document.ungroup_object_inner(&group_id).expect("ungroup");
        assert!(
            only_group(&document).is_none(),
            "ungrouping removed the group"
        );
        let unwrapped: Vec<Vec<i32>> =
            vec![document.object_rect(&first), document.object_rect(&second)];
        assert_eq!(
            unwrapped, before,
            "ungrouping moved a child: {before:?} became {unwrapped:?}"
        );
    }

    /// Moving the group moves its children — the property that makes it a
    /// group rather than two objects drawn near each other.
    #[test]
    fn moving_a_group_moves_its_children() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let (_, first, second) = two_boxes(&mut document);
        document
            .group_objects_inner(&format!("[\"{first}\",\"{second}\"]"))
            .expect("group them");
        let group = only_group(&document).expect("a group").id.to_string();

        let before_first = document.object_rect(&first);
        let before_second = document.object_rect(&second);
        let before_group = document.object_rect(&group);
        document
            .set_object_anchor_position(&group, 2_286_000.0, 1_828_800.0)
            .expect("move the group");
        let after_group = document.object_rect(&group);
        let dx = after_group[1] - before_group[1];
        let dy = after_group[2] - before_group[2];
        assert!(dx != 0 || dy != 0, "the group actually moved");

        for (object, before) in [(&first, &before_first), (&second, &before_second)] {
            let after = document.object_rect(object);
            assert_eq!(
                (after[1] - before[1], after[2] - before[2]),
                (dx, dy),
                "the child moved with the group"
            );
        }
    }

    /// A group that has been RESIZED has a child coordinate space that is not
    /// the identity, and ungrouping has to apply it. The group root's placed
    /// rect is the union of its CHILDREN, not its anchor origin, so a mapping
    /// written from the root's rect drifts the moment the transform is not the
    /// identity — which is exactly what a resized group is.
    #[test]
    fn ungrouping_a_resized_group_puts_its_children_where_they_were_drawn() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let (_, first, second) = two_boxes(&mut document);
        document
            .group_objects_inner(&format!("[\"{first}\",\"{second}\"]"))
            .expect("group them");
        let group = only_group(&document).expect("a group").id.to_string();

        // Halve it. `resizeObject` scales the transform, so the child space is
        // now twice the parent box — the non-identity case.
        let rect = document.object_rect(&group);
        let (left, top) = (f64::from(rect[1]) * 635.0, f64::from(rect[2]) * 635.0);
        let (width, height) = (
            f64::from(rect[3]) * 635.0 / 2.0,
            f64::from(rect[4]) * 635.0 / 2.0,
        );
        document
            .resize_object(&group, left, top, width, height)
            .expect("halve the group");
        let before = [document.object_rect(&first), document.object_rect(&second)];
        assert!(
            before.iter().all(|rect| rect.len() == 5),
            "both children are still placed: {before:?}"
        );

        document.ungroup_object_inner(&group).expect("ungroup");
        let after = [document.object_rect(&first), document.object_rect(&second)];
        // Within a twip per edge: the positions come from the placed rects,
        // which are twip-quantised.
        for (before, after) in before.iter().zip(after.iter()) {
            assert_eq!(after.len(), 5, "the child is still placed: {after:?}");
            for axis in 1..5 {
                assert!(
                    (after[axis] - before[axis]).abs() <= 1,
                    "ungrouping a half-scale group moved or resized a child: \
                     {before:?} became {after:?}"
                );
            }
        }
    }

    /// A group's declared box and the box its content actually occupies are
    /// two different rectangles — `a:chOff` is arbitrary, and a child dragged
    /// out of the declared box makes them disagree. Both directions have to
    /// translate by the CONTENT corner, not by the declared one, or every
    /// member of an imported group shifts the moment it is grouped again.
    #[test]
    fn a_group_whose_content_sits_outside_its_declared_box_still_does_not_move() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let paragraph = first_paragraph(&document);
        let (_, first, second) = two_boxes(&mut document);
        document
            .group_objects_inner(&format!("[\"{first}\",\"{second}\"]"))
            .expect("group the inner pair");
        let inner = only_group(&document)
            .expect("the inner group")
            .id
            .to_string();

        // Push the child that DEFINES the group's content corner up and to the
        // left, so the content corner moves away from `a:chOff`. Nudging any
        // other child would leave the minimum where it was and the guard would
        // pass without ever creating the condition it is about.
        document
            .move_group_child_by(&first, -457_200.0, -457_200.0)
            .expect("nudge a child outside the declared box");
        assert_eq!(
            document.object_rect(&first)[1..3],
            [720, 720],
            "the nudge really moved the content corner"
        );

        let third = document
            .insert_text_box(&paragraph, 0)
            .expect("a third box")
            .node;
        let third = owner_box(&document, &third);
        document
            .set_object_anchor_position(&third, 4_572_000.0, 914_400.0)
            .expect("place it");

        let before = [
            document.object_rect(&first),
            document.object_rect(&second),
            document.object_rect(&third),
        ];
        document
            .group_objects_inner(&format!("[\"{inner}\",\"{third}\"]"))
            .expect("group the group with the third box");
        let grouped = [
            document.object_rect(&first),
            document.object_rect(&second),
            document.object_rect(&third),
        ];
        for (before, after) in before.iter().zip(grouped.iter()) {
            assert_eq!(
                before, after,
                "grouping a group moved its content: {before:?} became {after:?}"
            );
        }

        let outer = only_group(&document)
            .expect("the outer group")
            .id
            .to_string();
        document.ungroup_object_inner(&outer).expect("ungroup");
        let ungrouped = [
            document.object_rect(&first),
            document.object_rect(&second),
            document.object_rect(&third),
        ];
        for (before, after) in before.iter().zip(ungrouped.iter()) {
            assert_eq!(
                before, after,
                "ungrouping moved a group's content: {before:?} became {after:?}"
            );
        }
    }

    /// Word refuses to group an in-line object, and so must we — with a reason,
    /// because a host ships a DISABLED command carrying why, never a dead one.
    #[test]
    fn grouping_refuses_with_a_reason_a_host_can_show() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let (_, first, second) = two_boxes(&mut document);
        document
            .set_object_anchor_kind_inner(&first, "inline")
            .expect("make one inline");

        let answer: serde_json::Value = serde_json::from_str(
            &document.can_group_objects(&format!("[\"{first}\",\"{second}\"]")),
        )
        .expect("JSON");
        assert_eq!(answer["can"], serde_json::json!(false));
        assert!(
            answer["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("in line with text")),
            "the reason names the problem: {answer}"
        );

        let answer: serde_json::Value =
            serde_json::from_str(&document.can_group_objects(&format!("[\"{second}\"]")))
                .expect("JSON");
        assert_eq!(answer["can"], serde_json::json!(false));
        assert!(
            answer["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("two")),
            "one object is not a group: {answer}"
        );
    }

    /// A group inside a group peels one layer per ungroup, the way Word's does.
    #[test]
    fn a_nested_group_survives_and_peels_one_layer_at_a_time() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let paragraph = first_paragraph(&document);
        let (_, first, second) = two_boxes(&mut document);
        document
            .group_objects_inner(&format!("[\"{first}\",\"{second}\"]"))
            .expect("group the inner pair");
        let inner = only_group(&document)
            .expect("the inner group")
            .id
            .to_string();

        let third = document
            .insert_text_box(&paragraph, 0)
            .expect("a third box")
            .node;
        let third = owner_box(&document, &third);
        document
            .set_object_anchor_position(&third, 4_572_000.0, 914_400.0)
            .expect("place it");
        document
            .group_objects_inner(&format!("[\"{inner}\",\"{third}\"]"))
            .expect("group the group with the third box");

        let outer = only_group(&document).expect("an outer group");
        assert_eq!(outer.children.len(), 2);
        assert!(
            outer.children.iter().any(
                |child| matches!(child, GroupChild::Group(nested) if nested.id.to_string() == inner)
            ),
            "the inner group became a nested child, not two loose members"
        );
        let outer_id = outer.id.to_string();

        let bytes = document.export_docx().expect("export");
        let reopened = open_document(&bytes).expect("reopen");
        let group = only_group(&reopened).expect("the nest came back");
        assert!(
            group
                .children
                .iter()
                .any(|child| matches!(child, GroupChild::Group(_))),
            "the nesting survived the DOCX round trip"
        );

        document
            .ungroup_object_inner(&outer_id)
            .expect("ungroup once");
        let group = only_group(&document).expect("the inner group is still a group");
        assert_eq!(
            group.id.to_string(),
            inner,
            "one ungroup peeled exactly one layer"
        );
    }

    /// A shape has no top-level inline of its own, and a top-level text box
    /// models no rotation. Ungrouping either must not therefore LOSE anything:
    /// both come back wrapped in a group of one.
    #[test]
    fn ungrouping_a_shape_loses_neither_the_shape_nor_a_rotation() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let paragraph = first_paragraph(&document);
        document
            .insert_shape(&paragraph, 0, "ellipse")
            .expect("insert a shape");
        let shape_group = only_group(&document)
            .expect("a group of one")
            .id
            .to_string();
        document
            .set_object_anchor_position(&shape_group, 914_400.0, 914_400.0)
            .expect("place it");
        let (_, text_box, _) = two_boxes(&mut document);
        document
            .group_objects_inner(&format!("[\"{shape_group}\",\"{text_box}\"]"))
            .expect("group the shape with a box");
        let outer = only_group(&document)
            .expect("the outer group")
            .id
            .to_string();

        // Rotate the grouped text box: only `GroupTextBox` models one.
        let inner_box = text_box.clone();
        document
            .set_object_rotation_inner(&inner_box, Some(30.0))
            .expect("rotate the grouped box");

        document.ungroup_object_inner(&outer).expect("ungroup");

        let transform = document.object_transform(&inner_box);
        assert!(
            !transform.is_empty(),
            "the ungrouped box still models a rotation — an empty answer means it \
             came back as a plain top-level text box, which models none"
        );
        let read: serde_json::Value = serde_json::from_str(&transform).expect("JSON");
        assert_eq!(
            read["rotationDegrees"],
            serde_json::json!(30.0),
            "the rotation survived ungrouping instead of being straightened"
        );
        let mut ellipses = 0;
        let mut visit = |group: &WordprocessingGroup| {
            for child in &group.children {
                if let GroupChild::Shape(shape) = child
                    && shape.geometry == casual_doc_model::v1::ShapeGeometry::Ellipse
                {
                    ellipses += 1;
                }
            }
        };
        for block in document.document.body() {
            if let BlockNode::Paragraph(paragraph) = block {
                for inline in &paragraph.inlines {
                    if let InlineNode::Group(group) = inline {
                        visit(group);
                    }
                }
            }
        }
        assert_eq!(
            ellipses, 1,
            "the ellipse came back as a shape, not as a box"
        );
    }

    /// A shape inside a group is positioned by its parent. Saying so is what
    /// lets a host disable the position dialog WITH A REASON rather than on an
    /// empty answer it cannot tell apart from "no such object".
    #[test]
    fn a_group_child_reports_that_its_parent_positions_it() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let paragraph = first_paragraph(&document);
        document
            .insert_shape(&paragraph, 0, "rectangle")
            .expect("insert a shape");
        let group = only_group(&document).expect("a group of one");
        let shape = match group.children.first().expect("one child") {
            GroupChild::Shape(shape) => shape.id.to_string(),
            other => panic!("expected a shape, got {other:?}"),
        };

        let read: serde_json::Value =
            serde_json::from_str(&document.object_position(&shape)).expect("JSON");
        assert_eq!(read["groupChild"], serde_json::json!(true));
        assert_eq!(read["floating"], serde_json::json!(false));

        // And a node that is not an object at all still answers nothing, so
        // the two cases stay distinguishable.
        assert_eq!(document.object_position(&paragraph), "");
    }

    /// Every preset the MODEL knows must be insertable from the host and must
    /// reach layout. "Modeled is not shipped" is the rule this exists for: the
    /// shape set grew by fifteen presets in the model, import, export and
    /// layout, and a gallery built from a separate hand-written list would
    /// have offered seven of them.
    ///
    /// Exhaustive over `ShapeGeometry::TYPED`, so a preset added later without
    /// a token — or with a token `insertShape` will not take — fails here
    /// rather than quietly never appearing in the product.
    #[test]
    fn every_modeled_preset_is_insertable_and_reaches_layout() {
        use casual_doc_model::v1::ShapeGeometry;

        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let paragraph = first_paragraph(&document);

        for geometry in ShapeGeometry::TYPED {
            let token = geometry
                .preset_token()
                .unwrap_or_else(|| panic!("{geometry:?} has no preset token"));
            // A `JsValue` calls a wasm-bindgen import when it is dropped OR
            // formatted, either of which panics on a native target and would
            // hide the token that actually failed — so the result is inspected
            // and then leaked.
            let inserted = document.insert_shape(&paragraph, 0, token);
            let refused = inserted.is_err();
            core::mem::forget(inserted);
            assert!(!refused, "insertShape refused the modeled preset {token:?}");
            // Each preset is inserted once, so the shape carrying this
            // geometry is the one just added.
            let shape = shapes_with(&document, geometry);
            assert_eq!(
                shape.len(),
                1,
                "{token:?} resolved to the wrong variant, or inserted nothing"
            );
            assert_eq!(
                document.object_rect(&shape[0]).len(),
                5,
                "{token:?} was modeled but never placed on a page"
            );
        }

        // A token no variant claims is refused, not silently drawn as a
        // rectangle — which a caller would see and take for a rendering bug.
        // The message is not inspected here: a `JsValue`'s `Debug` calls a
        // wasm-bindgen import, which panics on a native test.
        // A token no variant claims resolves to nothing, so `insertShape`
        // refuses instead of silently drawing a rectangle — which a caller
        // would see and take for a rendering bug. Asserted at the decision
        // point rather than through the command, because BUILDING the
        // command's error calls a wasm-bindgen import that panics natively.
        assert!(
            ShapeGeometry::from_preset_token("bentArrow").is_none(),
            "a curved preset this build cannot draw stays untyped, not approximated"
        );
        assert_eq!(
            ShapeGeometry::from_preset_token("straightConnector1"),
            Some(ShapeGeometry::Line),
            "the connector spelling of a line resolves to a line"
        );
    }

    /// A star with words in it is one OOXML element, not a text box parked on
    /// top of a shape. Adding text must keep the geometry — and the geometry
    /// must still PAINT, and must still be there after a DOCX round trip, or
    /// "a shape you can type in" is a text box wearing a shape's id.
    #[test]
    fn adding_text_to_a_shape_keeps_the_shape() {
        use casual_doc_model::v1::ShapeGeometry;

        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let paragraph = first_paragraph(&document);
        document
            .insert_shape(&paragraph, 0, "star5")
            .expect("insert a star");
        let shape = shapes_with(&document, ShapeGeometry::Star5);
        assert_eq!(shape.len(), 1, "one star");
        let shape = shape[0].clone();

        let result = document
            .add_text_to_shape_inner(&shape)
            .expect("give the star a text body");
        // The caret is inside the shape, so the host focuses it and types.
        document
            .insert_text(&result.node, result.offset, "INSIDE".to_owned())
            .expect("type into the star");

        let text_shape = grouped_text_box(&document, &shape).expect("the star now holds text");
        assert_eq!(
            text_shape.geometry,
            ShapeGeometry::Star5,
            "adding text kept the star geometry instead of flattening it"
        );

        // Still painted, and still painted as a star: layout resolves it
        // through the same geometry mapping a text-free shape uses.
        assert_eq!(
            document.object_rect(&shape).len(),
            5,
            "the text-bearing star is still placed on a page"
        );

        // And it is still a star after a DOCX round trip, with its words.
        let bytes = document.export_docx().expect("export");
        let reopened = open_document(&bytes).expect("reopen");
        let mut found = 0;
        for block in reopened.document.body() {
            if let BlockNode::Paragraph(paragraph) = block {
                for inline in &paragraph.inlines {
                    if let InlineNode::Group(group) = inline {
                        for child in &group.children {
                            if let GroupChild::TextBox(text_box) = child
                                && text_box.geometry == ShapeGeometry::Star5
                            {
                                found += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(
            found, 1,
            "the text-bearing star survived the DOCX round trip as a star"
        );
    }

    /// A freeform shape has a custom path `GroupTextBox` cannot model, so
    /// giving it text is REFUSED rather than silently flattening it.
    #[test]
    fn a_freeform_shape_refuses_text_instead_of_losing_its_path() {
        use casual_doc_model::v1::{ShapePath, ShapePathCommand};

        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let paragraph = first_paragraph(&document);
        document
            .insert_shape(&paragraph, 0, "rect")
            .expect("insert a shape");
        let shape = only_group(&document)
            .and_then(|group| match group.children.first() {
                Some(GroupChild::Shape(shape)) => Some(shape.id),
                _ => None,
            })
            .expect("a shape");

        // Give it a custom geometry, as an imported freeform carries.
        for block in document.document.body_mut() {
            if let BlockNode::Paragraph(paragraph) = block {
                for inline in &mut paragraph.inlines {
                    if let InlineNode::Group(group) = inline {
                        for child in &mut group.children {
                            if let GroupChild::Shape(target) = child
                                && target.id == shape
                            {
                                target.path = Some(ShapePath {
                                    width_emu: 0,
                                    height_emu: 0,
                                    commands: vec![
                                        ShapePathCommand::MoveTo {
                                            point: PointEmu { x_emu: 0, y_emu: 0 },
                                        },
                                        ShapePathCommand::Close,
                                    ],
                                });
                            }
                        }
                    }
                }
            }
        }

        let error = document
            .add_text_to_shape_inner(&shape.to_string())
            .expect_err("a freeform refuses text");
        assert!(
            error.contains("freeform"),
            "the refusal says why, so a host can disable the command with a reason: {error}"
        );
    }

    /// Bring to front / send to back must be exact even when the incoming keys
    /// are absent or already `0` — which is precisely where a "bump the target"
    /// implementation silently does nothing.
    #[test]
    fn z_order_is_exact_even_when_the_lowest_key_is_already_zero() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let (_, first, second) = two_boxes(&mut document);

        let z = |document: &WasmDocument, object: &str| -> Option<u64> {
            serde_json::from_str::<serde_json::Value>(&document.object_position(object))
                .ok()
                .and_then(|value| value["zOrder"].as_u64())
        };

        document
            .set_object_z_order_inner(&first, "front")
            .expect("bring the first box to the front");
        assert!(
            z(&document, &first) > z(&document, &second),
            "front put it above: {:?} vs {:?}",
            z(&document, &first),
            z(&document, &second)
        );

        document
            .set_object_z_order_inner(&first, "back")
            .expect("send it back again");
        assert!(
            z(&document, &first) < z(&document, &second),
            "back put it below even though the lowest key was 0: {:?} vs {:?}",
            z(&document, &first),
            z(&document, &second)
        );
        assert_eq!(z(&document, &first), Some(0), "the band is dense from 0");

        document
            .set_object_z_order_inner(&first, "forward")
            .expect("one step forward");
        assert!(
            z(&document, &first) > z(&document, &second),
            "one step moved it"
        );

        let error = document
            .set_object_z_order_inner(&first, "up")
            .expect_err("an invented token is refused");
        assert!(error.contains("up"), "{error}");
    }

    /// A group paints its children in document order, so restacking one inside
    /// a group is a move within that vector — a different mechanism from the
    /// float band, and it has to actually be wired.
    #[test]
    fn a_group_child_restacks_by_its_index() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let (_, first, second) = two_boxes(&mut document);
        document
            .group_objects_inner(&format!("[\"{first}\",\"{second}\"]"))
            .expect("group them");

        let index_of = |document: &WasmDocument, wanted: &str| -> usize {
            let group = only_group(document).expect("a group");
            group
                .children
                .iter()
                .position(|child| match child {
                    GroupChild::TextBox(text_box) => text_box.id.to_string() == wanted,
                    _ => false,
                })
                .expect("the child is in the group")
        };
        let before = index_of(&document, &first);
        document
            .set_object_z_order_inner(&first, "front")
            .expect("bring the child to the front");
        let after = index_of(&document, &first);
        assert_eq!(after, 1, "the child moved to the end, which paints last");
        assert_ne!(before, after, "it actually moved");

        document
            .set_object_z_order_inner(&first, "back")
            .expect("send it back");
        assert_eq!(index_of(&document, &first), 0, "and back to the start");
    }

    /// Rotation is stored in OOXML's 60000ths and normalised, so a handle
    /// dragged round twice does not author a value no other producer reads back
    /// the same way.
    #[test]
    fn a_rotation_round_trips_through_the_docx_in_sixty_thousandths() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let paragraph = first_paragraph(&document);
        document
            .insert_shape(&paragraph, 0, "rectangle")
            .expect("insert a shape");
        let group = only_group(&document).expect("a group of one");
        let shape = match group.children.first().expect("one child") {
            GroupChild::Shape(shape) => shape.id.to_string(),
            other => panic!("expected a shape, got {other:?}"),
        };

        // Modeled is not shipped: the shape must have reached LAYOUT, because
        // `objectRect` reports a placed anchor, not a model node.
        assert_eq!(
            document.object_rect(&shape).len(),
            5,
            "the inserted shape was placed on a page, not merely modeled"
        );

        document
            .set_object_rotation_inner(&shape, Some(405.0))
            .expect("rotate it a turn and a bit");
        let read: serde_json::Value =
            serde_json::from_str(&document.object_transform(&shape)).expect("JSON");
        assert_eq!(
            read["rotationDegrees"],
            serde_json::json!(45.0),
            "405° normalised into [0, 360): {read}"
        );

        document
            .set_object_flip(&shape, true, false)
            .expect("flip it");
        let bytes = document.export_docx().expect("export");
        let reopened = open_document(&bytes).expect("reopen");
        let group = only_group(&reopened).expect("the shape came back");
        let GroupChild::Shape(shape) = group.children.first().expect("one child") else {
            panic!("expected a shape");
        };
        assert_eq!(
            shape.rotation,
            Some(45 * 60_000),
            "the rotation survived as OOXML 60000ths"
        );
        assert!(shape.flip_h, "and so did the flip");
        assert!(!shape.flip_v);
    }

    /// The blocker this whole change exists to remove. A rotated object used to
    /// advertise ZERO resize handles, so `can_resize` went false and every grip
    /// and the size chrome vanished the moment anything turned — including on a
    /// document that merely IMPORTED a rotated picture, with no way back but
    /// undo. Word, Google Docs and ONLYOFFICE all resize a rotated object.
    ///
    /// Asserts the GUARANTEE and the DOCUMENT: eight grips plus the rotation
    /// grip are offered at 30 degrees, and a resize at that angle actually
    /// changes the stored extent.
    #[test]
    fn a_rotated_object_keeps_its_eight_grips_and_can_still_be_resized() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let paragraph = first_paragraph(&document);
        document
            .insert_shape(&paragraph, 0, "rectangle")
            .expect("insert a shape");
        let group = only_group(&document).expect("a group of one");
        let root = group.id.to_string();
        let shape = match group.children.first().expect("one child") {
            GroupChild::Shape(shape) => shape.id.to_string(),
            other => panic!("expected a shape, got {other:?}"),
        };

        let upright = document.object_rect(&shape);
        assert_eq!(upright.len(), 5, "the shape reached layout");
        assert_eq!(
            document.object_handles(&shape).len(),
            9 * 4,
            "an upright shape offers eight grips and the rotation grip"
        );

        document
            .set_object_rotation_inner(&shape, Some(30.0))
            .expect("rotate it 30 degrees");

        let handles = document.object_handles(&shape);
        assert_eq!(
            handles.len(),
            9 * 4,
            "a ROTATED shape still offers all nine grips — this is the assertion \
             the old `rotation.is_none()` gate failed, answering zero"
        );
        let frame = document.object_frame(&shape);
        assert_eq!(frame.len(), 6, "objectFrame is [page,x,y,w,h,milliDegrees]");
        assert_eq!(
            &frame[..5],
            &upright[..],
            "the published frame is the object's own UNROTATED box, unchanged by \
             the rotation — the rotation is about its centre"
        );
        assert_eq!(
            frame[5], 30_000,
            "and the angle is published in milli-degrees"
        );

        // The grips are drawn where the object IS. A 30-degree turn moves the
        // north-west grip off the frame's corner by a computable amount.
        let (cx, cy) = (upright[1] + upright[3] / 2, upright[2] + upright[4] / 2);
        let radians = 30.0_f64.to_radians();
        let (dx, dy) = (f64::from(upright[1] - cx), f64::from(upright[2] - cy));
        #[allow(clippy::cast_possible_truncation)]
        let expected = (
            (f64::from(cx) + dx * radians.cos() - dy * radians.sin()).round() as i32,
            (f64::from(cy) + dx * radians.sin() + dy * radians.cos()).round() as i32,
        );
        assert_eq!(
            (handles[1], handles[2]),
            expected,
            "the NW grip is drawn at the rotated corner, not at the unrotated one"
        );

        // And a resize at that angle reaches the document.
        let before = document.object_extent(&shape);
        document
            .resize_object(
                &root,
                f64::from(upright[1]) * 635.0,
                f64::from(upright[2]) * 635.0,
                f64::from(upright[3] + 720) * 635.0,
                f64::from(upright[4]) * 635.0,
            )
            .expect("a rotated object resizes");
        let after = document.object_extent(&shape);
        assert_ne!(after, before, "the rotated shape's extent actually changed");
        assert!(
            !document.object_transform(&shape).is_empty(),
            "and it is still rotated afterwards"
        );
        let read: serde_json::Value =
            serde_json::from_str(&document.object_transform(&shape)).expect("JSON");
        assert_eq!(read["rotationDegrees"], serde_json::json!(30.0));
    }

    /// A rotation handle must not appear on an object whose rotation nothing
    /// would paint — `SKILL` §10, never a dead control. A top-level text box
    /// models no `a:xfrm` at all (so `setObjectRotation` refuses it), and a
    /// group ROOT models one no layout pass applies.
    #[test]
    fn an_object_whose_rotation_is_not_painted_is_offered_no_rotation_grip() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let paragraph = first_paragraph(&document);
        let inserted = document
            .insert_text_box(&paragraph, 0)
            .expect("insert a text box")
            .node;
        let text_box = owner_box(&document, &inserted);

        assert_eq!(
            document.object_handles(&text_box).len(),
            8 * 4,
            "a top-level text box gets its eight resize grips and NO rotation grip"
        );
        assert!(
            document
                .set_object_rotation_inner(&text_box, Some(30.0))
                .is_err(),
            "and the facade refuses to rotate it, which is what the missing grip says"
        );
    }

    /// The text of a module before its test module — the only half that ships.
    ///
    /// Split on the attribute ALONE, with no surrounding newlines: a Windows
    /// checkout presents this file with CRLF, so a pattern carrying `\n`
    /// matches nothing there, `production` hands back the WHOLE file including
    /// its own tests, and the scan then reports string literals out of the test
    /// bodies as production refusals. That is not hypothetical — it reddened
    /// `platform (Windows-x64)` while every POSIX runner was green, with
    /// offenders like `"INSIDE"` and `"…"` that exist only inside this module's
    /// tests.
    fn production(source: &str) -> &str {
        source
            .find("#[cfg(test)]")
            .map_or(source, |at| &source[..at])
    }

    /// The byte index of the closing quote of the string literal that starts at
    /// `start`, honouring `\"`.
    fn literal_end(span: &str, start: usize) -> Option<usize> {
        let bytes = span.as_bytes();
        (start..bytes.len()).find(|i| bytes[*i] == b'"' && bytes[i - 1] != b'\\')
    }

    /// The string literal whose closing quote sits at `close`, honouring `\"`.
    fn literal_ending_at(span: &str, close: usize) -> Option<&str> {
        let bytes = span.as_bytes();
        let mut at = close;
        while at > 0 {
            at -= 1;
            if bytes[at] == b'"' && (at == 0 || bytes[at - 1] != b'\\') {
                return Some(&span[at + 1..close]);
            }
        }
        None
    }

    /// Every string literal `span` hands to an error constructor as a BARE
    /// literal — `to_js("…")`, `"…".to_owned()`, `"…".to_string()` — rather than
    /// through `refused!` or a `const` built from it.
    ///
    /// Textual, and that is what makes it a cheap build-time rule: a
    /// `refused!(…)` or a named const reaches `.to_owned()` through a `)` or an
    /// identifier, never through a closing quote, so the two are told apart
    /// without parsing Rust.
    fn bare_error_literals(span: &str) -> Vec<&str> {
        let bytes = span.as_bytes();
        let mut out = Vec::new();
        for call in [".to_owned()", ".to_string()"] {
            let mut from = 0;
            while let Some(found) = span[from..].find(call) {
                let at = from + found;
                from = at + call.len();
                let mut back = at;
                while back > 0 && bytes[back - 1].is_ascii_whitespace() {
                    back -= 1;
                }
                if back > 0
                    && bytes[back - 1] == b'"'
                    && let Some(literal) = literal_ending_at(span, back - 1)
                {
                    out.push(literal);
                }
            }
        }
        let mut from = 0;
        while let Some(found) = span[from..].find("to_js(\"") {
            let start = from + found + "to_js(\"".len();
            match literal_end(span, start) {
                Some(end) => {
                    out.push(&span[start..end]);
                    from = end;
                }
                None => from = start,
            }
        }
        out
    }

    /// Every `#[wasm_bindgen(js_name = …)]` method in `source` that belongs to
    /// the object surface, paired with the text of the method that follows it.
    ///
    /// DERIVED from the source rather than hand-listed, so a new object entry
    /// point joins the scan by existing (`SKILL` section 8 — counts and lists
    /// are derived). The surface names itself: all fifteen of its exports carry
    /// `Object`, plus `moveGroupChildBy`, the group-member mover. `Group` alone
    /// would also catch `decideRevisionGroup`, which is a review command and
    /// another family's conversion.
    ///
    /// A method's text ends at the first `}` closed at method indent, which is
    /// this file's one and only shape for a method body.
    fn object_geometry_spans(source: &str) -> Vec<(&str, &str)> {
        const ATTRIBUTE: &str = "#[wasm_bindgen(js_name = ";
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(found) = source[from..].find(ATTRIBUTE) {
            let at = from + found + ATTRIBUTE.len();
            let Some(close) = source[at..].find(')') else {
                break;
            };
            from = at + close;
            let name = &source[at..at + close];
            if !(name.contains("Object") || name.contains("GroupChild")) {
                continue;
            }
            let end = source[at..]
                .find("\n    }\n")
                .map_or(source.len(), |offset| at + offset);
            out.push((name, &source[at..end]));
        }
        out
    }

    /// The object-geometry family may not grow a refusal the reader cannot act
    /// on.
    ///
    /// `webapp/src/edit_errors.mjs` replaces every UNMARKED refusal with one
    /// generic sentence, so a bare `Err("not a floating object")` reaches the
    /// reader as "That edit isn't supported for this selection yet" — about a
    /// selection that is fine. The whole family was like that, which is the
    /// second thing the owner hit while dragging shapes.
    ///
    /// This is the mirror of `casual_doc_edit::refusal`'s
    /// `no_module_of_this_crate_writes_the_marker_by_hand`: that one stops a
    /// hand-written marker from appearing without a code, this one stops a
    /// refusal from appearing without the marker at all.
    ///
    /// It does NOT claim to cover every refusal in `lib.rs` — the other
    /// families (paste, tables, fields) are their own conversions. It covers the
    /// object module in full and, in `lib.rs`, every method whose exported name
    /// mentions an object or a group.
    #[test]
    fn no_object_geometry_refusal_reaches_a_reader_without_a_code() {
        // Internal vocabulary, deliberately generic. A malformed node id or a
        // malformed JSON payload is a HOST programming error, not something a
        // reader can act on, and the id space is 2^32 node ids — a bound no
        // document reaches. `{"can":true}` is a JSON answer, not an error.
        const INTERNAL: &[&str] = &[
            "invalid node id",
            "id space exhausted",
            "expected a JSON array of node ids",
            "{\\\"can\\\":true}",
        ];

        let mut offenders: Vec<(&str, &str)> =
            bare_error_literals(production(include_str!("objects.rs")))
                .into_iter()
                .map(|literal| ("objects.rs", literal))
                .collect();
        for (name, span) in object_geometry_spans(production(include_str!("lib.rs"))) {
            offenders.extend(
                bare_error_literals(span)
                    .into_iter()
                    .map(|literal| (name, literal)),
            );
        }
        offenders.retain(|(_, literal)| !INTERNAL.contains(literal));

        assert!(
            offenders.is_empty(),
            "these object refusals reach the reader as the generic \"that edit isn't \
             supported for this selection\" sentence; give each one a sentence and a \
             code with `refused!` (the family's table is at the top of objects.rs), or \
             add it to INTERNAL with a reason: {offenders:#?}"
        );
    }

    /// One cause, one code, one sentence.
    ///
    /// A code is a published routing key a host translates, so the same code
    /// carrying two different sentences would make the translation wrong for one
    /// of them. Every `object.` code is defined exactly once — which is what the
    /// `const` table buys and what keeps it from drifting back into inline
    /// duplicates (there were seven, two of them already duplicated).
    #[test]
    fn every_object_refusal_code_is_defined_exactly_once() {
        let mut codes: Vec<&str> = Vec::new();
        for source in [
            production(include_str!("objects.rs")),
            production(include_str!("lib.rs")),
        ] {
            let mut from = 0;
            while let Some(found) = source[from..].find("refused!(") {
                let at = from + found + "refused!(".len();
                let Some(quote) = source[at..].find('"') else {
                    break;
                };
                let start = at + quote + 1;
                let Some(end) = literal_end(source, start) else {
                    break;
                };
                from = end;
                let code = &source[start..end];
                if code.starts_with("object.") {
                    codes.push(code);
                }
            }
        }
        assert!(
            codes.len() >= 25,
            "the object refusal family is the table at the top of objects.rs; \
             {} codes is too few to be it",
            codes.len()
        );
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        let mut duplicates: Vec<&&str> = Vec::new();
        for pair in sorted.windows(2) {
            if pair[0] == pair[1] && !duplicates.contains(&&pair[0]) {
                duplicates.push(&pair[0]);
            }
        }
        assert!(
            duplicates.is_empty(),
            "an object refusal code is defined more than once, so one cause can \
             reach the reader as two sentences: {duplicates:?}"
        );
        for code in &codes {
            assert!(
                code.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '.' || c == '-'),
                "a routing code is stable, dotted and machine-shaped: {code:?}"
            );
        }
    }

    /// And the sentences actually reach the boundary, marked.
    ///
    /// The source scans above are build-time rules about text; this drives three
    /// real refusals from three different entry points and asserts each arrives
    /// as a reader's sentence plus a routable code, because a rule about source
    /// text is not evidence that the behaviour is reachable (`SKILL` section 9
    /// rule 4).
    #[test]
    fn a_refused_object_edit_arrives_with_a_sentence_and_a_code() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let paragraph = first_paragraph(&document);
        let inserted = document
            .insert_text_box(&paragraph, 0)
            .expect("insert a text box")
            .node;
        let text_box = owner_box(&document, &inserted);

        let refusals = [
            (
                "position a paragraph",
                document
                    .set_object_position_inner(&paragraph, "{}")
                    .expect_err("a paragraph has no position"),
                "object.not-selectable",
            ),
            (
                "move a top-level box inside a group",
                document
                    .move_group_child_by_inner(&text_box, 1_000.0, 0.0)
                    .expect_err("it is in no group"),
                "object.not-group-child",
            ),
            (
                "ungroup a text box",
                document
                    .ungroup_object_inner(&text_box)
                    .expect_err("it is not a group"),
                "object.not-a-group",
            ),
        ];
        for (gesture, message, expected) in refusals {
            let (sentence, code) = casual_doc_edit::refusal::split(&message);
            assert_eq!(
                code,
                Some(expected),
                "refusing to {gesture} carries its routing code: {message:?}"
            );
            assert!(
                sentence.starts_with("refused: ")
                    && sentence.len() > "refused: ".len() + 20
                    && sentence.ends_with('.'),
                "and a sentence a reader can act on: {sentence:?}"
            );
        }
    }

    /// Where the text of `paragraph`'s first painted line starts, in the painted
    /// layout's own twips — the right edge of the wrap-exclusion band a
    /// left-hand float imposes on it.
    ///
    /// Read from the PAINTED layout, and only ever compared as a delta, so the
    /// reference (the fragment origin plus the run's origin, which excludes the
    /// paragraph's constant indent) need not be an absolute page x.
    fn first_text_left(document: &WasmDocument, paragraph: &str) -> i32 {
        let wanted: NodeId = paragraph.parse().expect("a node id");
        for page in &document.painted_layout().pages {
            for placed in &page.placed {
                if let BlockFragment::Paragraph { id, lines, .. } = &placed.fragment
                    && *id == wanted
                    && let Some(run) = lines.lines.first().and_then(|line| line.runs.first())
                {
                    return placed.rect.origin.x.raw() + run.origin.x.raw();
                }
            }
        }
        panic!("the paragraph is not painted with any text");
    }

    /// Dragging a lone shape has to move the band of text it excludes, not only
    /// the shape.
    ///
    /// Every inserted shape — and every lone autoshape Word writes — is a GROUP
    /// OF ONE, so `object_boxes` reports the leaf as the subject and the group
    /// as the root, the host reads `subject != root` as "a child inside a
    /// group", and the drag, the arrow nudge and the Shift+arrow nudge all route
    /// to `moveGroupChildBy`. That moved the child's `a:xfrm` offset and nothing
    /// else, while every wrap consumer reads the GROUP's `wp:anchor` and
    /// `wp:extent`: the shape painted where it was dropped and its exclusion
    /// band stayed at the group's original anchor, so the paragraph kept a
    /// two-inch hole around empty space while the shape overlapped unwrapped
    /// text elsewhere. The exporter wrote the unmoved anchor out, so the saved
    /// `.docx` was wrong the same way — a correctness bug, not a repaint glitch.
    #[test]
    fn dragging_a_lone_shape_moves_its_text_exclusion_by_the_same_delta() {
        let mut document = open_document(RICH_DOCX).expect("open the rich fixture");
        let paragraph = first_paragraph(&document);
        document
            .insert_shape(&paragraph, 0, "rectangle")
            .expect("insert a shape");
        let group = only_group(&document).expect("a group of one");
        let shape = match group.children.first().expect("one child") {
            GroupChild::Shape(shape) => shape.id.to_string(),
            other => panic!("expected a shape, got {other:?}"),
        };

        let before_shape = document.object_rect(&shape);
        assert_eq!(before_shape.len(), 5, "the shape reached layout");
        let before_text = first_text_left(&document, &paragraph);

        // Half an inch to the LEFT, so the band still runs off the column's
        // leading edge and the text has nowhere to go but to its right: the one
        // placement where the band's right edge IS where the text starts.
        document
            .move_group_child_by_inner(&shape, -457_200.0, 0.0)
            .expect("drag the shape half an inch left");

        let after_shape = document.object_rect(&shape);
        let after_text = first_text_left(&document, &paragraph);
        assert_eq!(
            after_shape[1] - before_shape[1],
            -720,
            "the shape itself moved half an inch left: {before_shape:?} -> {after_shape:?}"
        );
        assert_eq!(
            after_text - before_text,
            after_shape[1] - before_shape[1],
            "the text's exclusion band moved by the SAME delta as the shape \
             (the text started at {before_text}, now starts at {after_text})"
        );

        // And the SAVED file is right, not merely faithful. The exporter writes
        // `wp:anchor` from the GROUP and `a:off` from the leaf, and Word
        // computes wrap from anchor plus extent too, so a move that touched only
        // the leaf wrote a document that is wrong in Word as well.
        //
        // The assertion is the DISTANCE from the shape's leading edge to where
        // the text starts, because a round trip reproduces a stale anchor
        // perfectly: comparing the reopened numbers to the ones above would pass
        // whether the band followed the shape or not.
        let bytes = document.export_docx().expect("export the document");
        let reopened = open_document(&bytes).expect("reopen the exported document");
        let reopened_paragraph = first_paragraph(&reopened);
        let reopened_group = only_group(&reopened).expect("the shape came back as a group of one");
        let reopened_shape = match reopened_group.children.first().expect("one child") {
            GroupChild::Shape(shape) => shape.id.to_string(),
            other => panic!("expected a shape, got {other:?}"),
        };
        let reopened_rect = reopened.object_rect(&reopened_shape);
        assert_eq!(reopened_rect.len(), 5, "the exported shape reopens placed");
        assert_eq!(
            first_text_left(&reopened, &reopened_paragraph) - reopened_rect[1],
            before_text - before_shape[1],
            "the saved file keeps the text beside the shape it wraps around, \
             rather than around the space the shape left"
        );
    }
}
