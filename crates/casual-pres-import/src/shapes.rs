// SPDX-License-Identifier: Apache-2.0

//! `p:spTree` into [`ShapeTree`]: `p:sp`, `p:pic`, `p:grpSp`, the placeholder
//! slot, and the DrawingML shape properties each carries.
//!
//! # What is reused rather than redeclared
//!
//! A `p:sp` **is** `a:xfrm` + `a:prstGeom`/`a:custGeom` + `a:solidFill` + `a:ln`,
//! which is exactly what `v1::GroupShape` already models and what the preset
//! table, the guide evaluator and the display list already paint. So a `p:sp`
//! maps onto `GroupChild::Shape`, a `p:pic` onto `GroupChild::Picture`, and a
//! `p:grpSp` onto `GroupChild::Group` — with no parallel vocabulary, because two
//! copies of a geometry engine disagree by the second preset.
//!
//! What a slide adds is wrapped by [`SlideNode`] and not bolted onto
//! `GroupShape`: the placeholder slot, the author-visible name, the hidden flag,
//! and the text body.
//!
//! # The transform that is absent, and the one that is zero
//!
//! A placeholder shape on a real slide usually carries **no** `a:xfrm` at all: it
//! takes its position and size from the matching slot in its layout, which takes
//! them from the master. `Presentation::resolve_slot` is the lookup that does
//! that, and it is why discarding the slot would discard the geometry of nearly
//! every shape in a deck.
//!
//! This importer therefore does **not** resolve inheritance into the shape. The
//! shape is stored with a zero offset and extent and the cascade is left to the
//! consumer — which is also, independently, what ONLYOFFICE does: it resolves the
//! inherited transform lazily at draw time into volatile fields and leaves the
//! persisted properties null, so a round trip cannot accidentally materialise
//! inherited geometry as authored geometry.
//!
//! The cost is named rather than hidden: `v1::GroupShape` has `offset` and
//! `extent` as plain values, so "no `a:xfrm`" and "an explicit zero `a:xfrm`"
//! import identically. ONLYOFFICE's cascade distinguishes exactly those two
//! cases — it inherits only when the shape's own transform record is *empty* — so
//! the distinction is load-bearing, not theoretical, and every absent `a:xfrm` is
//! reported as a degraded `p:spPr`.

use std::collections::BTreeSet;

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    CropRect, DrawingHyperlink, Extent, ExternalTarget, Fill, GroupChild, GroupPicture, GroupShape,
    GroupTransform, HyperlinkTarget, MAX_GROUP_DEPTH, MAX_SHAPE_ADJUSTMENTS,
    MAX_SHAPE_FORMULA_BYTES, MAX_SHAPE_GUIDE_NAME_BYTES, MAX_SHAPE_PATH_COMMANDS,
    MAX_SHAPE_PRESET_BYTES, MediaId, PointEmu, ShapeAdjustment, ShapeGeometry, ShapePath,
    ShapePathCommand, ShapeStyleRef, WordprocessingGroup,
};
// Own line (anti-conflict): the shared geometry model a freeform is read into.
use casual_doc_model::v1::{CustomGeometry, GeometryPoint, PathFill};
use casual_pres_model::{
    Placeholder, PlaceholderKind, PlaceholderOrientation, PlaceholderSize, ShapeTree, SlideNode,
    SlidePaint, TextBody,
};
use quick_xml::events::BytesStart;

use crate::ImportError;
use crate::color::{FillRead, LineRead, read_fill_child, read_line};
use crate::ids::Ids;
use crate::loss::Reporter;
use crate::media::MediaResolver;
// Own line (anti-conflict): the relationship table `a:hlinkClick` resolves against.
use crate::opc::{LinkTarget, Relationships, link_target};
// Own line (anti-conflict): the theme a shape's colours and `p:style` resolve against.
use crate::text::read_text_body;
use crate::theme::{Resolver, read_shape_style};
use crate::xml::{
    Cursor, attribute, boolean_attribute, children, enter, integer_attribute, local_name,
};

/// A DrawingML transform (`a:xfrm`), and whether the source wrote one at all.
///
/// `present` and `child_space_present` are the fields that carry the thing the
/// reused `v1` types cannot: whether the file *stated* a transform, as opposed to
/// stating a zero one. The placeholder cascade turns on exactly that difference.
#[derive(Clone, Copy, Debug)]
struct Transform {
    present: bool,
    offset: PointEmu,
    extent: Extent,
    child_offset: PointEmu,
    child_extent: Extent,
    child_space_present: bool,
    flip_h: bool,
    flip_v: bool,
    rotation: Option<i32>,
}

/// The transform of a shape that states none: a zero box at the origin, flagged
/// absent so the caller can report the inheritance it cannot represent.
///
/// Written out rather than derived because neither `v1::PointEmu` nor
/// `v1::Extent` implements `Default` — deliberately, since a zero EMU extent is a
/// real value in that model and not an absence.
impl Default for Transform {
    fn default() -> Self {
        let origin = PointEmu { x_emu: 0, y_emu: 0 };
        let empty = Extent {
            width_emu: 0,
            height_emu: 0,
        };
        Self {
            present: false,
            offset: origin,
            extent: empty,
            child_offset: origin,
            child_extent: empty,
            child_space_present: false,
            flip_h: false,
            flip_v: false,
            rotation: None,
        }
    }
}

/// The surface a top-level `p:spTree` positions its children on.
///
/// A real slide's `p:grpSpPr` is usually empty, or carries a zero `a:ext`/
/// `a:chExt`. `ShapeTree::validate` refuses a zero child extent under a populated
/// tree — correctly, because a child point is mapped into the parent box by the
/// ratio `ext / chExt`, so a zero child extent collapses every shape to a point.
///
/// For a *top-level* tree that zero does not mean a degenerate space: the model's
/// own documentation says the box is the slide surface and the child space is
/// normally identical to it, so children are in slide EMU directly. Substituting
/// the slide size for an absent or zero top-level extent is therefore the
/// identity mapping the file means, not a repair — and getting this wrong refuses
/// essentially every real deck.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Surface {
    /// Slide width in EMU.
    pub(crate) width_emu: i64,
    /// Slide height in EMU.
    pub(crate) height_emu: i64,
}

/// Keeps the FIRST shape in each placeholder slot and demotes any later one to a
/// plain shape, reporting each demotion.
///
/// # Why this exists: a real deck was refused
///
/// `ShapeTree::validate` enforces one shape per `(type, idx)` slot and one title
/// per tree, and `Presentation::new` validates — so a deck violating either was
/// refused **whole**, with no slide reaching the screen. That is the wrong failure
/// and it was observed on a real PowerPoint file, which refused with "has two title
/// placeholders at index 4294967295": PowerPoint leaves an orphaned placeholder
/// behind when a slide's layout is changed, writes a 32-bit `@idx` that is an
/// identifier rather than a sequence number, and opens such a file without
/// complaint. A viewer that refuses what the producer opens is the "loud refusal
/// turned into a blank page" failure inverted — a loud refusal where the file is
/// readable.
///
/// # Why demote rather than relax the invariant
///
/// Because the invariant is what makes slot resolution unambiguous. `ShapeTree`'s
/// own `slot` lookup returns the FIRST match, so "the first shape in the slot" is
/// already the engine's answer; making the model admit a second one would leave a
/// rule stated in one place and relied on in another. Demoting makes the model say
/// what the engine does.
///
/// # What it costs, stated rather than hidden
///
/// A demoted shape no longer inherits position, size or text properties from its
/// layout, so one carrying `<p:spPr/>` and nothing else becomes a zero-sized box
/// that paints nothing. That is why each demotion is REPORTED rather than done
/// quietly: it is a real fidelity loss, it is smaller than losing every slide, and
/// an orphaned duplicate is usually empty in practice — but "usually" is not
/// "always", and the report is what keeps that honest.
///
/// # Complexity
///
/// O(children) with one `BTreeSet` of the slots seen — the same scan
/// `validate_placeholders` performs, done once on the way in instead of once on
/// the way out.
fn demote_duplicate_slots(nodes: &mut [SlideNode], reporter: &mut Reporter, part: &str) {
    let mut seen: BTreeSet<(casual_pres_model::PlaceholderKind, u32)> = BTreeSet::new();
    let mut titled = false;
    for node in nodes.iter_mut() {
        let Some(placeholder) = node.placeholder else {
            continue;
        };
        let title = placeholder.kind.is_title();
        // Two conditions, not one: a second shape in the same slot, and a second
        // TITLE whatever its slot — a `title` and a `ctrTitle` are one slot for
        // inheritance everywhere else in this engine, so they collide with each
        // other even though their `(kind, idx)` pairs differ.
        if !seen.insert(placeholder.slot()) || (title && titled) {
            node.placeholder = None;
            reporter.degraded_attribute(part, b"ph", b"idx");
            continue;
        }
        titled |= title;
    }
}

/// Reads a `p:cSld`'s `p:spTree`, having just entered the `p:spTree`.
///
/// # Complexity
///
/// O(shapes in the tree), one pass, bounded by `ImportLimits::max_shapes_per_tree`.
pub(crate) fn read_shape_tree(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    ids: &mut Ids,
    media: &mut MediaResolver<'_>,
    surface: Surface,
    resolver: Resolver,
    styles: &mut Vec<(NodeId, ShapeStyleRef)>,
) -> Result<ShapeTree, ImportError> {
    let part = cursor.part().to_owned();
    let limits = cursor.limits();
    let id = ids.next()?;
    let mut transform = Transform::default();
    let mut nodes: Vec<SlideNode> = Vec::new();

    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"nvGrpSpPr" => Ok(false),
            b"grpSpPr" => {
                if empty {
                    return Ok(false);
                }
                transform =
                    read_group_properties(cursor, reporter, &mut FillRead::default(), resolver)?;
                Ok(true)
            }
            b"sp" | b"pic" | b"grpSp" | b"graphicFrame" | b"cxnSp" | b"contentPart" => {
                if nodes.len() >= limits.max_shapes_per_tree {
                    reporter.invalid(&part, local);
                    return Ok(false);
                }
                let consumed = read_tree_child(
                    cursor, reporter, ids, media, element, empty, local, &mut nodes, 0, resolver,
                    styles,
                )?;
                Ok(consumed)
            }
            b"extLst" => Ok(false),
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;

    demote_duplicate_slots(&mut nodes, reporter, &part);

    // The top-level identity mapping described on `Surface`.
    let extent = normalize_extent(transform.extent, surface);
    let child_extent = normalize_extent(transform.child_extent, surface);
    Ok(ShapeTree {
        id,
        transform: GroupTransform {
            offset: transform.offset,
            extent,
            child_offset: transform.child_offset,
            child_extent,
            flip_h: transform.flip_h,
            flip_v: transform.flip_v,
            rotation: transform.rotation,
        },
        children: nodes,
    })
}

fn normalize_extent(extent: Extent, surface: Surface) -> Extent {
    Extent {
        width_emu: if extent.width_emu > 0 {
            extent.width_emu
        } else {
            surface.width_emu
        },
        height_emu: if extent.height_emu > 0 {
            extent.height_emu
        } else {
            surface.height_emu
        },
    }
}

/// Reads one `p:spTree` child into `nodes`.
///
/// `depth` counts enclosing `p:grpSp`, so a top-level child is at zero — the
/// `p:spTree` itself is the root container every slide has and counting it would
/// spend one of the sixteen levels on nothing, which is also how
/// `ShapeTree::validate` counts.
#[expect(
    clippy::too_many_arguments,
    reason = "the recursion carries the reader's whole context; bundling it into a \
              struct would add a lifetime for no reduction in what must be threaded"
)]
fn read_tree_child(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    ids: &mut Ids,
    media: &mut MediaResolver<'_>,
    element: &BytesStart<'_>,
    empty: bool,
    local: &[u8],
    nodes: &mut Vec<SlideNode>,
    depth: u32,
    resolver: Resolver,
    styles: &mut Vec<(NodeId, ShapeStyleRef)>,
) -> Result<bool, ImportError> {
    let part = cursor.part().to_owned();
    // Lives as long as the relationship table rather than as long as the borrow,
    // so it can be held across the `&mut media` calls below.
    let links = media.relationships();
    match local {
        b"sp" => {
            if empty {
                reporter.invalid(&part, b"sp");
                return Ok(false);
            }
            if let Some(node) = read_shape(cursor, reporter, ids, false, resolver, styles, links)? {
                nodes.push(node);
            }
            Ok(true)
        }
        b"cxnSp" => {
            // A connector is a shape with endpoints. Its geometry, fill and line
            // read identically, so it maps onto the same `GroupShape`; what is
            // lost is the `p:cNvCxnSpPr` start/end shape bindings, which have no
            // field — so a BOUND connector imports at its authored position but
            // stops following the shapes it joins.
            //
            // There is no blanket `degraded(cxnSp)` here any more. It fired on
            // every connector, and a connector that binds neither end loses
            // nothing at all, so the report charged a loss to the plain drawn line
            // that is most of them. `read_non_visual` now reports each binding
            // that is actually declared, by the name of the end it lost.
            if empty {
                reporter.invalid(&part, b"cxnSp");
                return Ok(false);
            }
            if let Some(node) = read_shape(cursor, reporter, ids, true, resolver, styles, links)? {
                nodes.push(node);
            }
            Ok(true)
        }
        b"pic" => {
            if empty {
                reporter.invalid(&part, b"pic");
                return Ok(false);
            }
            if let Some(node) = read_picture(cursor, reporter, ids, media, resolver, styles)? {
                nodes.push(node);
            }
            Ok(true)
        }
        b"grpSp" => {
            if empty {
                reporter.invalid(&part, b"grpSp");
                return Ok(false);
            }
            if depth + 1 > MAX_GROUP_DEPTH {
                // Refused rather than flattened: flattening would move every
                // nested child into the wrong coordinate space, which looks like
                // a layout bug rather than a refused construct.
                reporter.invalid(&part, b"grpSp");
                return Ok(false);
            }
            if let Some(node) = read_group(cursor, reporter, ids, media, depth, resolver, styles)? {
                nodes.push(node);
            }
            Ok(true)
        }
        b"graphicFrame" => {
            // A `p:graphicFrame` holds a table, a chart, a SmartArt diagram or an
            // OLE object. The FRAME is read in every case — its box, its name and
            // its hidden flag are the same non-visual properties a `p:sp` carries
            // — and only an `a:tbl` payload arrives with it. Charts and SmartArt
            // belong to `docs/155`/ADR-050 and `crate::table` reports each by the
            // payload's own name, so the report says what was actually lost
            // instead of discarding the frame and naming the wrapper.
            if empty {
                reporter.invalid(&part, b"graphicFrame");
                return Ok(false);
            }
            if let Some(node) =
                crate::table::read_graphic_frame(cursor, reporter, ids, resolver, links)?
            {
                nodes.push(node);
            }
            Ok(true)
        }
        b"contentPart" => {
            reporter.omitted(&part, b"contentPart");
            Ok(false)
        }
        _ => {
            let _ = element;
            Ok(false)
        }
    }
}

/// Non-visual properties shared by every shape kind (`p:nvSpPr`, `p:nvPicPr`,
/// `p:nvGrpSpPr`, `p:nvCxnSpPr`).
#[derive(Clone, Debug, Default)]
pub(crate) struct NonVisual {
    pub(crate) name: Option<String>,
    pub(crate) hidden: bool,
    pub(crate) placeholder: Option<Placeholder>,
    descr: Option<String>,
    /// The resolved `p:cNvPr/a:hlinkClick` — the thing that makes this shape or
    /// picture clickable — when it names a target the model can carry.
    pub(crate) hyperlink: Option<DrawingHyperlink>,
}

/// Reads a `p:nv*Pr` wrapper: its `p:cNvPr`, its `p:cNv*Pr`, and its `p:nvPr`.
///
/// `links` is the DECLARING part's relationship table, because `a:hlinkClick`
/// spells its target as an `r:id` scoped to that part — the same scoping rule
/// `MediaResolver`'s module note sets out for `a:blip@r:embed`.
pub(crate) fn read_non_visual(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    links: &Relationships,
) -> Result<NonVisual, ImportError> {
    let part = cursor.part().to_owned();
    let mut non_visual = NonVisual::default();
    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"cNvPr" => {
                // The authored id is unique only within the part; see `ids.rs`
                // for why it cannot be carried and why losing it matters for
                // `p:timing`'s `spid`.
                if attribute(element, b"id", cursor.part())?.is_some() {
                    reporter.degraded_attribute(&part, b"cNvPr", b"id");
                }
                non_visual.name =
                    attribute(element, b"name", cursor.part())?.filter(|name| !name.is_empty());
                non_visual.hidden =
                    boolean_attribute(element, b"hidden", cursor.part())?.unwrap_or(false);
                non_visual.descr =
                    attribute(element, b"descr", cursor.part())?.filter(|descr| !descr.is_empty());
                // Its CHILDREN are entered now, which they were not before: a
                // `p:cNvPr` holds `a:hlinkClick`, and returning `Ok(false)` here
                // made `children` skip the subtree — so a linked shape lost its
                // link with nothing in the report saying so. A skipped subtree is
                // silent, not reported (see `children`'s own contract).
                enter(cursor, empty, |cursor, child, child_empty| {
                    read_click_target(
                        cursor,
                        reporter,
                        child,
                        child_empty,
                        &part,
                        links,
                        &mut non_visual.hyperlink,
                    )
                })
            }
            b"nvPr" => {
                if empty {
                    return Ok(false);
                }
                non_visual.placeholder = read_placeholder_slot(cursor, reporter)?;
                Ok(true)
            }
            // A CONNECTOR's own non-visual properties, and the one shape kind
            // whose `p:cNv*Pr` carries document content rather than editing
            // locks: `a:stCxn`/`a:endCxn` bind the connector's two ends to the
            // shapes it joins, and `GroupShape` has no field for either.
            //
            // Entered rather than skipped, so the report can name WHICH end was
            // unbound. The blanket `degraded(cxnSp)` this replaces fired on every
            // connector in the deck, including the overwhelming majority that bind
            // nothing at all and therefore lose nothing — a finding on healthy
            // markup, which is the half of `fc3ec556` that was wrong in the
            // generous direction.
            b"cNvCxnSpPr" => enter(cursor, empty, |_cursor, child, _child_empty| {
                match local_name(child) {
                    end @ (b"stCxn" | b"endCxn") => reporter.omitted(&part, end),
                    // An editing lock, not document content.
                    b"cxnSpLocks" | b"extLst" => {}
                    other => reporter.omitted(&part, other),
                }
                Ok(false)
            }),
            // `p:cNvGraphicFramePr` joins the list for a `p:graphicFrame`. Its only
            // child is `a:graphicFrameLocks`, which is an editing lock and not
            // document content, so reporting it would put a false loss in the
            // report for every table in a deck.
            b"cNvSpPr" | b"cNvPicPr" | b"cNvGrpSpPr" | b"cNvGraphicFramePr" => Ok(false),
            b"extLst" => Ok(false),
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;
    Ok(non_visual)
}

/// Reads one child of a `p:cNvPr`, resolving `a:hlinkClick` into `hyperlink`.
///
/// # The empty `r:id`, which is NOT a link
///
/// PowerPoint writes `<a:hlinkClick r:id=""/>` where a link was removed, and on
/// shapes that never had one. Resolving the empty id against the relationship
/// table finds nothing, so the obvious reading is "a link we could not resolve" —
/// and reporting that would put a loss finding on markup that states no link, in
/// a deck that lost nothing. It is therefore distinguished from a *populated* id
/// that fails to resolve, which is a real loss and is reported.
fn read_click_target(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
    part: &str,
    links: &Relationships,
    hyperlink: &mut Option<DrawingHyperlink>,
) -> Result<bool, ImportError> {
    let local = local_name(element);
    match local {
        b"hlinkClick" => {
            let Some(relationship_id) = attribute(element, b"id", cursor.part())? else {
                // No `r:id` at all. If it carries an `@action` it is a show action
                // (`ppaction://hlinkshowjump?jump=nextslide`), which this build does
                // not model; with neither, it states nothing.
                if action(element, cursor.part())?.is_some() {
                    reporter.omitted(part, local);
                }
                return Ok(false);
            };
            if relationship_id.is_empty() {
                // See this function's note: the no-link spelling.
                return Ok(false);
            }
            // The screen tip, under the model's own bound for it
            // (`v1::DrawingHyperlink::tooltip`: non-empty, at most 255 bytes).
            let tooltip = attribute(element, b"tooltip", cursor.part())?
                .filter(|value| !value.is_empty() && value.len() <= 255);
            match link_target(links, &relationship_id) {
                LinkTarget::External(url) => {
                    // No `anchor`: `a:hlinkClick` has no `@anchor` of its own —
                    // unlike `w:hyperlink` — so a fragment is already part of the
                    // relationship target and splitting it out here would invent a
                    // field the file does not have.
                    *hyperlink = Some(DrawingHyperlink {
                        target: HyperlinkTarget::External(ExternalTarget { url, anchor: None }),
                        tooltip,
                    });
                    // The link survives; the VERB does not. `ppaction://hlinkfile`
                    // on an external target says "open this in its application"
                    // rather than "navigate", and nothing carries that.
                    if action(element, cursor.part())?.is_some() {
                        reporter.degraded_attribute(part, local, b"action");
                    }
                }
                // Both are losses, and reported under the one feature name,
                // because what the reader can say about each is the same: the file
                // declared a link here and the model holds none.
                LinkTarget::InPackage | LinkTarget::Unresolved => reporter.omitted(part, local),
            }
            Ok(false)
        }
        // The HOVER link. A separate element with a separate target, and
        // `DrawingHyperlink` is one link per drawing, so there is nowhere for it
        // to go even when the click link is absent.
        b"hlinkHover" => {
            reporter.omitted(part, local);
            Ok(false)
        }
        b"extLst" => Ok(false),
        other => {
            if !empty {
                reporter.omitted(part, other);
            }
            Ok(false)
        }
    }
}

/// An `@action`, filtered to the non-empty case.
///
/// `action=""` is how PowerPoint spells "no action" beside a real `r:id`, the
/// same way `r:id=""` spells "no link"; treating it as a lost verb would report a
/// loss on every ordinary hyperlink it writes.
fn action(element: &BytesStart<'_>, part: &str) -> Result<Option<String>, ImportError> {
    Ok(attribute(element, b"action", part)?.filter(|value| !value.is_empty()))
}

/// Reads `p:nvPr`'s children, returning the `p:ph` slot if it declares one.
fn read_placeholder_slot(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
) -> Result<Option<Placeholder>, ImportError> {
    let part = cursor.part().to_owned();
    let mut placeholder = None;
    children(cursor, |cursor, element, _empty| {
        let local = local_name(element);
        match local {
            b"ph" => {
                // Inheritance resolves by the PAIR (type, idx), never by either
                // alone: a two-content layout has two `body` slots distinguished
                // only by `idx`, so matching on the type picks the wrong one half
                // the time. An absent `@type` is `obj`, which is the schema's own
                // default; an absent `@idx` is zero.
                placeholder = Some(Placeholder {
                    kind: attribute(element, b"type", cursor.part())?
                        .as_deref()
                        .map_or(PlaceholderKind::Object, PlaceholderKind::from_token),
                    index: integer_attribute(element, b"idx", cursor.part())?
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or(0),
                    size: attribute(element, b"sz", cursor.part())?
                        .as_deref()
                        .map_or(PlaceholderSize::Full, PlaceholderSize::from_token),
                    orientation: attribute(element, b"orient", cursor.part())?
                        .as_deref()
                        .map_or(
                            PlaceholderOrientation::Horizontal,
                            PlaceholderOrientation::from_token,
                        ),
                    has_custom_prompt: boolean_attribute(
                        element,
                        b"hasCustomPrompt",
                        cursor.part(),
                    )?
                    .unwrap_or(false),
                });
                Ok(false)
            }
            b"audioFile" | b"videoFile" | b"quickTimeFile" | b"wavAudioFile" | b"audioCd" => {
                reporter.omitted(&part, local);
                Ok(false)
            }
            b"custDataLst" | b"extLst" => Ok(false),
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;
    Ok(placeholder)
}

/// Geometry read from a `p:spPr`.
#[derive(Clone, Debug, Default)]
struct Geometry {
    geometry: ShapeGeometry,
    preset: Option<String>,
    adjustments: Vec<ShapeAdjustment>,
    path: Option<CustomGeometry>,
}

/// A shape's `p:spPr`: transform, geometry, fill, outline.
///
/// `fill` and `line` carry what the element STATED as well as what it resolved to,
/// because `a:noFill` resolves to the same `None` an absent element does and means
/// the opposite — see `casual_pres_model::SlidePaint`.
#[derive(Clone, Debug, Default)]
struct ShapeProperties {
    transform: Transform,
    geometry: Geometry,
    fill: FillRead,
    line: LineRead,
}

/// Reads one `p:sp` (or `p:cxnSp`), having just entered it.
fn read_shape(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    ids: &mut Ids,
    connector: bool,
    resolver: Resolver,
    styles: &mut Vec<(NodeId, ShapeStyleRef)>,
    links: &Relationships,
) -> Result<Option<SlideNode>, ImportError> {
    let part = cursor.part().to_owned();
    let id = ids.next()?;
    let mut non_visual = NonVisual::default();
    let mut properties = ShapeProperties::default();
    let mut text: Option<TextBody> = None;

    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"nvSpPr" | b"nvCxnSpPr" => {
                if empty {
                    return Ok(false);
                }
                non_visual = read_non_visual(cursor, reporter, links)?;
                Ok(true)
            }
            // An empty `<p:spPr/>` needs no special case: `Transform::default`
            // already has `present` false, so the single post-loop check below
            // covers all three shapes of the same fact — no `p:spPr`, an empty
            // one, and a populated one with no `a:xfrm` inside it. Reporting
            // here as well counted every `<p:spPr/>` shape TWICE, which a
            // presence-only guard could not see; the count-derived guard in
            // `tests.rs` is what caught it.
            b"spPr" => enter(cursor, empty, |cursor, child, child_empty| {
                read_shape_property(
                    cursor,
                    reporter,
                    child,
                    child_empty,
                    &mut properties,
                    resolver,
                )
            }),
            b"txBody" => {
                if empty {
                    return Ok(false);
                }
                text = Some(read_text_body(cursor, reporter, ids, resolver)?);
                Ok(true)
            }
            b"style" => {
                // `p:style` names theme fill/line/effect/font references
                // (`a:fillRef`, `a:lnRef`, `a:effectRef`, `a:fontRef`), and the
                // reference is kept AS a reference in the `Definitions` side table
                // the document class already uses for `wps:style`. Resolving the
                // matrix entry into `GroupShape::fill` instead would turn the
                // theme's content into the shape's authorship, so a later theme
                // change would stop following it.
                //
                // An empty `<p:style/>` is malformed — all four children are
                // required — and entering it would consume the next sibling's
                // events, so the guard is not cosmetic.
                if empty {
                    reporter.invalid(&part, b"style");
                    return Ok(false);
                }
                styles.push((id, read_shape_style(cursor, reporter, resolver)?));
                Ok(true)
            }
            b"extLst" => Ok(false),
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;

    if !properties.transform.present {
        reporter.degraded_attribute(&part, b"spPr", b"xfrm");
    }

    let _ = connector;
    let shape = GroupShape {
        id,
        offset: properties.transform.offset,
        extent: properties.transform.extent,
        geometry: properties.geometry.geometry,
        preset: properties.geometry.preset,
        adjustments: properties.geometry.adjustments,
        path: properties.geometry.path,
        fill: properties.fill.fill,
        stroke: properties.line.stroke,
        flip_h: properties.transform.flip_h,
        flip_v: properties.transform.flip_v,
        rotation: properties.transform.rotation,
        hyperlink: non_visual.hyperlink,
    };
    Ok(Some(SlideNode {
        placeholder: non_visual.placeholder,
        name: non_visual.name,
        hidden: non_visual.hidden,
        // The two states the `GroupShape` above cannot hold. Named rather than
        // defaulted, because defaulting here is exactly the silent drop that made
        // `a:noFill` a reported loss in the first place.
        fill: properties.fill.state,
        outline: properties.line.state,
        content: GroupChild::Shape(shape),
        text,
        table: None,
    }))
}

/// Reads one `p:pic`, having just entered it.
fn read_picture(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    ids: &mut Ids,
    media: &mut MediaResolver<'_>,
    resolver: Resolver,
    styles: &mut Vec<(NodeId, ShapeStyleRef)>,
) -> Result<Option<SlideNode>, ImportError> {
    let part = cursor.part().to_owned();
    let id = ids.next()?;
    let mut non_visual = NonVisual::default();
    let mut properties = ShapeProperties::default();
    let mut blip_fill = BlipFill::default();
    let mut text: Option<TextBody> = None;
    // See `read_tree_child`: outlives the borrow, so the closure below and the
    // `&mut media` resolution after it can both have what they need.
    let links = media.relationships();

    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"nvPicPr" => {
                if empty {
                    return Ok(false);
                }
                non_visual = read_non_visual(cursor, reporter, links)?;
                Ok(true)
            }
            b"blipFill" => {
                if empty {
                    return Ok(false);
                }
                blip_fill = read_blip_fill(cursor, reporter)?;
                Ok(true)
            }
            // See `read_shape`: the post-loop check is the single reporting
            // site for a shape that states no transform.
            b"spPr" => enter(cursor, empty, |cursor, child, child_empty| {
                read_shape_property(
                    cursor,
                    reporter,
                    child,
                    child_empty,
                    &mut properties,
                    resolver,
                )
            }),
            b"txBody" => {
                if empty {
                    return Ok(false);
                }
                text = Some(read_text_body(cursor, reporter, ids, resolver)?);
                Ok(true)
            }
            b"style" => {
                if empty {
                    reporter.invalid(&part, b"style");
                    return Ok(false);
                }
                styles.push((id, read_shape_style(cursor, reporter, resolver)?));
                Ok(true)
            }
            b"extLst" => Ok(false),
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;

    // `GroupPicture::media` is not optional and `ShapeTree::validate` refuses a
    // media reference that does not resolve, so a picture whose `r:embed` names
    // nothing admitted cannot be modelled at all. Reported as invalid — refused,
    // not merely dropped — rather than substituting a placeholder image, which
    // would look like the author's choice.
    let Some(embed) = blip_fill.embed else {
        reporter.invalid(&part, b"blip");
        return Ok(None);
    };
    let Some(media_id) = media.resolve(&embed, reporter, ids)? else {
        reporter.invalid(&part, b"blip");
        return Ok(None);
    };

    if !properties.transform.present {
        reporter.degraded_attribute(&part, b"spPr", b"xfrm");
    }

    let picture = GroupPicture {
        id,
        media: MediaId::new(media_id),
        offset: properties.transform.offset,
        extent: properties.transform.extent,
        descr: non_visual.descr,
        crop: blip_fill.crop,
        opacity: None,
        hyperlink: non_visual.hyperlink,
        border: properties.line.stroke,
        flip_h: properties.transform.flip_h,
        flip_v: properties.transform.flip_v,
        rotation: properties.transform.rotation,
    };
    Ok(Some(SlideNode {
        placeholder: non_visual.placeholder,
        name: non_visual.name,
        hidden: non_visual.hidden,
        // A `p:pic`'s `p:spPr` fill is the fill BEHIND the image, which
        // `GroupPicture` has no field for — so `a:noFill` there is carried and not
        // contradicted by anything, and the outline is the picture's frame.
        fill: properties.fill.state,
        outline: properties.line.state,
        content: GroupChild::Picture(picture),
        text,
        // A `p:pic` is not a `p:graphicFrame`; only the frame reader produces a
        // table, so this is `None` by construction rather than by omission.
        table: None,
    }))
}

/// What a `p:blipFill` carries that a [`GroupPicture`] has a field for.
#[derive(Debug, Default)]
struct BlipFill {
    /// The `a:blip@r:embed` relationship id.
    embed: Option<String>,
    /// The `a:srcRect` source crop, when it hides something.
    crop: Option<CropRect>,
}

/// Reads a `p:blipFill` into the fields a picture can hold.
///
/// `r:link` is deliberately not followed: an external image is a network fetch at
/// open time, which is an exfiltration channel and a hang. Reported instead.
fn read_blip_fill(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
) -> Result<BlipFill, ImportError> {
    let part = cursor.part().to_owned();
    let mut blip_fill = BlipFill::default();
    children(cursor, |cursor, element, _empty| {
        let local = local_name(element);
        match local {
            b"blip" => {
                blip_fill.embed = attribute(element, b"embed", cursor.part())?;
                if blip_fill.embed.is_none()
                    && attribute(element, b"link", cursor.part())?.is_some()
                {
                    reporter.omitted(&part, b"blip/link");
                }
                // `a:blip`'s children are the picture effects (`a:alphaModFix`,
                // `a:biLevel`, `a:duotone`, `a:lum`, …). `GroupPicture` carries
                // `opacity` for the first; the rest have no field. Not consuming
                // the subtree here means `children` skips it, so each is reported
                // once at this level rather than per effect.
                Ok(false)
            }
            b"srcRect" => {
                // The four edge fractions, in thousandths of a percent of the
                // SOURCE image (`ST_Percentage`). A missing edge is zero — crop
                // nothing on that side — so `<a:srcRect t="20000"/>` is a top crop
                // and not three dropped attributes.
                //
                // This is a second parse SITE and not a second implementation of
                // the rule. The two decisions that could drift are the clamp into
                // the legal range and "an all-zero rect is no crop at all", and
                // both are `CropRect::clamped`/`CropRect::is_identity` on the
                // shared model — the same two calls `casual-doc-import` makes.
                // Only reaching the element differs, because a `p:blipFill` is
                // reached differently from a `pic:blipFill`.
                //
                // The identity rect is dropped rather than modelled, so a producer
                // that writes the no-op explicitly does not become a document
                // carrying a field that changes nothing — and an editor offering to
                // reset a crop does not offer it on a picture that has none.
                let edge = |name: &[u8]| -> Result<i32, ImportError> {
                    Ok(integer_attribute(element, name, cursor.part())?
                        .and_then(|value| i32::try_from(value).ok())
                        .unwrap_or(0))
                };
                let crop = CropRect {
                    left: edge(b"l")?,
                    top: edge(b"t")?,
                    right: edge(b"r")?,
                    bottom: edge(b"b")?,
                }
                .clamped();
                if !crop.is_identity() {
                    blip_fill.crop = Some(crop);
                }
                Ok(false)
            }
            b"stretch" | b"tile" => Ok(false),
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;
    Ok(blip_fill)
}

/// Reads one `p:grpSp`, having just entered it.
fn read_group(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    ids: &mut Ids,
    media: &mut MediaResolver<'_>,
    depth: u32,
    resolver: Resolver,
    styles: &mut Vec<(NodeId, ShapeStyleRef)>,
) -> Result<Option<SlideNode>, ImportError> {
    let part = cursor.part().to_owned();
    let limits = cursor.limits();
    let id = ids.next()?;
    let mut non_visual = NonVisual::default();
    let mut transform = Transform::default();
    let mut nodes: Vec<SlideNode> = Vec::new();
    // See `read_tree_child`: outlives the borrow, so the closure can read it while
    // the recursion below holds `media` mutably.
    let links = media.relationships();

    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"nvGrpSpPr" => {
                if empty {
                    return Ok(false);
                }
                non_visual = read_non_visual(cursor, reporter, links)?;
                Ok(true)
            }
            b"grpSpPr" => {
                if empty {
                    return Ok(false);
                }
                transform =
                    read_group_properties(cursor, reporter, &mut FillRead::default(), resolver)?;
                Ok(true)
            }
            b"sp" | b"pic" | b"grpSp" | b"graphicFrame" | b"cxnSp" | b"contentPart" => {
                if nodes.len() >= limits.max_shapes_per_tree {
                    reporter.invalid(&part, local);
                    return Ok(false);
                }
                read_tree_child(
                    cursor,
                    reporter,
                    ids,
                    media,
                    element,
                    empty,
                    local,
                    &mut nodes,
                    depth + 1,
                    resolver,
                    styles,
                )
            }
            b"extLst" => Ok(false),
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;

    // A nested group's children are `GroupChild`, not `SlideNode`: the wrapper's
    // three extra fields have nowhere to live inside `WordprocessingGroup`. A
    // placeholder inside a group is not a thing PowerPoint authors — the slot is
    // a top-level concept — but a NAME is, and the selection pane shows it, so
    // each dropped name is reported.
    let mut group_children: Vec<GroupChild> = Vec::with_capacity(nodes.len());
    for node in nodes {
        if node.name.is_some() {
            reporter.degraded_attribute(&part, b"grpSp", b"name");
        }
        if node.hidden {
            reporter.degraded_attribute(&part, b"grpSp", b"hidden");
        }
        if node.placeholder.is_some() {
            reporter.degraded_attribute(&part, b"grpSp", b"ph");
        }
        // The one place `a:noFill` is still a LOSS, and it is reported with the
        // feature name it has always had: a group's children are bare
        // `GroupChild`s, so the `SlideNode` carrying the distinction is thrown away
        // here along with the name, the slot and the text. A grouped transparent
        // shape therefore still reopens filled, and the report still says so.
        if node.fill.suppresses() {
            reporter.degraded_attribute(&part, b"spPr", b"noFill");
        }
        if node.outline.suppresses() {
            reporter.degraded_attribute(&part, b"ln", b"noFill");
        }
        if node.table.is_some() {
            // A `p:graphicFrame` inside a `p:grpSp`. The frame's box survives as
            // an unpainted rectangle inside the group, which keeps the group's
            // own extent honest, but the table itself has nowhere to live: a
            // `GroupChild` carries no payload. Reported as `grpSp/tbl` for the
            // same reason grouped text is reported — a grouped table that reopens
            // empty is loss, and the group is the reason, not the table.
            reporter.omitted(&part, b"grpSp/tbl");
        }
        if node.text.is_some() {
            // A text-bearing shape inside a group: `GroupChild` can only carry
            // slide text through `GroupChild::TextBox`, which the presentation
            // model REFUSES on a slide (`PresentationError::TextBoxShapeOnSlide`)
            // because its `Vec<BlockNode>` cannot express an outline level, an
            // inline bullet or an `a:lstStyle`. So grouped text is dropped and
            // reported rather than silently downgraded into a construct the model
            // rejects. This is the sharpest gap in this importer.
            reporter.omitted(&part, b"grpSp/txBody");
        }
        group_children.push(node.content);
    }

    let group = WordprocessingGroup {
        id,
        anchor: None,
        relative_height: None,
        extent: transform.extent,
        transform: GroupTransform {
            offset: transform.offset,
            extent: transform.extent,
            child_offset: transform.child_offset,
            // A nested group with a zero child extent IS degenerate — unlike a
            // top-level `p:spTree`, there is no surface to substitute. Falling
            // back to the group's own box is the only mapping that keeps the
            // children visible, and it is reported because it is a guess.
            child_extent: if transform.child_space_present
                && transform.child_extent.width_emu > 0
                && transform.child_extent.height_emu > 0
            {
                transform.child_extent
            } else {
                reporter.degraded_attribute(&part, b"grpSpPr", b"chExt");
                transform.extent
            },
            flip_h: transform.flip_h,
            flip_v: transform.flip_v,
            rotation: transform.rotation,
        },
        // A GROUP can be clickable too, and `WordprocessingGroup` has the field, so
        // this is carried rather than reported. A linked child inside a linked group
        // keeps its own link: `GroupChild::Shape`/`Picture` each have `hyperlink`,
        // which is the one thing the `SlideNode` wrapper above does NOT have to
        // carry for them.
        hyperlink: non_visual.hyperlink,
        children: group_children,
    };
    Ok(Some(SlideNode {
        placeholder: non_visual.placeholder,
        name: non_visual.name,
        hidden: non_visual.hidden,
        // `Inherited` on both, and not because the `p:grpSpPr` was not read: it was,
        // and its fill was discarded above. A `p:grpSpPr/a:noFill` is PowerPoint's
        // boilerplate on essentially every group and it loses nothing, because a
        // group is not a painted surface — `WordprocessingGroup` has no fill field
        // and the display list emits no item for the group itself. Recording
        // `Suppressed` here would make the model claim a group was deliberately
        // transparent, which is a statement about something that is never painted.
        fill: SlidePaint::Inherited,
        outline: SlidePaint::Inherited,
        content: GroupChild::Group(Box::new(group)),
        text: None,
        // A `p:graphicFrame` inside a `p:grpSp` is read as a frame by the same
        // recursion, so a group itself never carries the table.
        table: None,
    }))
}

/// Reads a `p:grpSpPr`, which is a `p:spPr` whose `a:xfrm` also carries a child
/// coordinate space.
fn read_group_properties(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    fill: &mut FillRead,
    resolver: Resolver,
) -> Result<Transform, ImportError> {
    let part = cursor.part().to_owned();
    let mut transform = Transform::default();
    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"xfrm" => {
                transform = read_transform(cursor, element, empty)?;
                Ok(!empty)
            }
            b"effectLst" | b"effectDag" | b"scene3d" => {
                if !empty {
                    reporter.omitted(&part, local);
                }
                Ok(false)
            }
            b"extLst" => Ok(false),
            _ => read_fill_child(cursor, reporter, element, empty, fill, resolver),
        }
    })?;
    Ok(transform)
}

/// Reads one child of a `p:spPr` into `properties`.
///
/// Takes `&mut ShapeProperties` rather than returning a fresh one so the caller
/// owns the value across the whole element — which is what lets the ONE
/// post-loop "this shape stated no transform" check see an empty `<p:spPr/>` and
/// a populated `p:spPr` with no `a:xfrm` in it alike.
///
/// That single site is the point. There used to be two — one in the empty-element
/// branch and one after the loop — and both fired for an empty `<p:spPr/>`, so
/// every unpositioned placeholder was reported **twice**. A presence-only
/// assertion could not see it; the count-derived guard in `tests.rs` is what
/// caught it, and a count that must match the deck is what keeps it caught.
fn read_shape_property(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
    properties: &mut ShapeProperties,
    resolver: Resolver,
) -> Result<bool, ImportError> {
    let part = cursor.part().to_owned();
    {
        let local = local_name(element);
        match local {
            b"xfrm" => {
                properties.transform = read_transform(cursor, element, empty)?;
                Ok(!empty)
            }
            b"prstGeom" => {
                if empty {
                    return Ok(false);
                }
                read_preset_geometry(cursor, reporter, element, &mut properties.geometry)?;
                Ok(true)
            }
            b"custGeom" => {
                if empty {
                    return Ok(false);
                }
                read_custom_geometry(cursor, reporter, &mut properties.geometry)?;
                Ok(true)
            }
            b"ln" => {
                // NOT `if empty { return Ok(false) }`. `<a:ln w="12700"/>` is a
                // legal self-closing outline whose whole meaning is its `@w`, and
                // answering `default()` for it discarded the width — the sixth
                // instance of the trap `xml::enter` was written for. `read_line`
                // takes the flag and reads the attributes either way.
                properties.line = read_line(cursor, reporter, element, empty, resolver)?;
                Ok(!empty)
            }
            b"effectLst" | b"effectDag" | b"scene3d" | b"sp3d" => {
                // Self-closed is "no effects" and carries no lost meaning;
                // populated is a lost shadow, glow or reflection.
                if !empty {
                    reporter.omitted(&part, local);
                }
                Ok(false)
            }
            b"extLst" => Ok(false),
            _ => read_fill_child(
                cursor,
                reporter,
                element,
                empty,
                &mut properties.fill,
                resolver,
            ),
        }
    }
}

/// Reads an `a:xfrm`: `a:off`/`a:ext`, and `a:chOff`/`a:chExt` when present.
///
/// All four are EMU. `@rot` is in 1/60000 of a degree, clockwise about the box
/// centre — not degrees, and not radians.
fn read_transform(
    cursor: &mut Cursor<'_>,
    element: &BytesStart<'_>,
    empty: bool,
) -> Result<Transform, ImportError> {
    let part = cursor.part().to_owned();
    let mut transform = Transform {
        present: true,
        flip_h: boolean_attribute(element, b"flipH", &part)?.unwrap_or(false),
        flip_v: boolean_attribute(element, b"flipV", &part)?.unwrap_or(false),
        rotation: integer_attribute(element, b"rot", &part)?
            .and_then(|value| i32::try_from(value).ok()),
        ..Transform::default()
    };
    if empty {
        return Ok(transform);
    }
    children(cursor, |cursor, child, _child_empty| {
        match local_name(child) {
            b"off" => {
                transform.offset = PointEmu {
                    x_emu: integer_attribute(child, b"x", cursor.part())?.unwrap_or(0),
                    y_emu: integer_attribute(child, b"y", cursor.part())?.unwrap_or(0),
                };
            }
            b"ext" => {
                transform.extent = Extent {
                    width_emu: integer_attribute(child, b"cx", cursor.part())?.unwrap_or(0),
                    height_emu: integer_attribute(child, b"cy", cursor.part())?.unwrap_or(0),
                };
            }
            b"chOff" => {
                transform.child_offset = PointEmu {
                    x_emu: integer_attribute(child, b"x", cursor.part())?.unwrap_or(0),
                    y_emu: integer_attribute(child, b"y", cursor.part())?.unwrap_or(0),
                };
                transform.child_space_present = true;
            }
            b"chExt" => {
                transform.child_extent = Extent {
                    width_emu: integer_attribute(child, b"cx", cursor.part())?.unwrap_or(0),
                    height_emu: integer_attribute(child, b"cy", cursor.part())?.unwrap_or(0),
                };
                transform.child_space_present = true;
            }
            _ => {}
        }
        Ok(false)
    })?;
    Ok(transform)
}

/// Reads an `a:prstGeom` and its `a:avLst`.
///
/// A preset this build has no typed primitive for keeps its authored token in
/// `GroupShape::preset` and paints its bounding rectangle — the one token table
/// (`ShapeGeometry::from_preset_token`) decides which, so a preset cannot be
/// readable here and unwritable on export.
fn read_preset_geometry(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    geometry: &mut Geometry,
) -> Result<(), ImportError> {
    let part = cursor.part().to_owned();
    let token = attribute(element, b"prst", &part)?;
    match token.as_deref() {
        Some(token) if token.len() <= MAX_SHAPE_PRESET_BYTES => {
            match ShapeGeometry::from_preset_token(token) {
                Some(typed) => geometry.geometry = typed,
                None => {
                    geometry.geometry = ShapeGeometry::Other;
                    geometry.preset = Some(token.to_owned());
                    reporter.degraded_attribute(&part, b"prstGeom", b"prst");
                }
            }
        }
        _ => {
            reporter.invalid(&part, b"prstGeom");
        }
    }
    children(cursor, |cursor, child, child_empty| {
        if local_name(child) != b"avLst" {
            return Ok(false);
        }
        // `<a:avLst/>` self-closed is what PowerPoint writes for every unadjusted
        // preset, which is most shapes in most decks. `enter` is what makes that
        // case safe; see its documentation for the bug it exists to prevent.
        enter(cursor, child_empty, |cursor, guide, _guide_empty| {
            if local_name(guide) != b"gd" {
                return Ok(false);
            }
            if geometry.adjustments.len() >= MAX_SHAPE_ADJUSTMENTS {
                return Ok(false);
            }
            let name = attribute(guide, b"name", cursor.part())?.unwrap_or_default();
            let formula = attribute(guide, b"fmla", cursor.part())?.unwrap_or_default();
            if name.len() <= MAX_SHAPE_GUIDE_NAME_BYTES
                && formula.len() <= MAX_SHAPE_FORMULA_BYTES
                && !name.is_empty()
            {
                geometry.adjustments.push(ShapeAdjustment { name, formula });
            }
            Ok(false)
        })
    })?;
    Ok(())
}

/// Reads an `a:custGeom`'s paths into the document model's [`CustomGeometry`],
/// the type the shared geometry engine draws.
///
/// `geometry` stays [`ShapeGeometry::Other`] so nothing mistakes a freeform for a
/// preset; the custom geometry always wins over it where both are set.
///
/// Literal coordinates only. The DOCX importer reads the whole `a:custGeom`
/// grammar — guides, `a:arcTo`, handles, the text rectangle — but inside its own
/// streaming parser, where this reader cannot call it. Until that reader is lifted
/// into a crate both importers share, a guide list is reported and a path
/// containing `a:arcTo` or a guide-named coordinate is refused as a whole rather
/// than drawn with a straight segment in place of the arc, which would be a
/// different shape presented as the authored one.
fn read_custom_geometry(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    geometry: &mut Geometry,
) -> Result<(), ImportError> {
    let part = cursor.part().to_owned();
    geometry.geometry = ShapeGeometry::Other;
    let mut paths: Vec<ShapePath> = Vec::new();
    let mut refused = false;

    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"pathLst" => {
                if empty {
                    return Ok(false);
                }
                children(cursor, |cursor, path_element, path_empty| {
                    if local_name(path_element) != b"path" || path_empty {
                        return Ok(false);
                    }
                    if paths.len() >= MAX_CUSTOM_GEOMETRY_PATHS {
                        reporter.degraded(&part, b"pathLst");
                        return Ok(false);
                    }
                    let (read, path_refused) = read_path(cursor, reporter, path_element)?;
                    refused |= path_refused;
                    paths.extend(read);
                    Ok(true)
                })?;
                Ok(true)
            }
            b"avLst" | b"gdLst" | b"ahLst" | b"cxnLst" | b"rect" => {
                // A custom geometry's own guides, handles, connection sites and
                // text rectangle have no field: `ShapePath` is resolved
                // coordinates, so a formula that would have been evaluated
                // against them is already gone.
                if !empty {
                    reporter.omitted(&part, local);
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    })?;

    let custom = CustomGeometry {
        paths,
        ..CustomGeometry::default()
    };
    // The model's own check, which compiles the geometry: a geometry that will
    // not compile would paint a rectangle while claiming to be drawn.
    if refused || custom.paths.is_empty() || custom.check(&geometry.adjustments).is_err() {
        if !refused && !custom.paths.is_empty() {
            reporter.invalid(&part, b"custGeom");
        }
        geometry.path = None;
    } else {
        geometry.path = Some(custom);
    }
    Ok(())
}

/// How many `a:path`s one custom geometry may carry before the rest are reported
/// rather than read. A freeform with a counter (a letter `O`) needs two; the
/// bound exists only so a hostile file cannot grow the vector without limit.
const MAX_CUSTOM_GEOMETRY_PATHS: usize = 64;

/// Reads one `a:path`, returning it and whether an unmodelled command refused it.
fn read_path(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
) -> Result<(Option<ShapePath>, bool), ImportError> {
    let part = cursor.part().to_owned();
    // `a:path@w`/`@h` are the path's own coordinate space; zero means the
    // coordinates are absolute EMU, which is what `ShapePath` documents.
    let width_emu = integer_attribute(element, b"w", &part)?
        .filter(|value| *value > 0)
        .unwrap_or(0);
    let height_emu = integer_attribute(element, b"h", &part)?
        .filter(|value| *value > 0)
        .unwrap_or(0);
    // `@fill`, `@stroke` and `@extrusionOk` are per path: the unfilled leader of
    // a callout and the stroked outline over a filled face are two paths of one
    // freeform, and the shared engine paints each by its own switches.
    let fill = match attribute(element, b"fill", &part)? {
        None => Some(PathFill::Norm),
        Some(token) => PathFill::from_token(&token),
    };
    let stroke = boolean_attribute(element, b"stroke", &part)?;
    let extrusion_ok = boolean_attribute(element, b"extrusionOk", &part)?;
    let stated_stroke = attribute(element, b"stroke", &part)?.is_some();
    let stated_extrusion = attribute(element, b"extrusionOk", &part)?.is_some();
    let (Some(fill), true, true) = (
        fill,
        stroke.is_some() || !stated_stroke,
        extrusion_ok.is_some() || !stated_extrusion,
    ) else {
        reporter.invalid(&part, b"path");
        return Ok((None, true));
    };
    let mut commands: Vec<ShapePathCommand> = Vec::new();
    let mut refused = false;
    let mut overflowed = false;

    children(cursor, |cursor, command, command_empty| {
        let local = local_name(command);
        if local == b"close" {
            push_command(&mut commands, ShapePathCommand::Close, &mut overflowed);
            return Ok(false);
        }
        if local == b"arcTo" {
            reporter.omitted(&part, b"arcTo");
            refused = true;
            return Ok(false);
        }
        let expected = match local {
            b"moveTo" | b"lnTo" => 1_usize,
            b"quadBezTo" => 2,
            b"cubicBezTo" => 3,
            other => {
                reporter.omitted(&part, other);
                return Ok(false);
            }
        };
        if command_empty {
            reporter.invalid(&part, local);
            return Ok(false);
        }
        let mut points: Vec<GeometryPoint> = Vec::new();
        let mut refused_point = false;
        children(cursor, |cursor, point, _point_empty| {
            if local_name(point) != b"pt" {
                return Ok(false);
            }
            // A guide NAME here rather than a number is a formula reference, and
            // `integer_attribute` yields `None` for it. Treating that as zero
            // would move the vertex to the origin, so the command is refused.
            let x = integer_attribute(point, b"x", cursor.part())?;
            let y = integer_attribute(point, b"y", cursor.part())?;
            match (x, y) {
                (Some(x), Some(y)) => points.push(GeometryPoint::literal(x, y)),
                _ => refused_point = true,
            }
            Ok(false)
        })?;
        if points.len() != expected || refused_point {
            reporter.invalid(&part, local);
            refused = true;
            return Ok(true);
        }
        let command = match (local, points.as_slice()) {
            (b"moveTo", [point]) => ShapePathCommand::MoveTo {
                point: point.clone(),
            },
            (b"lnTo", [point]) => ShapePathCommand::LineTo {
                point: point.clone(),
            },
            (b"quadBezTo", [control, point]) => ShapePathCommand::QuadBezTo {
                control: control.clone(),
                point: point.clone(),
            },
            (b"cubicBezTo", [control1, control2, point]) => ShapePathCommand::CubicBezTo {
                control1: control1.clone(),
                control2: control2.clone(),
                point: point.clone(),
            },
            _ => {
                refused = true;
                return Ok(true);
            }
        };
        push_command(&mut commands, command, &mut overflowed);
        Ok(true)
    })?;

    if overflowed {
        reporter.invalid(&part, b"path");
        refused = true;
    }
    // `ShapePath` documents that the commands always start with a `MoveTo`. A
    // path that does not is malformed, and drawing from an undefined pen position
    // is how a freeform ends up as a line to the origin.
    if !matches!(commands.first(), Some(ShapePathCommand::MoveTo { .. })) {
        if !commands.is_empty() {
            reporter.invalid(&part, b"path");
        }
        return Ok((None, refused));
    }
    Ok((
        Some(ShapePath {
            width_emu,
            height_emu,
            fill,
            stroke: stroke.unwrap_or(true),
            extrusion_ok: extrusion_ok.unwrap_or(true),
            commands,
        }),
        refused,
    ))
}

fn push_command(
    commands: &mut Vec<ShapePathCommand>,
    command: ShapePathCommand,
    overflowed: &mut bool,
) {
    if commands.len() >= MAX_SHAPE_PATH_COMMANDS {
        *overflowed = true;
        return;
    }
    commands.push(command);
}

/// Reads a `p:cSld/p:bg`, having just entered it.
///
/// A background is `None` when the file states none, and that `None` means
/// **inherit** — from the layout, then the master. It must stay distinguishable
/// from an explicit white, which is why the model made it an `Option` and why
/// substituting a default here would repaint every slide in the deck.
pub(crate) fn read_background(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    resolver: Resolver,
) -> Result<Option<Fill>, ImportError> {
    let part = cursor.part().to_owned();
    let mut fill = FillRead::default();
    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"bgPr" => enter(cursor, empty, |cursor, child, child_empty| {
                read_fill_child(cursor, reporter, child, child_empty, &mut fill, resolver)
            }),
            b"bgRef" => {
                // A background style reference resolves against the theme's
                // `a:bgFillStyleLst`, which this importer does not read.
                reporter.omitted(&part, b"bgRef");
                Ok(false)
            }
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;
    Ok(fill.fill)
}
