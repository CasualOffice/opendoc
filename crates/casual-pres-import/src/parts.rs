// SPDX-License-Identifier: Apache-2.0

//! The four part readers: `ppt/presentation.xml`, `p:sld`, `p:sldLayout`,
//! `p:sldMaster`.
//!
//! # Slide order is the deck, and it comes from the relationship ids
//!
//! `p:sldIdLst` is an ordered list of `p:sldId` entries, each naming a slide by
//! its **relationship id**, not by its part name. The order of that list is the
//! order the deck is shown in, and there is nothing else that encodes it.
//!
//! Reading the order from the part names instead is the defect this module is
//! shaped to avoid, and it is not hypothetical: `ppt/slides/slide10.xml` sorts
//! before `ppt/slides/slide2.xml` in every lexical ordering, so a ten-slide deck
//! read by part name is shown in the order 1, 10, 2, 3, … . Numeric-suffix
//! parsing does not save it either, because the names are a producer convention
//! and nothing in OPC requires a slide part to be called `slideN.xml` at all — a
//! deck whose slides were reordered in PowerPoint keeps its original part names
//! and only `p:sldIdLst` changes.

use std::collections::BTreeMap;

use casual_doc_model::NodeId;
use casual_doc_model::v1::{Definitions, Fill};
// Own line (anti-conflict): the `p:defaultTextStyle` tier's type.
use casual_pres_model::ListStyle;
use casual_pres_model::{
    LayoutKind, ShapeTree, Slide, SlideId, SlideLayout, SlideLayoutId, SlideMaster, SlideMasterId,
    SlideSize, SlideSizeKind, TextStyles,
};

use crate::ImportError;
use crate::ids::Ids;
use crate::loss::Reporter;
use crate::media::MediaResolver;
use crate::opc::{PresentationPackage, Relationships};
use crate::shapes::{Surface, read_background, read_shape_tree};
// Own line (anti-conflict): the master's `p:txStyles` tiers share the
// `a:lstStyle` reader.
use crate::text::read_list_style;
use crate::xml::{Cursor, attribute, boolean_attribute, children, integer_attribute, local_name};

/// What `ppt/presentation.xml` declares.
#[derive(Clone, Debug)]
pub(crate) struct PresentationPart {
    /// The surface every slide is laid out on.
    pub(crate) slide_size: SlideSize,
    /// `p:sldIdLst`'s relationship ids, **in presentation order**.
    pub(crate) slide_relationship_ids: Vec<String>,
    /// `p:sldMasterIdLst`'s relationship ids, in declaration order.
    pub(crate) master_relationship_ids: Vec<String>,
    /// `p:defaultTextStyle`: the LAST tier of the text cascade, below every
    /// master's `p:txStyles`.
    pub(crate) default_text_style: ListStyle,
}

/// Reads `ppt/presentation.xml`.
///
/// # Complexity
///
/// O(slides + masters) — the part is a list of references and nothing else.
pub(crate) fn read_presentation_part(
    bytes: &[u8],
    part: &str,
    reporter: &mut Reporter,
    ids: &mut Ids,
    limits: crate::limits::ImportLimits,
) -> Result<PresentationPart, ImportError> {
    let mut cursor = Cursor::new(bytes, part, limits);
    cursor.root()?;
    let mut slide_size: Option<SlideSize> = None;
    let mut slide_relationship_ids: Vec<String> = Vec::new();
    let mut master_relationship_ids: Vec<String> = Vec::new();
    let mut default_text_style = ListStyle::default();

    children(&mut cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"sldSz" => {
                // The dimensions are authoritative and `@type` is a hint. The
                // model keeps the token anyway because export must write back
                // what the file said — rewriting a `letter` deck as `custom` with
                // identical dimensions is a diff in every package we touch.
                let width_emu = integer_attribute(element, b"cx", cursor.part())?;
                let height_emu = integer_attribute(element, b"cy", cursor.part())?;
                if let (Some(width_emu), Some(height_emu)) = (width_emu, height_emu) {
                    slide_size = Some(SlideSize {
                        width_emu,
                        height_emu,
                        kind: attribute(element, b"type", cursor.part())?
                            .as_deref()
                            .map_or(SlideSizeKind::Custom, SlideSizeKind::from_token),
                    });
                }
                Ok(false)
            }
            b"sldIdLst" => {
                if empty {
                    return Ok(false);
                }
                slide_relationship_ids =
                    read_id_list(cursor, reporter, b"sldId", limits.max_slides)?;
                Ok(true)
            }
            b"sldMasterIdLst" => {
                if empty {
                    return Ok(false);
                }
                master_relationship_ids =
                    read_id_list(cursor, reporter, b"sldMasterId", limits.max_masters)?;
                Ok(true)
            }
            b"notesMasterIdLst" | b"handoutMasterIdLst" => {
                // Notes and handout masters are not modelled: `Presentation` has
                // no slot for either, and `docs/156` names them as deliberately
                // absent. Reported rather than skipped silently — a deck with
                // authored speaker notes that reopens without them is loss.
                if !empty {
                    reporter.omitted(part, local);
                }
                Ok(false)
            }
            b"defaultTextStyle" => {
                // The last tier of the text cascade (shape, layout, master's
                // `p:txStyles`, THEN this). A `CT_TextListStyle` like every other
                // tier, so it goes through the one reader.
                if empty {
                    return Ok(false);
                }
                default_text_style = read_list_style(cursor, reporter, ids)?;
                Ok(true)
            }
            b"sldLayoutIdLst" | b"photoAlbum" | b"custShowLst" | b"kinsoku"
            | b"embeddedFontLst" | b"modifyVerifier" => {
                if !empty {
                    reporter.omitted(part, local);
                }
                Ok(false)
            }
            b"extLst" => Ok(false),
            other => {
                reporter.omitted(part, other);
                Ok(false)
            }
        }
    })?;

    Ok(PresentationPart {
        // Required, not defaulted: every shape's geometry is expressed on this
        // surface, so substituting 16:9 would silently reposition every shape in
        // a 4:3 deck — and a deck whose surface we guessed would render subtly
        // wrong rather than visibly refused.
        slide_size: slide_size.ok_or(ImportError::MissingSlideSize)?,
        slide_relationship_ids,
        master_relationship_ids,
        default_text_style,
    })
}

/// Reads a `p:sldIdLst`/`p:sldMasterIdLst`/`p:sldLayoutIdLst`, in order.
///
/// Order is preserved exactly — this is the one list in the package whose
/// sequence is the document's meaning rather than a convenience.
fn read_id_list(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    entry: &[u8],
    maximum: usize,
) -> Result<Vec<String>, ImportError> {
    let part = cursor.part().to_owned();
    let mut ids = Vec::new();
    children(cursor, |cursor, element, _empty| {
        let local = local_name(element);
        if local != entry {
            reporter.omitted(&part, local);
            return Ok(false);
        }
        if ids.len() >= maximum {
            reporter.invalid(&part, local);
            return Ok(false);
        }
        // A `p:sldId` carries BOTH `id="256"` (a producer-scoped numeric slide id)
        // and `r:id="rId2"` (the relationship). Only the relationship names a
        // part, so the prefixed form is matched explicitly; see
        // `relationship_id_of` for why a local-name match cannot be used here.
        if let Some(relationship_id) = relationship_id_of(element, cursor.part())? {
            ids.push(relationship_id);
        } else {
            reporter.invalid(&part, local);
        }
        Ok(false)
    })?;
    Ok(ids)
}

/// An element's `r:id`, distinguished from its unprefixed `id`.
///
/// `attribute` matches on the local name, and `r:id`'s local name *is* `id` — so
/// on a `p:sldId`, which carries both `id="256"` and `r:id="rId2"`, a local-name
/// match would return whichever came first. This is the one place in the importer
/// where the namespace prefix is load-bearing, so it is matched explicitly.
fn relationship_id_of(
    element: &quick_xml::events::BytesStart<'_>,
    part: &str,
) -> Result<Option<String>, ImportError> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| ImportError::MalformedPartXml {
            part: part.to_owned(),
        })?;
        let key = attribute.key.into_inner();
        // The prefix a producer binds to the relationships namespace is
        // conventionally `r`, and a package that binds another one is vanishingly
        // rare; what matters is that an UNPREFIXED `id` is never accepted here.
        let prefixed = key.contains(&b':');
        if !prefixed || attribute.key.local_name().as_ref() != b"id" {
            continue;
        }
        let raw = core::str::from_utf8(attribute.value.as_ref()).map_err(|_| {
            ImportError::MalformedPartXml {
                part: part.to_owned(),
            }
        })?;
        if raw.is_empty() {
            continue;
        }
        return Ok(Some(raw.to_owned()));
    }
    Ok(None)
}

/// A part's `p:cSld`: its name, its background, and its shape tree.
#[derive(Debug)]
pub(crate) struct CommonSlideData {
    pub(crate) name: Option<String>,
    pub(crate) background: Option<Fill>,
    pub(crate) shapes: ShapeTree,
    /// The master's `p:txStyles`, which is a SIBLING of `p:cSld` rather than a
    /// child of it, and is therefore empty for a layout or a slide. Carried out
    /// through the same struct because `read_root_children` is what walks both and
    /// a second return value would thread an always-empty tier set through two
    /// readers that cannot use it.
    pub(crate) text_styles: TextStyles,
}

/// Reads a slide, layout or master part's `p:cSld`, plus the root attributes the
/// caller asks for.
///
/// One reader for all three, because `p:cSld` is literally the same element in
/// each: a name, an optional background, and an ordered shape tree. Writing three
/// would be three things to keep in step, and `SKILL` §8 is explicit that a
/// parallel path is evidence the abstraction is wrong.
fn read_common_slide_data(
    cursor: &mut Cursor<'_>,
    element: &quick_xml::events::BytesStart<'_>,
    reporter: &mut Reporter,
    ids: &mut Ids,
    media: &mut MediaResolver<'_>,
    surface: Surface,
) -> Result<CommonSlideData, ImportError> {
    let part = cursor.part().to_owned();
    // `p:cSld@name` is the author-visible name: the slide's own, or the one the
    // layout gallery shows ("Title and Content"). An empty attribute is the same
    // as none, so it does not become a slide called "".
    let name = attribute(element, b"name", &part)?.filter(|name| !name.is_empty());
    let mut background = None;
    let mut shapes: Option<ShapeTree> = None;

    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"bg" => {
                if empty {
                    return Ok(false);
                }
                background = read_background(cursor, reporter)?;
                Ok(true)
            }
            b"spTree" => {
                if empty {
                    return Ok(false);
                }
                shapes = Some(read_shape_tree(cursor, reporter, ids, media, surface)?);
                Ok(true)
            }
            b"custDataLst" | b"controls" | b"extLst" => Ok(false),
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;

    let shapes = match shapes {
        Some(shapes) => shapes,
        // A part with no `p:spTree` is malformed — every `p:cSld` has one, even
        // for a blank slide. An empty tree keeps the deck openable, and the fact
        // is reported.
        None => {
            reporter.invalid(&part, b"spTree");
            ShapeTree::empty(ids.next()?, surface.width_emu, surface.height_emu)
        }
    };
    Ok(CommonSlideData {
        name,
        background,
        shapes,
        // `p:txStyles` is a sibling of `p:cSld`, not a child, so this reader never
        // sees one. `read_root_children` substitutes what it read.
        text_styles: TextStyles::default(),
    })
}

/// Reads one slide part (`p:sld`).
pub(crate) fn read_slide(
    bytes: &[u8],
    part: &str,
    layout: SlideLayoutId,
    relationships: &Relationships,
    context: &mut PartContext<'_>,
    surface: Surface,
) -> Result<Slide, ImportError> {
    let (root_hidden, common) =
        read_part_with_common(bytes, part, relationships, context, surface)?;
    Ok(Slide {
        id: SlideId::new(context.ids.next()?),
        layout,
        shapes: common.shapes,
        name: common.name,
        // The model stores `hidden` rather than mirroring `p:sld@show`'s polarity
        // so the default is `false` and an absent attribute needs no special
        // case. A hidden slide is retained, not dropped: it is still in the deck,
        // still edited and still exported.
        hidden: root_hidden,
        background: common.background,
    })
}

/// Reads one layout part (`p:sldLayout`).
pub(crate) fn read_layout(
    bytes: &[u8],
    part: &str,
    master: SlideMasterId,
    relationships: &Relationships,
    context: &mut PartContext<'_>,
    surface: Surface,
    id: NodeId,
) -> Result<SlideLayout, ImportError> {
    let mut cursor = Cursor::new(bytes, part, context.limits);
    let root = cursor.root()?;
    // `p:sldLayout@type` is a hint, not a behaviour: the layout's authority is
    // its own shape tree. It is modelled because export must write back what the
    // file said, and because the "New Slide" gallery groups by it — a deck whose
    // layouts all read `cust` loses its gallery even though every slide still
    // renders correctly.
    let kind = attribute(&root, b"type", part)?
        .as_deref()
        .map_or(LayoutKind::Custom, LayoutKind::from_token);
    let common = read_root_children(&mut cursor, part, relationships, context, surface)?;
    Ok(SlideLayout {
        id: SlideLayoutId::new(id),
        master,
        kind,
        shapes: common.shapes,
        name: common.name,
        background: common.background,
    })
}

/// Reads one master part (`p:sldMaster`).
pub(crate) fn read_master(
    bytes: &[u8],
    part: &str,
    relationships: &Relationships,
    context: &mut PartContext<'_>,
    surface: Surface,
    id: NodeId,
) -> Result<SlideMaster, ImportError> {
    let (_, common) = read_part_with_common(bytes, part, relationships, context, surface)?;
    Ok(SlideMaster {
        id: SlideMasterId::new(id),
        shapes: common.shapes,
        name: common.name,
        background: common.background,
        text_styles: common.text_styles,
    })
}

/// The per-import state a part reader needs.
#[derive(Debug)]
pub(crate) struct PartContext<'a> {
    pub(crate) reporter: &'a mut Reporter,
    pub(crate) ids: &'a mut Ids,
    pub(crate) definitions: &'a mut Definitions,
    pub(crate) registered_media: &'a mut BTreeMap<String, NodeId>,
    pub(crate) admitted: &'a BTreeMap<String, Option<String>>,
    pub(crate) limits: crate::limits::ImportLimits,
}

/// Reads a part's root element, returning its `show="0"` flag and its `p:cSld`.
fn read_part_with_common(
    bytes: &[u8],
    part: &str,
    relationships: &Relationships,
    context: &mut PartContext<'_>,
    surface: Surface,
) -> Result<(bool, CommonSlideData), ImportError> {
    let mut cursor = Cursor::new(bytes, part, context.limits);
    let root = cursor.root()?;
    let hidden = boolean_attribute(&root, b"show", part)?.is_some_and(|show| !show);
    let common = read_root_children(&mut cursor, part, relationships, context, surface)?;
    Ok((hidden, common))
}

/// Reads a slide/layout/master root's children.
fn read_root_children(
    cursor: &mut Cursor<'_>,
    part: &str,
    relationships: &Relationships,
    context: &mut PartContext<'_>,
    surface: Surface,
) -> Result<CommonSlideData, ImportError> {
    let mut common: Option<CommonSlideData> = None;
    let mut text_styles = TextStyles::default();
    let PartContext {
        reporter,
        ids,
        definitions,
        registered_media,
        admitted,
        ..
    } = context;
    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"cSld" => {
                if empty {
                    return Ok(false);
                }
                let mut media = MediaResolver::new(
                    relationships,
                    part,
                    registered_media,
                    definitions,
                    admitted,
                );
                common = Some(read_common_slide_data(
                    cursor, element, reporter, ids, &mut media, surface,
                )?);
                Ok(true)
            }
            b"clrMap" | b"clrMapOvr" => {
                // The colour map binds the twelve `a:schemeClr` slots to the
                // theme's. Not modelled, and it is why a scheme colour is
                // reported rather than resolved: without the map a `tx1` could be
                // either of two theme entries.
                reporter.omitted(part, local);
                Ok(false)
            }
            b"transition" | b"timing" => {
                // `docs/156` §8 makes byte-faithful retention of `p:transition`
                // and `p:timing` a Tier 2 import/export requirement, NOT a Tier 4
                // nicety — because silent loss is forbidden and a deck whose
                // animations vanish is what ends an evaluation. There is no
                // retention side-table on the presentation path yet, so this is
                // reported as outright omitted. That is the honest reading today
                // and it is the gap that matters most in this file.
                reporter.omitted(part, local);
                Ok(false)
            }
            b"txStyles" => {
                // The master's three text-style tiers (title, body, other), each a
                // nine-level `CT_TextListStyle` — the cascade's largest single
                // contribution, and the reason a slide's runs have a resolvable
                // font size at all. Only on a MASTER: `p:txStyles` is not a child
                // of `p:sldLayout` or `p:sld`, so a layout carrying one is
                // malformed and keeps falling through to the report.
                if empty {
                    return Ok(false);
                }
                text_styles = read_text_styles(cursor, reporter, ids)?;
                Ok(true)
            }
            b"hf" => {
                reporter.omitted(part, local);
                Ok(false)
            }
            b"sldLayoutIdLst" | b"extLst" => Ok(false),
            other => {
                reporter.omitted(part, other);
                Ok(false)
            }
        }
    })?;
    match common {
        Some(common) => Ok(CommonSlideData {
            text_styles,
            ..common
        }),
        None => {
            reporter.invalid(part, b"cSld");
            Ok(CommonSlideData {
                name: None,
                background: None,
                shapes: ShapeTree::empty(ids.next()?, surface.width_emu, surface.height_emu),
                // A part with no `p:cSld` is already being reported as invalid;
                // whatever `p:txStyles` it carried is not worth salvaging onto a
                // master with no shapes.
                text_styles: TextStyles::default(),
            })
        }
    }
}

/// Reads a master's `p:txStyles` into the three tiers.
///
/// Each tier is a `CT_TextListStyle`, the same grammar `a:lstStyle` uses, so this
/// delegates to the one reader rather than growing a second. `p:titleStyle`,
/// `p:bodyStyle` and `p:otherStyle` are the only children ECMA-376 allows; an
/// `p:extLst` is scaffolding and anything else is reported.
///
/// O(levels), bounded at nine per tier by `ListStyle::validate`.
fn read_text_styles(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    ids: &mut Ids,
) -> Result<TextStyles, ImportError> {
    let part = cursor.part().to_owned();
    let mut styles = TextStyles::default();
    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        let tier = match local {
            b"titleStyle" => &mut styles.title,
            b"bodyStyle" => &mut styles.body,
            b"otherStyle" => &mut styles.other,
            b"extLst" => return Ok(false),
            other => {
                reporter.omitted(&part, other);
                return Ok(false);
            }
        };
        // An empty tier states nothing, which is what `TextStyles::default` already
        // holds — and `enter`ing a self-closing element would consume its sibling's
        // events as its children.
        if empty {
            return Ok(false);
        }
        *tier = read_list_style(cursor, reporter, ids)?;
        Ok(true)
    })?;
    Ok(styles)
}

/// Resolves one `r:id` against a part's relationships to an admitted part name.
pub(crate) fn resolve_part(
    package: &PresentationPackage<'_>,
    relationships: &Relationships,
    source_part: &str,
    relationship_id: &str,
) -> Result<String, ImportError> {
    let unresolved = || ImportError::UnresolvedRelationship {
        part: source_part.to_owned(),
        relationship_id: relationship_id.to_owned(),
    };
    let relationship = relationships.get(relationship_id).ok_or_else(unresolved)?;
    let resolved = relationship
        .resolved_part
        .as_deref()
        .ok_or_else(unresolved)?;
    if !package.contains_part(resolved) {
        return Err(unresolved());
    }
    Ok(resolved.to_owned())
}

/// The first relationship of a given type, with its resolved part name.
pub(crate) fn first_of_type(
    relationships: &Relationships,
    relationship_type: &str,
) -> Option<String> {
    relationships
        .values()
        .find(|relationship| relationship.relationship_type == relationship_type)
        .and_then(|relationship| relationship.resolved_part.clone())
}

/// Every relationship of a given type, with its resolved part, ordered by
/// relationship id.
pub(crate) fn all_of_type(relationships: &Relationships, relationship_type: &str) -> Vec<String> {
    relationships
        .values()
        .filter(|relationship| relationship.relationship_type == relationship_type)
        .filter_map(|relationship| relationship.resolved_part.clone())
        .collect()
}
