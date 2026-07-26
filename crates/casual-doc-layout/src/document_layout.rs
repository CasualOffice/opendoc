//! The one-call layout driver — turning a [`Document`] into a finished
//! [`PaginatedLayout`](crate::page::PaginatedLayout).
//!
//! Rendering a real document is a fixed pipeline of already-built pieces: derive
//! the page geometry from the section, build the body galley, flow the section's
//! headers/footers, paginate, then run the three post-pagination passes (running
//! content, page-number fields, anchored drawings) in the order the
//! incremental-golden invariant requires. [`paginate_document`] wires them
//! together so callers — the viewer and the fidelity harness — get a
//! ready-to-render layout from a single call, at the document's *true* geometry
//! (its real page size and margins), rather than the hand-built US-Letter
//! `PageConfig` every call site used to carry.
//!
//! It composes the existing engine pieces only; it does **not** touch the
//! incremental/halt core of [`crate::paginate`]. Every step is the same public
//! function the manual wiring calls (verified by the `equals_manual_wiring`
//! regression test), so the driver can never drift from the pipeline it replaces.
//!
//! ## Geometry and the single-section first cut
//!
//! The geometry (page size, margins, header/footer band heights) is taken from the
//! document's **first** section ([`Definitions::sections`](casual_doc_model::v1::Definitions::sections)). A multi-section
//! document is paginated under that one geometry for now — a `paginate_sections`
//! that switches geometry at each section boundary is a follow-up; there is no
//! such entry point in the paginator yet, so composing one here would mean
//! reaching into the halt core, which this driver deliberately does not do. A
//! document with no sections at all (which a valid imported DOCX never is — Word
//! always writes a trailing `sectPr`) falls back to US-Letter with 1-inch margins.
//!
//! The header/footer band heights are the natural height of the tallest flowed
//! variant ([`RunningContent::band_heights`]), so the reserved band always fits
//! the content Word would place there; the body content area shrinks by both bands
//! exactly as [`crate::running`] documents.

use casual_doc_model::v1::{Document, HeaderFooterKind, SectionBoundary};

use crate::anchor::{collect_anchored, place_anchored_drawings};
use crate::flow::{build_galley, flow_header_footer};
use crate::paginate::{PageConfig, paginate, resolve_fields};
use crate::running::{HeaderFooter, RunningContent, place_running_content};
use crate::units::{Size, Twip};
use casual_doc_model::v1::SectionId;

/// US-Letter page size in twips (8.5in × 11in), the fallback for a document that
/// declares no section.
const LETTER: Size = Size {
    width: Twip(12_240),
    height: Twip(15_840),
};
/// A 1-inch margin in twips — the fallback margin when no section is declared.
const ONE_INCH: Twip = Twip(1_440);

/// Derives the page geometry ([`PageConfig`]) for a document from its first
/// section, with **zero** header/footer bands — the pure page box and margins.
///
/// [`paginate_document`] calls this and then fills in the band heights once it has
/// flowed the running content; callers that only need the page dimensions (for
/// example, to size a render surface) can use it directly, since the full page
/// size and margins do not depend on the bands.
///
/// A document with no section falls back to US-Letter with 1-inch margins.
#[must_use]
pub fn document_page_config(document: &Document) -> PageConfig {
    let sections = &document.definitions().sections;
    match sections.first() {
        Some(section) => section_page_config(section),
        None => PageConfig {
            section: SectionId::new(document.id()),
            page_size: LETTER,
            margin_top: ONE_INCH,
            margin_bottom: ONE_INCH,
            margin_start: ONE_INCH,
            margin_end: ONE_INCH,
            header_height: Twip::ZERO,
            footer_height: Twip::ZERO,
        },
    }
}

/// The zero-band [`PageConfig`] for one section's page box and margins.
fn section_page_config(section: &SectionBoundary) -> PageConfig {
    PageConfig {
        section: section.id,
        page_size: Size::new(
            Twip(section.page_size.width_twips),
            Twip(section.page_size.height_twips),
        ),
        margin_top: Twip(section.page_margins.top_twips),
        margin_bottom: Twip(section.page_margins.bottom_twips),
        margin_start: Twip(section.page_margins.start_twips),
        margin_end: Twip(section.page_margins.end_twips),
        header_height: Twip::ZERO,
        footer_height: Twip::ZERO,
    }
}

/// Builds the document's [`RunningContent`] — the first section's header/footer
/// variants, each flowed through the same [`flow_header_footer`] path the body
/// uses (so a header holding a paragraph, table, or image lays out identically),
/// plus the two flags that drive per-page variant selection.
///
/// Each [`HeaderFooterRef`](casual_doc_model::v1::HeaderFooterRef) is resolved
/// against the header/footer definition store; a reference that does not resolve
/// contributes nothing (an empty variant falls back to the default at selection
/// time, matching Word). `content_width` is the body content width (page width
/// minus the side margins) — the same width the bands are laid out at.
fn build_running_content(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    content_width: Twip,
) -> RunningContent {
    let defs = document.definitions();
    let mut header = HeaderFooter::default();
    let mut footer = HeaderFooter::default();
    let section = defs.sections.first();

    if let Some(section) = section {
        for reference in &section.headers {
            if let Some(hf) = defs.headers.get(&reference.reference) {
                let flowed = flow_header_footer(document, &hf.blocks, shaper, content_width);
                *variant_mut(&mut header, reference.kind) = flowed;
            }
        }
        for reference in &section.footers {
            if let Some(hf) = defs.footers.get(&reference.reference) {
                let flowed = flow_header_footer(document, &hf.blocks, shaper, content_width);
                *variant_mut(&mut footer, reference.kind) = flowed;
            }
        }
    }

    RunningContent {
        header,
        footer,
        title_page: section.and_then(|s| s.title_page).unwrap_or(false),
        even_and_odd: defs.settings.even_and_odd_headers,
    }
}

/// The variant slot of `hf` a given [`HeaderFooterKind`] writes into.
fn variant_mut(
    hf: &mut HeaderFooter,
    kind: HeaderFooterKind,
) -> &mut Vec<crate::block::BlockFragment> {
    match kind {
        HeaderFooterKind::Default => &mut hf.default,
        HeaderFooterKind::First => &mut hf.first,
        HeaderFooterKind::Even => &mut hf.even,
    }
}

/// Lays a whole [`Document`] out into a finished, ready-to-render
/// [`PaginatedLayout`](crate::page::PaginatedLayout) in one call — the single entry point the viewer and the
/// fidelity harness build on.
///
/// The pipeline, all composed from existing engine functions:
///
/// 1. Derive the [`PageConfig`] from the first section's geometry
///    ([`document_page_config`]).
/// 2. Flow the section's header/footer variants into [`RunningContent`] and
///    reserve their band heights in the config, so the body content area is
///    correct before pagination.
/// 3. Build the body galley at the content width ([`build_galley`]).
/// 4. [`paginate`] the galley, then run the post-pagination passes **in order** —
///    [`place_running_content`], [`resolve_fields`], [`place_anchored_drawings`] —
///    the order the incremental-golden post-passes require (running content before
///    fields so a `Page X of Y` footer resolves; anchors last, off the pagination
///    hot path).
///
/// Multi-section documents are laid out under the first section's geometry (see
/// the module docs); everything else — headers/footers, page-number fields,
/// inline and anchored drawings, tables — flows through the identical pipeline the
/// manual wiring used.
#[must_use]
pub fn paginate_document(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
) -> crate::page::PaginatedLayout {
    let mut config = document_page_config(document);
    // The content (and band) width is the page box minus the side margins; it does
    // not depend on the header/footer bands, so it is stable before they are set.
    let content_width = config.content_area().size.width;

    let running = build_running_content(document, shaper, content_width);
    let (header_height, footer_height) = running.band_heights();
    config.header_height = header_height;
    config.footer_height = footer_height;

    let galley = build_galley(document, shaper, content_width);
    let mut layout = paginate(&galley, &config);

    // Post-pagination passes, in the required order: running content is placed
    // first so its fields exist to stamp, then the field pass resolves every
    // `PAGE`/`NUMPAGES` (body and running content), then anchored drawings are
    // placed onto the pages their paragraphs landed on.
    place_running_content(&mut layout, &running, &config);
    resolve_fields(&mut layout, shaper);
    let anchors = collect_anchored(document);
    place_anchored_drawings(&mut layout, &anchors, &config);

    layout
}
