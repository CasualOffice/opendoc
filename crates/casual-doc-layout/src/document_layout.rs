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
//! ## Sections and column-aware flow
//!
//! The body is partitioned into one run per section
//! ([`Definitions::sections`](casual_doc_model::v1::Definitions::sections)); each
//! run is flowed at *its own* column width and paginated by
//! [`crate::columns`], which fills a section's columns in order and carries the
//! page cursor across section boundaries (a `continuous` section shares the
//! previous section's page). This is what flows a `w:cols` document into newspaper
//! columns instead of one full-width column. A document with no sections at all
//! (which a valid imported DOCX never is — Word always writes a trailing `sectPr`)
//! falls back to a single full-width run under US-Letter with 1-inch margins.
//!
//! Header/footer variants and band reservations are resolved independently for
//! every section. Each produced page selects the running-content plan identified
//! by its immutable [`Page::section`](crate::page::Page::section), and `titlePg`
//! is evaluated against that section's first page rather than only document page
//! one. Per-section balancing of the last column page remains a documented
//! deferral (see [`crate::columns`]).

use std::collections::BTreeMap;

use casual_doc_model::v1::{
    BlockNode, Document, HeaderFooterKind, HeaderFooterRef, SectionBoundary,
};

use crate::anchor::{header_float_reserve_for_section, place_floats};
use crate::columns::{
    ColumnLayout, SectionRun, column_layout, paginate_columns, section_starts_new_page,
};
use crate::flow::{build_galley_for_blocks, flow_header_footer};
use crate::paginate::{PageConfig, resolve_anchored_fields, resolve_fields};
use crate::running::{HeaderFooter, RunningContent, place_running_content_on_page};
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
/// Word's default header/footer band distance from the page edge (`w:pgMar`
/// `@w:header`/`@w:footer`), used when the attribute is absent.
const DEFAULT_BAND_DISTANCE: Twip = Twip(720);

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
            header_distance: DEFAULT_BAND_DISTANCE,
            footer_distance: DEFAULT_BAND_DISTANCE,
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
        // The `w:header`/`w:footer` distances the header/footer bands nest at,
        // falling back to Word's 720-twip default when the attribute is absent.
        header_distance: section
            .page_margins
            .header_twips
            .map_or(DEFAULT_BAND_DISTANCE, Twip),
        footer_distance: section
            .page_margins
            .footer_twips
            .map_or(DEFAULT_BAND_DISTANCE, Twip),
        header_height: Twip::ZERO,
        footer_height: Twip::ZERO,
    }
}

/// Builds one section's [`RunningContent`] — its header/footer
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
    section: &SectionBoundary,
    content_width: Twip,
) -> RunningContent {
    let defs = document.definitions();
    let mut header = HeaderFooter::default();
    let mut footer = HeaderFooter::default();

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

    RunningContent {
        header,
        footer,
        title_page: section.title_page.unwrap_or(false),
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

/// All layout inputs that are local to one section. The config already includes
/// that section's measured header/footer (and positioned-header-float) bands.
struct SectionPlan {
    config: PageConfig,
    running: RunningContent,
}

/// Resolves running-content inheritance and geometry for every section before
/// body flow. In OOXML an omitted reference links to the previous section; an
/// explicit reference (including one to an empty part) replaces that variant.
fn build_section_plans(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
) -> Vec<SectionPlan> {
    let mut plans = Vec::new();
    let mut effective_headers: Vec<HeaderFooterRef> = Vec::new();
    let mut effective_footers: Vec<HeaderFooterRef> = Vec::new();

    for section in &document.definitions().sections {
        merge_running_refs(&mut effective_headers, &section.headers);
        merge_running_refs(&mut effective_footers, &section.footers);

        let mut effective_section = section.clone();
        effective_section.headers.clone_from(&effective_headers);
        effective_section.footers.clone_from(&effective_footers);

        let mut config = section_page_config(section);
        let content_width = config.content_area().size.width;
        let running = build_running_content(document, shaper, &effective_section, content_width);
        let (header_height, footer_height) = running.band_heights();
        let header_float =
            header_float_reserve_for_section(document, shaper, &config, &effective_section);
        config.header_height = header_height.max(header_float);
        config.footer_height = footer_height;
        plans.push(SectionPlan { config, running });
    }

    // A sectionless model is malformed for imported DOCX but remains a supported
    // deterministic fallback for programmatic callers.
    if plans.is_empty() {
        plans.push(SectionPlan {
            config: document_page_config(document),
            running: RunningContent {
                even_and_odd: document.definitions().settings.even_and_odd_headers,
                ..RunningContent::default()
            },
        });
    }
    plans
}

/// Applies explicit references over inherited references, one variant at a time.
fn merge_running_refs(effective: &mut Vec<HeaderFooterRef>, current: &[HeaderFooterRef]) {
    for reference in current {
        if let Some(slot) = effective
            .iter_mut()
            .find(|existing| existing.kind == reference.kind)
        {
            *slot = *reference;
        } else {
            effective.push(*reference);
        }
    }
}

/// Builds one [`SectionRun`] per document section, in body order. The body is
/// partitioned at each paragraph carrying a section break (the final section is
/// body-level and covers the trailing blocks); each section's block slice is
/// flowed at that section's column width, so line breaking happens at the column
/// — not the full body — width.
///
/// Each section uses the page geometry and header/footer band heights already
/// resolved in its matching [`SectionPlan`]. A document with no declared section
/// produces one full-width run under the fallback plan.
fn build_section_runs(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
) -> Vec<SectionRun> {
    let sections = &document.definitions().sections;
    let body = document.body();
    if sections.is_empty() {
        let config = plans[0].config;
        let content = config.content_area();
        let layout = ColumnLayout::single(content);
        let galley = build_galley_for_blocks(document, shaper, body, layout.flow_width());
        return vec![SectionRun {
            config,
            layout,
            galley,
            starts_new_page: true,
        }];
    }

    let mut runs = Vec::new();
    let mut start = 0usize;
    // Non-final sections each end at a paragraph carrying that section's break.
    for (end_excl, boundary) in section_break_points(body, sections) {
        push_section_run(
            document,
            shaper,
            plan_for_section(plans, boundary.id),
            boundary,
            &body[start..end_excl],
            &mut runs,
        );
        start = end_excl;
    }
    // The trailing (body-level) final section covers everything left.
    if let Some(last) = sections.last() {
        push_section_run(
            document,
            shaper,
            plan_for_section(plans, last.id),
            last,
            &body[start..],
            &mut runs,
        );
    }
    runs
}

/// Finds the precomputed plan for `section`, falling back to the first plan for
/// malformed section references in the same way as [`section_break_points`].
fn plan_for_section(plans: &[SectionPlan], section: SectionId) -> &SectionPlan {
    plans
        .iter()
        .find(|plan| plan.config.section == section)
        .unwrap_or(&plans[0])
}

/// The `(end_exclusive, boundary)` cut points of the non-final sections: for each
/// body paragraph carrying a `w:sectPr`, the index one past it and the matching
/// [`SectionBoundary`]. A break whose id does not resolve falls back to the first
/// section's geometry so a malformed document still lays out.
fn section_break_points<'a>(
    body: &[BlockNode],
    sections: &'a [SectionBoundary],
) -> Vec<(usize, &'a SectionBoundary)> {
    let mut points = Vec::new();
    for (i, block) in body.iter().enumerate() {
        if let BlockNode::Paragraph(paragraph) = block
            && let Some(sid) = paragraph.properties.section_break
        {
            let boundary = sections
                .iter()
                .find(|s| s.id == sid)
                .or_else(|| sections.first());
            if let Some(boundary) = boundary {
                points.push((i + 1, boundary));
            }
        }
    }
    points
}

/// Flows one section's block slice at its column width and appends its
/// [`SectionRun`]. An empty slice (a section that carries no body block of its own)
/// is skipped so it never emits a stray band.
fn push_section_run(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plan: &SectionPlan,
    boundary: &SectionBoundary,
    blocks: &[BlockNode],
    runs: &mut Vec<SectionRun>,
) {
    if blocks.is_empty() {
        return;
    }
    let config = plan.config;

    let layout = column_layout(&boundary.columns, config.content_area());
    let galley = build_galley_for_blocks(document, shaper, blocks, layout.flow_width());
    runs.push(SectionRun {
        config,
        layout,
        galley,
        starts_new_page: section_starts_new_page(boundary),
    });
}

/// Lays a whole [`Document`] out into a finished, ready-to-render
/// [`PaginatedLayout`](crate::page::PaginatedLayout) in one call — the single entry point the viewer and the
/// fidelity harness build on.
///
/// The pipeline, all composed from existing engine functions:
///
/// 1. Derive a section-local [`PageConfig`] and [`RunningContent`] plan for every
///    section, including inherited references and band reservations.
/// 3. Partition the body into per-section runs (`build_section_runs`), each flowed
///    at its own column width.
/// 4. [`paginate_columns`] the runs, then run the post-pagination passes **in
///    order** — section-scoped running-content placement, [`resolve_fields`],
///    [`place_floats`] —
///    the order the incremental-golden post-passes require (running content before
///    fields so a `Page X of Y` footer resolves; anchors last, off the pagination
///    hot path).
///
/// Header/footer variants, page-number fields, inline and anchored drawings, and
/// tables flow through the identical pipeline the manual wiring used; only body
/// pagination is now column- and section-aware (see the module docs and
/// [`crate::columns`]).
#[must_use]
pub fn paginate_document(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
) -> crate::page::PaginatedLayout {
    let plans = build_section_plans(document, shaper);
    let fallback_config = plans[0].config;

    // Build one paginated run per section, each flowed at its own column width,
    // then paginate them into shared pages (column-aware, section boundaries
    // carried across pages).
    let runs = build_section_runs(document, shaper, &plans);
    let mut layout = paginate_columns(&runs);

    // Post-pagination passes, in the required order: running content is placed
    // first so its fields exist to stamp, then the field pass resolves every
    // `PAGE`/`NUMPAGES` (body and running content), then anchored drawings are
    // placed onto the pages their paragraphs landed on.
    let mut section_page_numbers: BTreeMap<SectionId, u32> = BTreeMap::new();
    for page in &mut layout.pages {
        let section_page_number = section_page_numbers.entry(page.section).or_default();
        *section_page_number = section_page_number.saturating_add(1);
        let plan = plan_for_section(&plans, page.section);
        place_running_content_on_page(page, &plan.running, &plan.config, *section_page_number);
    }
    resolve_fields(&mut layout, shaper);
    // Floating objects last: anchored pictures, floating text boxes, and DrawingML
    // groups, over body AND header/footer bands, each resolved to a rect + z-key
    // for the float layer to paint in order.
    place_floats(&mut layout, document, shaper, &fallback_config);
    // A floating text box (e.g. the SDS footer's positioned `v:textbox` page-number
    // box) can itself hold `PAGE`/`NUMPAGES` fields; resolve them now that the
    // floats — and their flowed block content — exist on each page.
    resolve_anchored_fields(&mut layout, shaper);

    layout
}
