// SPDX-License-Identifier: Apache-2.0

//! The import driver: admit the package, read the three part tiers in dependency
//! order, wire the references, validate, and report.
//!
//! # Why masters and layouts are read before slides
//!
//! A `Slide` holds a `SlideLayoutId` and a `SlideLayout` holds a
//! `SlideMasterId`, and `Presentation::validate` refuses a reference that does
//! not resolve. So the three tiers are read bottom-up — masters, then each
//! master's layouts, then the slides in `p:sldIdLst` order — and each tier's ids
//! are known before the tier that names them is built. Reading slides first
//! would mean either a second pass to patch the references or a placeholder id
//! that is briefly wrong, and ONLYOFFICE independently does the same thing:
//! parse every part into flat arrays first, wire the pointers in a separate pass.
//!
//! # What this importer reads, in one list
//!
//! Enumerating the families rather than the successes (`SKILL` §9.3). **Read:**
//! `[Content_Types].xml`; `_rels/.rels`; `ppt/presentation.xml`'s `p:sldSz`,
//! `p:sldIdLst` and `p:sldMasterIdLst`; every master, layout and slide part
//! reachable through those lists; each part's `p:cSld` name, `p:bg` solid fill
//! and `p:spTree`; `p:sp`, `p:cxnSp`, `p:pic` and nested `p:grpSp` with
//! `a:xfrm`, `a:prstGeom` + `a:avLst`, a single-subpath `a:custGeom`,
//! `a:solidFill` and `a:ln`; `p:nvPr/p:ph`; and `a:txBody` in full —
//! `a:bodyPr` with its autofit, the nine-level `a:lstStyle`, and `a:p`/`a:pPr`
//! with level, alignment, margins, indent, spacing, bullets and tab stops, plus
//! `a:r`/`a:rPr`, `a:br` and `a:fld`. Media reachable from an `a:blip@r:embed`
//! is registered in `Definitions::media`.
//!
//! **Not read, and reported:** the theme part — so every `a:schemeClr`,
//! `p:style` reference and `p:clrMap` is a reported gap and no shape gets a
//! themed fill; `p:txStyles` and `p:defaultTextStyle`, which are two tiers of the
//! text cascade; `p:transition` and `p:timing`; notes and handout masters;
//! `p:graphicFrame`, so tables, charts and SmartArt do not arrive at all;
//! gradient, picture and pattern fills; effects; `a:tbl`; `a:arcTo`; and
//! `p14:sectionLst`.

use std::collections::BTreeMap;

use casual_doc_loss::{CompatibilityReport, PreservationLedger};
use casual_doc_model::NodeId;
use casual_doc_model::v1::Definitions;
use casual_doc_package::{BoundedPackage, PackageLimits};
use casual_pres_model::{
    Presentation, Slide, SlideLayout, SlideLayoutId, SlideMaster, SlideMasterId,
};

use crate::ImportError;
use crate::ids::Ids;
use crate::limits::ImportLimits;
use crate::loss::Reporter;
use crate::opc::{PresentationPackage, SLIDE_LAYOUT_REL};
use crate::parts::{
    PartContext, all_of_type, first_of_type, read_layout, read_master, read_presentation_part,
    read_slide, resolve_part,
};
use crate::shapes::Surface;

/// A `.pptx` imported into the presentation model, with its fidelity report.
///
/// The report is not optional and not a side channel: "direct OOXML with verbatim
/// retention" is only an advantage over a converter pipeline **if the loss that
/// retention does not cover is detected, named and auditable**. A caller that
/// opens a deck and ignores this has the same information a converter gives them.
#[derive(Debug)]
pub struct ImportedPresentation {
    /// The validated deck.
    pub presentation: Presentation,
    /// What the projection did not recover, by construct.
    pub report: CompatibilityReport,
    /// The records licensing any `preserved` claim in the report. Empty today,
    /// because there is no presentation writer and so no verbatim byte floor —
    /// see `loss.rs` for why that is stated rather than papered over.
    pub ledger: PreservationLedger,
}

/// Imports a `.pptx` package into a validated [`Presentation`].
///
/// # What is modelled and what is not
///
/// See the module documentation, which enumerates both — including the
/// constructs this build reads nothing of. An ordinary deck therefore imports
/// with a **non-empty** report, and that is the honest outcome rather than a
/// defect: a report that fired only on exotic files would be claiming a fidelity
/// this build does not have.
///
/// # Errors
///
/// Fails if the ZIP is refused at the admission boundary, if the package is not a
/// presentation, if a part's XML is malformed or over a bound, if
/// `ppt/presentation.xml` declares no `p:sldSz`, if a slide/layout/master
/// relationship does not resolve, or if the resulting deck fails
/// [`Presentation::validate`].
///
/// # Complexity
///
/// O(package): one pass over each admitted part that the reference graph reaches,
/// plus one validation walk over the built deck. This is an open-the-file
/// operation and must not run on an interaction path (`docs/107` §4).
pub fn import_pptx(
    bytes: &[u8],
    package_limits: PackageLimits,
    limits: ImportLimits,
) -> Result<ImportedPresentation, ImportError> {
    let limits = limits.clamped();
    let package = BoundedPackage::open(bytes, package_limits)?;
    let mut package = PresentationPackage::open(package)?;

    let mut reporter = Reporter::new();
    let mut ids = Ids::new();
    let mut definitions = Definitions::default();
    let mut registered_media: BTreeMap<String, NodeId> = BTreeMap::new();

    // Every admitted part with its declared content type. Built once: the media
    // resolver needs admission AND the declared type, and asking the package per
    // reference would be a lookup-by-name inside a loop over names.
    let admitted: BTreeMap<String, Option<String>> = package
        .part_names()
        .into_iter()
        .map(|part_name| {
            let content_type = package.content_type_of(&part_name).map(str::to_owned);
            (part_name, content_type)
        })
        .collect();

    let presentation_part = package.presentation_part().to_owned();
    let presentation_bytes = package.read_part(&presentation_part)?;
    let declaration = read_presentation_part(
        &presentation_bytes,
        &presentation_part,
        &mut reporter,
        limits,
    )?;
    let presentation_relationships = package.relationships_of(&presentation_part)?;
    let surface = Surface {
        width_emu: declaration.slide_size.width_emu,
        height_emu: declaration.slide_size.height_emu,
    };

    // Which parts the reference graph reached, so the unconsumed remainder can be
    // enumerated rather than assumed empty.
    let mut consumed: Vec<String> = vec![
        crate::opc::CONTENT_TYPES_PART.to_owned(),
        crate::opc::ROOT_RELATIONSHIPS_PART.to_owned(),
        presentation_part.clone(),
    ];

    // Tier 1: masters, in `p:sldMasterIdLst` order.
    let mut masters: Vec<SlideMaster> = Vec::new();
    let mut master_ids: BTreeMap<String, SlideMasterId> = BTreeMap::new();
    for relationship_id in &declaration.master_relationship_ids {
        let part = resolve_part(
            &package,
            &presentation_relationships,
            &presentation_part,
            relationship_id,
        )?;
        if master_ids.contains_key(&part) {
            continue;
        }
        let bytes = package.read_part(&part)?;
        let relationships = package.relationships_of(&part)?;
        let id = ids.next()?;
        let mut context = PartContext {
            reporter: &mut reporter,
            ids: &mut ids,
            definitions: &mut definitions,
            registered_media: &mut registered_media,
            admitted: &admitted,
            limits,
        };
        let master = read_master(&bytes, &part, &relationships, &mut context, surface, id)?;
        master_ids.insert(part.clone(), master.id);
        consumed.push(part.clone());
        masters.push(master);
    }

    // Tier 2: layouts, read through each MASTER's own `slideLayout`
    // relationships. Reading them from the presentation part's relationships
    // instead would miss a layout only one master references, which is how a
    // multi-master deck loses half its gallery. Keyed by part so a layout shared
    // between two masters is read once and keeps one identity.
    let mut layouts: Vec<SlideLayout> = Vec::new();
    let mut layout_ids: BTreeMap<String, SlideLayoutId> = BTreeMap::new();
    for (master_part, master_id) in &master_ids {
        let relationships = package.relationships_of(master_part)?;
        for layout_part in all_of_type(&relationships, SLIDE_LAYOUT_REL) {
            if layout_ids.contains_key(&layout_part) || !package.contains_part(&layout_part) {
                continue;
            }
            if layouts.len() >= limits.max_layouts {
                break;
            }
            let bytes = package.read_part(&layout_part)?;
            let layout_relationships = package.relationships_of(&layout_part)?;
            let id = ids.next()?;
            let mut context = PartContext {
                reporter: &mut reporter,
                ids: &mut ids,
                definitions: &mut definitions,
                registered_media: &mut registered_media,
                admitted: &admitted,
                limits,
            };
            let layout = read_layout(
                &bytes,
                &layout_part,
                *master_id,
                &layout_relationships,
                &mut context,
                surface,
                id,
            )?;
            layout_ids.insert(layout_part.clone(), layout.id);
            consumed.push(layout_part.clone());
            layouts.push(layout);
        }
    }

    // Tier 3: slides, in `p:sldIdLst` order — which IS the deck's order. Nothing
    // else in the package encodes it; see `parts.rs`.
    let mut slides: Vec<Slide> = Vec::new();
    for relationship_id in &declaration.slide_relationship_ids {
        let part = resolve_part(
            &package,
            &presentation_relationships,
            &presentation_part,
            relationship_id,
        )?;
        let bytes = package.read_part(&part)?;
        let relationships = package.relationships_of(&part)?;
        let layout_part = first_of_type(&relationships, SLIDE_LAYOUT_REL)
            .ok_or_else(|| ImportError::SlideWithoutLayout { part: part.clone() })?;
        let layout = match layout_ids.get(&layout_part) {
            Some(layout) => *layout,
            None => {
                // The slide names a layout no master reached. Read it now and
                // attach it to the first master, rather than refusing the slide:
                // a layout outside every master's list is a package defect whose
                // repair is unambiguous, and losing the slide would be worse than
                // losing its gallery grouping.
                if !package.contains_part(&layout_part) {
                    return Err(ImportError::SlideWithoutLayout { part: part.clone() });
                }
                let master_id = masters.first().map(|master| master.id).ok_or_else(|| {
                    ImportError::LayoutWithoutMaster {
                        part: layout_part.clone(),
                    }
                })?;
                let layout_bytes = package.read_part(&layout_part)?;
                let layout_relationships = package.relationships_of(&layout_part)?;
                let id = ids.next()?;
                let mut context = PartContext {
                    reporter: &mut reporter,
                    ids: &mut ids,
                    definitions: &mut definitions,
                    registered_media: &mut registered_media,
                    admitted: &admitted,
                    limits,
                };
                let read = read_layout(
                    &layout_bytes,
                    &layout_part,
                    master_id,
                    &layout_relationships,
                    &mut context,
                    surface,
                    id,
                )?;
                reporter.degraded_attribute(&presentation_part, b"sldMasterId", b"sldLayoutIdLst");
                layout_ids.insert(layout_part.clone(), read.id);
                consumed.push(layout_part.clone());
                layouts.push(read);
                *layout_ids
                    .get(&layout_part)
                    .ok_or_else(|| ImportError::SlideWithoutLayout { part: part.clone() })?
            }
        };
        let mut context = PartContext {
            reporter: &mut reporter,
            ids: &mut ids,
            definitions: &mut definitions,
            registered_media: &mut registered_media,
            admitted: &admitted,
            limits,
        };
        let slide = read_slide(&bytes, &part, layout, &relationships, &mut context, surface)?;
        consumed.push(part.clone());
        slides.push(slide);
    }

    // A layout whose master relationship is absent would have been attached to a
    // master above, so this only fires for a layout that reached no master at all
    // — which `Presentation::validate` would refuse with a dangling reference.
    if layouts
        .iter()
        .any(|layout| !master_ids.values().any(|master| *master == layout.master))
    {
        return Err(ImportError::LayoutWithoutMaster {
            part: presentation_part.clone(),
        });
    }

    // Every admitted part the reference graph did not reach. A whole-part
    // disposition each, because absence from a report is an overstatement by
    // omission — the `_rels` companions and the media parts registered above are
    // excluded, since those are consumed.
    report_unconsumed(&mut reporter, &admitted, &consumed, &registered_media);

    let presentation = Presentation::new(
        ids.next()?,
        declaration.slide_size,
        masters,
        layouts,
        slides,
        definitions,
    )?;
    let (report, ledger) = reporter.finish()?;
    Ok(ImportedPresentation {
        presentation,
        report,
        ledger,
    })
}

/// Reports each admitted part the semantic projection did not consume.
fn report_unconsumed(
    reporter: &mut Reporter,
    admitted: &BTreeMap<String, Option<String>>,
    consumed: &[String],
    registered_media: &BTreeMap<String, NodeId>,
) {
    for part_name in admitted.keys() {
        if consumed.iter().any(|entry| entry == part_name) {
            continue;
        }
        if registered_media.contains_key(part_name) {
            continue;
        }
        // A `_rels` part is metadata about a part, not content: it is consumed
        // whenever its owner is, and reporting it would double every finding
        // about the owner.
        if part_name.contains("/_rels/") || part_name.starts_with("_rels/") {
            continue;
        }
        reporter.unconsumed_part(part_name);
    }
}
