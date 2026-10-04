// SPDX-License-Identifier: Apache-2.0

//! PresentationML semantic export: a [`Presentation`] back out as a `.pptx`.
//!
//! # Why this exists, and what it changes about the fidelity report
//!
//! `casual-pres-import` opens a real deck, and until this crate there was no
//! writer at all — so **no finding in a deck's compatibility report could claim
//! `preserved`**. `casual_doc_loss::SourceRetention::Snapshot` licenses that claim
//! from a verbatim byte floor, and with nothing to write the bytes back there was
//! no floor to point at. The import lane says so explicitly and stamps every entry
//! `Regenerated`.
//!
//! This crate does not change that stamp, and the distinction is worth stating
//! plainly rather than blurring: a writer gives **round-trip fidelity on the
//! semantic path** — a construct the model carries survives write then reopen —
//! which is what the DOCX side asserts with `m1 == m2`. It does not give verbatim
//! retention, because nothing here keeps the source bytes. A `preserved` claim
//! still needs a retention side table on the presentation path, and that is a
//! separate piece of work.
//!
//! What it does buy immediately: a construct the model holds is no longer
//! destroyed by a save. Before this, opening and saving a deck lost everything,
//! which made the whole import lane read-only in practice.
//!
//! # What it writes
//!
//! The OPC container and its relationship graph; `[Content_Types].xml`;
//! `ppt/presentation.xml` with `p:sldSz`, `p:sldMasterIdLst`, `p:sldIdLst` and
//! `p:defaultTextStyle`; every master with its `p:sldLayoutIdLst` and
//! `p:txStyles`; every layout with its `@type`; every slide with `p:sld@show`;
//! and for each of the three tiers a `p:cSld` carrying the name, a solid `p:bg`
//! and the shape tree — `p:sp` with `a:xfrm`, `a:prstGeom` + `a:avLst` or
//! `a:custGeom`, a solid fill and `a:ln`, `p:nvPr/p:ph` with its type/idx/sz/orient,
//! nested `p:grpSp`, `p:pic` with an `a:blip`, and `a:txBody` in full — `a:bodyPr`,
//! the nine-level `a:lstStyle`, and paragraphs with `a:pPr`, `a:r`/`a:rPr`, `a:br`
//! and `a:fld`.
//!
//! # What it does NOT write, stated as families rather than successes
//!
//! Nothing the model does not carry can be written, so this list is the model's
//! gap list seen from the other side: the theme part (so a written deck has no
//! `a:theme` and a reader of it resolves no `a:schemeClr`), `p:clrMap`,
//! notes and handout masters, `p:transition`, `p:timing`, `p:graphicFrame` — so a
//! table, chart or diagram is not written because it never arrived — gradient,
//! picture and pattern fills on a slide shape, effects, and `p14:sectionLst`.
//!
//! A deck that imported with a non-empty report therefore exports without the
//! constructs that report named. That is the honest composition of two lanes, not
//! a defect in either.
//!
//! # Media
//!
//! An image part's BYTES are not in the model — `Definitions::media` carries a
//! part name and a relationship id, not the content. So [`export_pptx`] takes the
//! bytes from the caller and **writes no `a:blip` at all** for a picture whose
//! bytes were not supplied, rather than emitting a relationship that points at a
//! part the package does not contain. A dangling reference is a package a reader
//! refuses; a missing picture is a package it opens. The DOCX writer answers this
//! the same way, under `109` FID-R-06.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod opc;
mod shapes;
mod text;

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::io::{Cursor, Read as _};

use casual_pres_model::{Presentation, SlideLayout, SlideMaster};

use opc::{ContentTypes, Parts, Relationships, content_type, rel, rels_name};

#[cfg(test)]
mod tests;

/// A deck-writing failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExportError {
    /// The ZIP container could not be assembled.
    Package,
    /// An XML writer failed, which for an in-memory buffer means a bug here
    /// rather than an I/O condition.
    Xml,
    /// Two parts were assigned one name. A writer bug, refused rather than
    /// silently overwriting: one of the two would be unreachable.
    DuplicatePart(String),
    /// A slide names a layout the deck does not contain, or a layout names a
    /// missing master. The model validates its references, so this is reachable
    /// only for a `Presentation` built by hand.
    DanglingReference,
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Package => write!(formatter, "the package could not be assembled"),
            Self::Xml => write!(formatter, "a part could not be serialized"),
            Self::DuplicatePart(name) => {
                write!(formatter, "two parts were assigned the name {name}")
            }
            Self::DanglingReference => {
                write!(
                    formatter,
                    "a slide or layout references a part not in the deck"
                )
            }
        }
    }
}

impl Error for ExportError {}

/// The original package's parts, carried through a save unchanged.
///
/// # Why this is the structural piece, not a convenience
///
/// `casual-doc-loss::SourceRetention::Snapshot` licenses a `preserved` claim from a
/// **verbatim byte floor**: the bytes the import came from, still available at
/// write time. Without one, every finding in a deck's report reads `Regenerated`
/// and a construct the model does not carry is destroyed by the first save —
/// a deck's theme, its transitions, its animation, its notes masters, all gone.
///
/// That is the difference between direct OOXML and a converter, and it is only an
/// advantage if the bytes actually survive. So a caller that still holds the
/// package it imported hands it here, and every part this writer does NOT
/// regenerate is written back exactly as it arrived.
///
/// # The rule, and why it is this way round
///
/// A regenerated part WINS. The model is the authority on anything it carries, so
/// a slide the user edited must be written from the model even though a retained
/// copy of the original exists. Retention fills the gaps; it never overrides.
/// Getting this backwards would make every edit invisible, which is a worse failure
/// than the one retention fixes.
#[derive(Clone, Debug, Default)]
pub struct RetainedParts {
    /// Package-relative part name to its original bytes.
    pub parts: BTreeMap<String, Vec<u8>>,
}

impl RetainedParts {
    /// Reads every part of an original `.pptx` so it can be carried through a save.
    ///
    /// # Errors
    ///
    /// [`ExportError::Package`] when `bytes` is not a readable ZIP container. No
    /// bound is applied here: the caller is handing back a package it already
    /// admitted through `casual-doc-package`'s bounded reader, and re-deriving a
    /// limit would be a second, weaker gate in front of the same bytes.
    pub fn from_package(bytes: &[u8]) -> Result<Self, ExportError> {
        let mut archive =
            zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| ExportError::Package)?;
        let mut parts = BTreeMap::new();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|_| ExportError::Package)?;
            // A directory entry has no content and must not become a zero-byte
            // part: OPC has no directories, and writing one produces a package
            // some readers reject.
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_owned();
            let mut body = Vec::new();
            entry
                .read_to_end(&mut body)
                .map_err(|_| ExportError::Package)?;
            parts.insert(name, body);
        }
        Ok(Self { parts })
    }
}

/// Writes `presentation` as a `.pptx` package.
///
/// `media` maps a media part name — the same `MediaReference::part_name` the
/// importer recorded — to its bytes. A picture whose bytes are absent is written
/// without its `a:blip`; see the module documentation for why that beats a
/// dangling relationship.
///
/// Nothing is retained. Use [`export_pptx_retaining`] where the original package
/// is still to hand — that is the call that keeps a deck's theme, transitions and
/// animation through a save, and it is the one an application should make.
///
/// # Complexity
///
/// O(deck): one pass over every master, layout and slide, and one pass over each
/// shape tree. Not for a keystroke.
///
/// # Errors
///
/// [`ExportError`] when the container cannot be assembled, a part fails to
/// serialize, or a hand-built `Presentation` carries a dangling tier reference.
pub fn export_pptx(
    presentation: &Presentation,
    media: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, ExportError> {
    export_pptx_retaining(presentation, media, &RetainedParts::default())
}

/// Writes `presentation`, carrying through every part of the original package this
/// writer does not regenerate.
///
/// This is the call that makes a deck survive a round trip rather than merely
/// reopen: the theme part, `p:transition`, `p:timing`, notes and handout masters
/// and `p:graphicFrame`'s own parts are all constructs the model does not carry,
/// and all of them are written back byte-for-byte.
///
/// A regenerated part wins over a retained one — see [`RetainedParts`] for why that
/// direction is the only safe one.
///
/// # What it does NOT make true
///
/// A retained part is not a *validated* retention record. Nothing here checks that
/// the carried part is still consistent with the model beside it — a retained
/// `p:sldLayoutIdLst` in a theme override could in principle name a layout the
/// model no longer has. Carrying the bytes is strictly better than destroying them,
/// and the ledger that would license a `preserved` CLAIM needs that consistency
/// check; it is named here rather than assumed.
///
/// # Errors
///
/// As [`export_pptx`].
pub fn export_pptx_retaining(
    presentation: &Presentation,
    media: &BTreeMap<String, Vec<u8>>,
    retained: &RetainedParts,
) -> Result<Vec<u8>, ExportError> {
    let mut parts = Parts::default();
    let mut types = ContentTypes::default();
    types.default_for("rels", content_type::RELATIONSHIPS);
    types.default_for("xml", "application/xml");

    // Part names are assigned by POSITION in the model's own ordering, one-based
    // like every producer writes them. The order is what `p:sldIdLst` records, so
    // naming by position keeps a written deck's part names in the same sequence a
    // reader sees — which is not required, but makes a package a human opens
    // comprehensible.
    let master_names: Vec<String> = (1..=presentation.masters().len())
        .map(|index| format!("ppt/slideMasters/slideMaster{index}.xml"))
        .collect();
    let layout_names: Vec<String> = (1..=presentation.layouts().len())
        .map(|index| format!("ppt/slideLayouts/slideLayout{index}.xml"))
        .collect();
    let slide_names: Vec<String> = (1..=presentation.slides().len())
        .map(|index| format!("ppt/slides/slide{index}.xml"))
        .collect();

    // The presentation part's relationships, and the ids the id lists must quote.
    let mut presentation_rels = Relationships::default();
    let master_ids: Vec<String> = master_names
        .iter()
        .map(|name| presentation_rels.add(rel::SLIDE_MASTER, &relative_target("ppt", name)))
        .collect();
    let slide_ids: Vec<String> = slide_names
        .iter()
        .map(|name| presentation_rels.add(rel::SLIDE, &relative_target("ppt", name)))
        .collect();

    parts.add(
        "[Content_Types].xml",
        // Placeholder: the real content-types part is written last, once every
        // override is known, and reserving the name here keeps the retention pass
        // from carrying the original's copy.
        //
        // What actually GUARANTEES the rebuild is the `replace` at the end of this
        // function, which runs after retention and overwrites whatever is at this
        // name. Measured rather than assumed: removing this reservation leaves the
        // content-types guard green. So the reservation is for clarity and for one
        // skip, not for correctness, and this comment says so instead of claiming
        // a load it does not carry.
        Vec::new(),
    )?;
    parts.add("_rels/.rels", root_relationships())?;

    parts.add(
        "ppt/presentation.xml",
        presentation_part(presentation, &master_ids, &slide_ids)?,
    )?;
    types.override_for("ppt/presentation.xml", content_type::PRESENTATION);
    parts.add(
        &rels_name("ppt/presentation.xml"),
        presentation_rels.to_xml(),
    )?;

    for (index, master) in presentation.masters().iter().enumerate() {
        let name = &master_names[index];
        let mut rels = Relationships::default();
        // A master offers the layouts that name it, in the deck's layout order.
        let mut layout_ids = Vec::new();
        for (position, layout) in presentation.layouts().iter().enumerate() {
            if layout.master == master.id {
                layout_ids.push(rels.add(
                    rel::SLIDE_LAYOUT,
                    &relative_target("ppt/slideMasters", &layout_names[position]),
                ));
            }
        }
        let context = shapes::ShapeContext {
            definitions: presentation.definitions(),
            media,
            part_name: name,
        };
        let body = master_part(master, &layout_ids, &context, &mut rels)?;
        parts.add(name, body)?;
        types.override_for(name, content_type::SLIDE_MASTER);
        if !rels.is_empty() {
            parts.add(&rels_name(name), rels.to_xml())?;
        }
    }

    for (index, layout) in presentation.layouts().iter().enumerate() {
        let name = &layout_names[index];
        let master_position = presentation
            .masters()
            .iter()
            .position(|master| master.id == layout.master)
            .ok_or(ExportError::DanglingReference)?;
        let mut rels = Relationships::default();
        // Every layout names its master. Written first so it is `rId1`, which is
        // the convention every producer follows and the one a reader is most
        // likely to have been tested against.
        rels.add(
            rel::SLIDE_MASTER,
            &relative_target("ppt/slideLayouts", &master_names[master_position]),
        );
        let context = shapes::ShapeContext {
            definitions: presentation.definitions(),
            media,
            part_name: name,
        };
        let body = layout_part(layout, &context, &mut rels)?;
        parts.add(name, body)?;
        types.override_for(name, content_type::SLIDE_LAYOUT);
        parts.add(&rels_name(name), rels.to_xml())?;
    }

    for (index, slide) in presentation.slides().iter().enumerate() {
        let name = &slide_names[index];
        let layout_position = presentation
            .layouts()
            .iter()
            .position(|layout| layout.id == slide.layout)
            .ok_or(ExportError::DanglingReference)?;
        let mut rels = Relationships::default();
        rels.add(
            rel::SLIDE_LAYOUT,
            &relative_target("ppt/slides", &layout_names[layout_position]),
        );
        let context = shapes::ShapeContext {
            definitions: presentation.definitions(),
            media,
            part_name: name,
        };
        let body = shapes::slide_part(slide, &context, &mut rels)?;
        parts.add(name, body)?;
        types.override_for(name, content_type::SLIDE);
        parts.add(&rels_name(name), rels.to_xml())?;
    }

    // Image parts, once each, for every media reference whose bytes the caller
    // supplied. Written after the tiers so the relationship ids are already minted.
    for (part_name, bytes) in media {
        if parts.add(part_name, bytes.clone()).is_err() {
            continue;
        }
        if let Some(extension) = part_name.rsplit_once('.').map(|(_, extension)| extension) {
            types.default_for(extension, image_content_type(extension));
        }
    }

    // Retention: every original part this writer did not produce, carried through
    // unchanged. `Parts::add` refuses a duplicate, so a regenerated part wins
    // simply by having been written first — which is the direction `RetainedParts`
    // documents and the only one under which an edit stays visible.
    //
    // One test, not two: a part this writer produced is already in `parts`, and
    // that includes `[Content_Types].xml`, whose name is reserved at the top of
    // this function. A retained copy of it would declare the original's part names
    // rather than the written ones, and a reader resolves a slide's type through
    // it — though the `replace` after this loop is what makes the rebuild
    // certain.
    //
    // The same test covers relationship parts: one this writer regenerated is
    // skipped, because carrying the original's would reinstate ids that now point
    // nowhere; one belonging to a part this writer does NOT produce is retained,
    // which is what keeps a notes slide's own references intact.
    for (name, bytes) in &retained.parts {
        if parts.contains(name) {
            continue;
        }
        parts.add(name, bytes.clone())?;
        if let Some(extension) = name.rsplit_once('.').map(|(_, extension)| extension) {
            // The original's own content type, which is the only source of truth
            // for a part this writer knows nothing about. An extension default is
            // used rather than an override because the retained
            // `[Content_Types].xml` is not consulted — stated as a limit rather
            // than a feature: a retained part whose type came from an OVERRIDE
            // keyed to its exact name will be declared by extension instead, which
            // is correct for the `.xml`, `.png` and `.thmx` families a deck
            // actually carries and would be wrong for a part that shares an
            // extension with a differently-typed sibling.
            types.default_for(extension, retained_content_type(extension));
        }
    }

    // The content types part, now that every override is known. Replacing the
    // reserved entry rather than adding one, so `Parts::add`'s duplicate refusal
    // still covers every other name.
    parts.replace("[Content_Types].xml", types.to_xml());
    parts.into_package()
}

/// `_rels/.rels`: the package root pointing at the presentation part.
fn root_relationships() -> Vec<u8> {
    let mut rels = Relationships::default();
    rels.add(rel::OFFICE_DOCUMENT, "ppt/presentation.xml");
    rels.to_xml()
}

/// A target expressed relative to `source_folder`, which is the form every
/// producer writes.
///
/// `ppt/slides` -> `ppt/slideLayouts/slideLayout1.xml` becomes
/// `../slideLayouts/slideLayout1.xml`. A target written absolute would resolve for
/// some readers and not others, so the relative form is the only one emitted.
fn relative_target(source_folder: &str, target: &str) -> String {
    let source: Vec<&str> = source_folder.split('/').filter(|s| !s.is_empty()).collect();
    let full: Vec<&str> = target.split('/').collect();
    let shared = source
        .iter()
        .zip(full.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let ups = source.len() - shared;
    let mut out = String::new();
    for _ in 0..ups {
        out.push_str("../");
    }
    out.push_str(&full[shared..].join("/"));
    out
}

/// The content type for a retained part's extension.
///
/// Images resolve through [`image_content_type`]; everything else is `application/xml`,
/// because every remaining part a PresentationML package carries — the theme, the
/// presentation properties, the view properties, the table styles — is XML. An
/// extension this build has never seen falls through to the image table's
/// `application/octet-stream`, which a reader refuses VISIBLY rather than
/// mis-decoding.
fn retained_content_type(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        "xml" => "application/xml",
        "rels" => content_type::RELATIONSHIPS,
        other => image_content_type(other),
    }
}

/// The content type for an image extension.
///
/// A small table rather than a guess: an extension this build does not know is
/// declared `application/octet-stream`, which a reader will refuse to decode —
/// visibly, rather than by rendering nothing and saying nothing.
fn image_content_type(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "emf" => "image/x-emf",
        "wmf" => "image/x-wmf",
        _ => "application/octet-stream",
    }
}

/// `ppt/presentation.xml`.
fn presentation_part(
    presentation: &Presentation,
    master_ids: &[String],
    slide_ids: &[String],
) -> Result<Vec<u8>, ExportError> {
    let size = presentation.slide_size();
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">"#,
    );
    // `p:sldMasterIdLst` precedes `p:sldIdLst` in `CT_Presentation`, and the
    // `@id` values are producer-scoped tokens: the 2147483648 base is what
    // PowerPoint writes and carries no meaning beyond uniqueness.
    if !master_ids.is_empty() {
        xml.push_str("<p:sldMasterIdLst>");
        for (index, id) in master_ids.iter().enumerate() {
            let token = 2_147_483_648_u64 + index as u64;
            xml.push_str(&format!(r#"<p:sldMasterId id="{token}" r:id="{id}"/>"#));
        }
        xml.push_str("</p:sldMasterIdLst>");
    }
    if !slide_ids.is_empty() {
        xml.push_str("<p:sldIdLst>");
        for (index, id) in slide_ids.iter().enumerate() {
            // Slide `@id` starts at 256, which is the floor `ST_SlideId` allows.
            let token = 256_u32 + index as u32;
            xml.push_str(&format!(r#"<p:sldId id="{token}" r:id="{id}"/>"#));
        }
        xml.push_str("</p:sldIdLst>");
    }
    xml.push_str(&format!(
        r#"<p:sldSz cx="{}" cy="{}" type="{}"/>"#,
        size.width_emu,
        size.height_emu,
        size.kind.token()
    ));
    // `p:notesSz` is required by the schema and the model does not carry one, so
    // the slide surface is written transposed — which is what PowerPoint writes for
    // a deck with no notes master, and is the only value that cannot misrepresent
    // an authored one.
    xml.push_str(&format!(
        r#"<p:notesSz cx="{}" cy="{}"/>"#,
        size.height_emu, size.width_emu
    ));
    let default_text_style = text::list_style_xml(presentation.default_text_style(), "a:lvl");
    if !default_text_style.is_empty() {
        xml.push_str("<p:defaultTextStyle>");
        xml.push_str(&default_text_style);
        xml.push_str("</p:defaultTextStyle>");
    }
    xml.push_str("</p:presentation>");
    Ok(xml.into_bytes())
}

/// One `ppt/slideMasters/slideMasterN.xml`.
fn master_part(
    master: &SlideMaster,
    layout_ids: &[String],
    context: &shapes::ShapeContext<'_>,
    rels: &mut Relationships,
) -> Result<Vec<u8>, ExportError> {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">"#,
    );
    xml.push_str(&shapes::common_slide_data(
        master.name.as_deref(),
        master.background.as_ref(),
        &master.shapes,
        context,
        rels,
    )?);
    // `p:clrMap` is REQUIRED on a master by `CT_SlideMaster`, and the model does
    // not carry one — the importer reports it as a loss for exactly that reason.
    // The identity mapping is written: it is the only choice that does not invent
    // a binding, and it is what PowerPoint writes for an unmodified theme.
    xml.push_str(
        r#"<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>"#,
    );
    if !layout_ids.is_empty() {
        xml.push_str("<p:sldLayoutIdLst>");
        for (index, id) in layout_ids.iter().enumerate() {
            let token = 2_147_483_649_u64 + index as u64;
            xml.push_str(&format!(r#"<p:sldLayoutId id="{token}" r:id="{id}"/>"#));
        }
        xml.push_str("</p:sldLayoutIdLst>");
    }
    let tiers = text::text_styles_xml(&master.text_styles);
    if !tiers.is_empty() {
        xml.push_str(&tiers);
    }
    xml.push_str("</p:sldMaster>");
    Ok(xml.into_bytes())
}

/// One `ppt/slideLayouts/slideLayoutN.xml`.
fn layout_part(
    layout: &SlideLayout,
    context: &shapes::ShapeContext<'_>,
    rels: &mut Relationships,
) -> Result<Vec<u8>, ExportError> {
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="{}">"#,
        layout.kind.token()
    );
    xml.push_str(&shapes::common_slide_data(
        layout.name.as_deref(),
        layout.background.as_ref(),
        &layout.shapes,
        context,
        rels,
    )?);
    xml.push_str("</p:sldLayout>");
    Ok(xml.into_bytes())
}
