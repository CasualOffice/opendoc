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
//! The two tiers of the text cascade above the shape are read too: a master's
//! `p:txStyles` (title, body and other, nine levels each) and the presentation's
//! `p:defaultTextStyle`, both through the one `CT_TextListStyle` reader
//! `a:lstStyle` uses.
//!
//! The theme is read as well: `ppt/theme/theme1.xml`'s `a:clrScheme` (twelve
//! slots), `a:fontScheme` (major and minor, latin/ea/cs plus script overrides) and
//! the modelled subset of `a:fmtScheme`, together with the `p:clrMap` on each
//! master and the `p:clrMapOvr/a:overrideClrMapping` on any layout or slide that
//! replaces it. With both halves present an `a:schemeClr` resolves to a concrete
//! colour with its transforms folded in, a `p:style` reference resolves against
//! the style matrix, and a `+mj-lt` typeface resolves through
//! `Presentation::resolve_typeface`.
//!
//! **Not read, and reported:** `p:transition` and `p:timing`; notes and handout
//! masters; `p:graphicFrame`, so tables, charts and SmartArt do not arrive at all;
//! gradient, picture and pattern fills on a shape; effects; `a:tbl`; `a:arcTo`;
//! `p14:sectionLst`; `a:fontRef` (neither the collection it names nor its colour
//! has a field); `a:satMod` and the hue/gamma/channel colour modifiers; the theme
//! part's `a:objectDefaults`, `a:extraClrSchemeLst` and `a:custClrLst`; and
//! `a:bgFillStyleLst`, so a `p:bgRef` still resolves to nothing.
//!
//! # One theme per deck, and what that costs
//!
//! `v1::Definitions` carries ONE `color_scheme`/`font_scheme`/`format_scheme`,
//! because a WordprocessingML package has one theme part. A deck may have one per
//! master. So the theme reached from `ppt/presentation.xml` is the deck's, and a
//! master naming a DIFFERENT theme part is reported rather than silently resolved
//! against the wrong palette. Every colour in the deck therefore resolves against
//! one theme, which is consistent with what the model says and honest about what
//! it cannot say. The fix is a presentation-side theme table keyed by master; it
//! is additive and it is not built here.

use std::collections::BTreeMap;

use casual_doc_loss::{CompatibilityReport, PreservationLedger};
use casual_doc_model::NodeId;
use casual_doc_model::v1::Definitions;
use casual_doc_package::{BoundedPackage, PackageLimits};
use casual_pres_model::{
    ColorMap, ColorMapping, Presentation, Slide, SlideLayout, SlideLayoutId, SlideMaster,
    SlideMasterId,
};

use crate::ImportError;
use crate::ids::Ids;
use crate::limits::ImportLimits;
use crate::loss::Reporter;
use crate::opc::{PresentationPackage, SLIDE_LAYOUT_REL, SLIDE_MASTER_REL, THEME_REL};
use crate::parts::{
    PartContext, all_of_type, first_of_type, read_layout, read_master, read_presentation_part,
    read_slide, resolve_part,
};
use crate::shapes::Surface;
// Own line (anti-conflict): the theme half of the import.
use crate::theme::{
    Resolver, ThemePart, read_color_map_of, read_theme_part, report_unpaintable_style_refs,
};

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
/// plus one validation walk over the built deck. A master, layout or slide part is
/// read TWICE — once by `theme::read_color_map_of` and once in full — because the
/// schema puts `p:clrMap` after `p:cSld` while the shape tree inside `p:cSld` is
/// what needs it; that is a factor on the constant, not on the order, and it is
/// stated rather than hidden. This is an open-the-file operation and must not run
/// on an interaction path (`docs/107` §4).
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
    let presentation_relationships = package.relationships_of(&presentation_part)?;

    // Which parts the reference graph reached, so the unconsumed remainder can be
    // enumerated rather than assumed empty.
    let mut consumed: Vec<String> = vec![
        crate::opc::CONTENT_TYPES_PART.to_owned(),
        crate::opc::ROOT_RELATIONSHIPS_PART.to_owned(),
        presentation_part.clone(),
    ];

    // Tier 0: the theme, BEFORE anything that states a colour. A `p:clrMap` is
    // read per part on top of it; without the palette underneath, every
    // `a:schemeClr` in the part would have to be deferred, and `v1::Fill` holds a
    // concrete `Rgba` with nowhere to defer one to — including in
    // `p:defaultTextStyle`, where PowerPoint writes a `tx1` glyph colour.
    let theme_part = discover_theme_part(&mut package, &presentation_relationships)?;
    let theme = match theme_part.as_deref() {
        Some(part) => {
            let bytes = package.read_part(part)?;
            let read = read_theme_part(&bytes, part, &mut reporter, limits)?;
            consumed.push(part.to_owned());
            read
        }
        None => ThemePart::default(),
    };
    // The presentation-level resolver uses the IDENTITY colour map, because only a
    // master states one and `p:defaultTextStyle` sits above every master.
    let deck_resolver = Resolver::new(&theme, ColorMap::IDENTITY);

    let presentation_bytes = package.read_part(&presentation_part)?;
    let declaration = read_presentation_part(
        &presentation_bytes,
        &presentation_part,
        &mut reporter,
        &mut ids,
        limits,
        deck_resolver,
    )?;
    let surface = Surface {
        width_emu: declaration.slide_size.width_emu,
        height_emu: declaration.slide_size.height_emu,
    };
    let mut color_mapping = ColorMapping::default();

    // Tier 1: masters, in `p:sldMasterIdLst` order.
    let mut masters: Vec<SlideMaster> = Vec::new();
    let mut master_ids: BTreeMap<String, SlideMasterId> = BTreeMap::new();
    // The colour map each master established, so a layout that inherits it does
    // not have to re-resolve a chain through a part list that is still being built.
    let mut master_maps: BTreeMap<SlideMasterId, ColorMap> = BTreeMap::new();
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
        // A master may name its own theme part, and `Definitions` holds one theme.
        // Reported rather than resolved against the wrong palette; see the module
        // note on what that costs.
        if let Some(own_theme) = first_of_type(&relationships, THEME_REL)
            && theme_part.as_deref() != Some(own_theme.as_str())
        {
            reporter.degraded_attribute(&part, b"sldMaster", b"theme");
        }
        // A master with no `p:clrMap` is malformed — the schema requires one — so
        // the identity map keeps the deck openable and the fact is reported.
        let map = match read_color_map_of(&bytes, &part, &mut reporter, limits)? {
            Some(map) => map,
            None => {
                reporter.invalid(&part, b"clrMap");
                ColorMap::IDENTITY
            }
        };
        let mut context = PartContext {
            reporter: &mut reporter,
            ids: &mut ids,
            definitions: &mut definitions,
            registered_media: &mut registered_media,
            admitted: &admitted,
            limits,
            resolver: deck_resolver.with_map(map),
        };
        let master = read_master(&bytes, &part, &relationships, &mut context, surface, id)?;
        color_mapping.masters.insert(master.id, map);
        master_maps.insert(master.id, map);
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
    // The map in force on each layout, inherited or overridden, for its slides.
    let mut layout_maps: BTreeMap<SlideLayoutId, ColorMap> = BTreeMap::new();
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
            let inherited = master_maps.get(master_id).copied().unwrap_or_default();
            let overridden = read_color_map_of(&bytes, &layout_part, &mut reporter, limits)?;
            let in_force = overridden.unwrap_or(inherited);
            let mut context = PartContext {
                reporter: &mut reporter,
                ids: &mut ids,
                definitions: &mut definitions,
                registered_media: &mut registered_media,
                admitted: &admitted,
                limits,
                resolver: deck_resolver.with_map(in_force),
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
            // Only an OVERRIDE is recorded. A layout that states
            // `<a:masterClrMapping/>` inherits, and storing the inherited value
            // would make the two indistinguishable in the model.
            if let Some(map) = overridden {
                color_mapping.layouts.insert(layout.id, map);
            }
            layout_maps.insert(layout.id, in_force);
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
                let inherited = master_maps.get(&master_id).copied().unwrap_or_default();
                let overridden =
                    read_color_map_of(&layout_bytes, &layout_part, &mut reporter, limits)?;
                let in_force = overridden.unwrap_or(inherited);
                let mut context = PartContext {
                    reporter: &mut reporter,
                    ids: &mut ids,
                    definitions: &mut definitions,
                    registered_media: &mut registered_media,
                    admitted: &admitted,
                    limits,
                    resolver: deck_resolver.with_map(in_force),
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
                if let Some(map) = overridden {
                    color_mapping.layouts.insert(read.id, map);
                }
                layout_maps.insert(read.id, in_force);
                layout_ids.insert(layout_part.clone(), read.id);
                consumed.push(layout_part.clone());
                layouts.push(read);
                *layout_ids
                    .get(&layout_part)
                    .ok_or_else(|| ImportError::SlideWithoutLayout { part: part.clone() })?
            }
        };
        let inherited = layout_maps.get(&layout).copied().unwrap_or_default();
        let overridden = read_color_map_of(&bytes, &part, &mut reporter, limits)?;
        let mut context = PartContext {
            reporter: &mut reporter,
            ids: &mut ids,
            definitions: &mut definitions,
            registered_media: &mut registered_media,
            admitted: &admitted,
            limits,
            resolver: deck_resolver.with_map(overridden.unwrap_or(inherited)),
        };
        let slide = read_slide(&bytes, &part, layout, &relationships, &mut context, surface)?;
        if let Some(map) = overridden {
            color_mapping.slides.insert(slide.id, map);
        }
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

    // The theme lands in the SHARED definition tables, not in a presentation-only
    // one: `v1::Definitions` already carries these three fields for the document
    // class and `casual-doc-layout` already resolves against them, so a deck and a
    // document cannot disagree about what a theme is.
    //
    // `format_scheme_xml` is deliberately NOT populated. On the document side it is
    // the verbatim subtree the semantic writer emits back; there is no
    // PresentationML writer, so retaining the string here would buy nothing and
    // would read as a retention claim the pipeline cannot honour.
    definitions.color_scheme = theme.color_scheme.clone();
    definitions.font_scheme = theme.font_scheme.clone();
    definitions.format_scheme = theme.format_scheme.clone();

    // Classified AFTER the shapes, because it needs both halves: the matrix says
    // what an entry is, the side table says which entries are actually asked for.
    if let Some(scheme) = theme.format_scheme.as_ref() {
        report_unpaintable_style_refs(scheme, &definitions.shape_styles, &mut reporter);
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
    )?
    // The bottom tier of the text cascade, attached after construction because it
    // is an optional part and a seventh positional argument would be a breaking
    // change to every caller for something most packages omit.
    .with_default_text_style(declaration.default_text_style)?
    // The colour maps, for the same reason plus one more: they are keyed by part
    // id, so they can only be attached once those ids exist.
    .with_color_mapping(color_mapping)?;
    let (report, ledger) = reporter.finish()?;
    Ok(ImportedPresentation {
        presentation,
        report,
        ledger,
    })
}

/// The deck's theme part: the FIRST master's, else the presentation part's own.
///
/// The master comes first because that is where ECMA-376 puts the required theme
/// relationship; `ppt/presentation.xml`'s is conventional, and a package that
/// carries only the required one would otherwise import with no theme at all and
/// every scheme colour reported — which is the gap this whole change closes.
///
/// Masters are reached by relationship TYPE here and nowhere else. That is sound
/// precisely because the question is "which theme", not "which order": see
/// `SLIDE_MASTER_REL`.
///
/// # Complexity
///
/// O(masters) relationship-part reads, which is one or two in almost every deck,
/// and they are the same parts the master tier reads immediately afterwards.
fn discover_theme_part(
    package: &mut PresentationPackage<'_>,
    presentation_relationships: &crate::opc::Relationships,
) -> Result<Option<String>, ImportError> {
    for master_part in all_of_type(presentation_relationships, SLIDE_MASTER_REL) {
        if !package.contains_part(&master_part) {
            continue;
        }
        let relationships = package.relationships_of(&master_part)?;
        if let Some(theme) =
            first_of_type(&relationships, THEME_REL).filter(|part| package.contains_part(part))
        {
            return Ok(Some(theme));
        }
    }
    Ok(first_of_type(presentation_relationships, THEME_REL)
        .filter(|part| package.contains_part(part)))
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
