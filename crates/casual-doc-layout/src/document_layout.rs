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

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Document, GroupChild, HeaderFooterKind, HeaderFooterRef, InlineNode, NoteId,
    NoteKind, PageBorders, PageVerticalAlignment, SectionBoundary,
};

use crate::anchor::{body_wrap_rects, header_float_reserve_for_section, place_floats};
use crate::block::{BlockFragment, CellFragment, CellVerticalMerge};
use crate::columns::{
    ColumnLayout, SectionRun, column_layout, paginate_columns, section_starts_new_page,
};
use crate::flow::{
    ParagraphFloatExclusion, ParagraphFloatExclusions, ReviewView, build_galley_cached,
    build_galley_for_blocks_inner, flow_header_footer,
};
use crate::incremental::{DirtySet, GalleyCache};
use crate::notes::{paginate_section_footnotes, run_has_body_footnotes};
use crate::paginate::{
    PageConfig, page_number_labels, resolve_anchored_fields_labeled, resolve_fields_labeled,
};
use crate::running::{HeaderFooter, RunningContent, place_running_content_on_page};
use crate::text::InlineFloatSide;
use crate::units::{Point, Rect, Size, Twip};
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
    page_borders: PageBorders,
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
        plans.push(SectionPlan {
            config,
            running,
            page_borders: section.page_borders.clone(),
        });
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
            page_borders: PageBorders::default(),
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
    review_view: ReviewView,
) -> Vec<SectionRun> {
    build_section_runs_inner(document, shaper, plans, None, review_view)
}

fn build_section_runs_with_exclusions(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
    exclusions: &ParagraphFloatExclusions,
    review_view: ReviewView,
) -> Vec<SectionRun> {
    build_section_runs_inner(document, shaper, plans, Some(exclusions), review_view)
}

fn build_section_runs_inner(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
    exclusions: Option<&ParagraphFloatExclusions>,
    review_view: ReviewView,
) -> Vec<SectionRun> {
    let sections = &document.definitions().sections;
    let body = document.body();
    if sections.is_empty() {
        let config = plans[0].config;
        let content = config.content_area();
        let layout = ColumnLayout::single(content);
        let blocks = body_with_appended_endnotes(document, body, body);
        let galley = build_body_galley(
            document,
            shaper,
            &blocks,
            layout.flow_width(),
            exclusions,
            review_view,
        );
        return vec![SectionRun {
            config,
            layout,
            galley,
            column_galleys: Vec::new(),
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
            exclusions,
            review_view,
            &mut runs,
        );
        start = end_excl;
    }
    // The trailing (body-level) final section covers everything left.
    if let Some(last) = sections.last() {
        let trailing = body_with_appended_endnotes(document, &body[start..], body);
        push_section_run(
            document,
            shaper,
            plan_for_section(plans, last.id),
            last,
            &trailing,
            exclusions,
            review_view,
            &mut runs,
        );
    }
    runs
}

fn body_with_appended_endnotes(
    document: &Document,
    blocks: &[BlockNode],
    reference_scope: &[BlockNode],
) -> Vec<BlockNode> {
    let endnotes = referenced_endnotes(reference_scope);
    if endnotes.is_empty() {
        return blocks.to_vec();
    }
    let mut out = blocks.to_vec();
    for id in endnotes {
        if let Some(note) = document.definitions().endnotes.get(&id) {
            out.extend(note.blocks.clone());
        }
    }
    out
}

fn referenced_endnotes(blocks: &[BlockNode]) -> Vec<NoteId> {
    let mut out = Vec::new();
    for block in blocks {
        collect_block_endnotes(block, &mut out);
    }
    out
}

fn push_unique_endnote(out: &mut Vec<NoteId>, note: NoteId) {
    if !out.contains(&note) {
        out.push(note);
    }
}

fn collect_block_endnotes(block: &BlockNode, out: &mut Vec<NoteId>) {
    match block {
        BlockNode::Paragraph(paragraph) => collect_inline_endnotes(&paragraph.inlines, out),
        BlockNode::Table(table) => {
            for row in &table.rows {
                for cell in &row.cells {
                    for block in &cell.blocks {
                        collect_block_endnotes(block, out);
                    }
                }
            }
        }
        BlockNode::Sdt(sdt) => {
            for block in &sdt.blocks {
                collect_block_endnotes(block, out);
            }
        }
        BlockNode::AltChunk(_) => {}
    }
}

fn collect_inline_endnotes(inlines: &[InlineNode], out: &mut Vec<NoteId>) {
    for inline in inlines {
        match inline {
            InlineNode::NoteReference(reference) if reference.kind == NoteKind::Endnote => {
                push_unique_endnote(out, reference.note);
            }
            InlineNode::Hyperlink(hyperlink) => collect_inline_endnotes(&hyperlink.inlines, out),
            InlineNode::Field(field) => collect_inline_endnotes(&field.inlines, out),
            InlineNode::TextBox(text_box) => {
                for block in &text_box.blocks {
                    collect_block_endnotes(block, out);
                }
            }
            InlineNode::Group(group) => collect_group_endnotes(&group.children, out),
            InlineNode::Revision(revision)
                if revision
                    .kind
                    .contributes_to(casual_doc_model::v1::ReviewProjection::FinalWithMarkup) =>
            {
                collect_inline_endnotes(&revision.inlines, out);
            }
            InlineNode::Revision(_) => {}
            InlineNode::Sdt(sdt) => collect_inline_endnotes(&sdt.inlines, out),
            InlineNode::Run(_)
            | InlineNode::Tab(_)
            | InlineNode::Break(_)
            | InlineNode::Drawing(_)
            | InlineNode::AnchoredDrawing(_)
            | InlineNode::EmbeddedObject(_)
            | InlineNode::CommentReference(_)
            | InlineNode::CommentRangeStart(_)
            | InlineNode::CommentRangeEnd(_)
            | InlineNode::BookmarkStart(_)
            | InlineNode::BookmarkEnd(_)
            | InlineNode::MoveRangeStart(_)
            | InlineNode::MoveRangeEnd(_)
            | InlineNode::Math(_)
            | InlineNode::Symbol(_)
            | InlineNode::HorizontalRule(_)
            | InlineNode::NoBreakHyphen(_)
            | InlineNode::SoftHyphen(_)
            | InlineNode::PositionalTab(_)
            | InlineNode::NoteReference(_) => {}
        }
    }
}

fn collect_group_endnotes(children: &[GroupChild], out: &mut Vec<NoteId>) {
    for child in children {
        match child {
            GroupChild::TextBox(text_box) => {
                for block in &text_box.blocks {
                    collect_block_endnotes(block, out);
                }
            }
            GroupChild::Group(group) => collect_group_endnotes(&group.children, out),
            GroupChild::Picture(_) | GroupChild::Shape(_) => {}
        }
    }
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
#[allow(clippy::too_many_arguments)]
fn push_section_run(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plan: &SectionPlan,
    boundary: &SectionBoundary,
    blocks: &[BlockNode],
    exclusions: Option<&ParagraphFloatExclusions>,
    review_view: ReviewView,
    runs: &mut Vec<SectionRun>,
) {
    if blocks.is_empty() {
        return;
    }
    let config = plan.config;

    let layout = column_layout(&boundary.columns, config.content_area());
    let galley = build_body_galley(
        document,
        shaper,
        blocks,
        layout.flow_width(),
        exclusions,
        review_view,
    );
    let column_galleys = if layout.has_unequal_widths() {
        layout
            .flow_widths()
            .into_iter()
            .map(|width| {
                build_body_galley(document, shaper, blocks, width, exclusions, review_view)
            })
            .collect()
    } else {
        Vec::new()
    };
    runs.push(SectionRun {
        config,
        layout,
        galley,
        column_galleys,
        starts_new_page: section_starts_new_page(boundary),
    });
}

fn build_body_galley(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    blocks: &[BlockNode],
    width: Twip,
    exclusions: Option<&ParagraphFloatExclusions>,
    review_view: ReviewView,
) -> Vec<BlockFragment> {
    build_galley_for_blocks_inner(document, shaper, blocks, width, exclusions, review_view)
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
///    order** — section-scoped running-content placement, `resolve_fields`
///    (with section-`pgNumType`-aware page-number labels),
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
    paginate_document_view(document, shaper, ReviewView::Editing)
}

/// [`paginate_document`] under an explicit [`ReviewView`] (docs/93). `Editing`
/// (the default entry) is the live-editor byte space; `Markup` produces a
/// **read-only** layout that shows struck deletions and author-colored/underlined
/// insertions + highlighted comment ranges — for a "show changes" viewer. The
/// markup layout's byte space differs (deletions are shown), so it must not drive
/// caret/selection; it is a render-only view.
#[must_use]
pub fn paginate_document_view(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    review_view: ReviewView,
) -> crate::page::PaginatedLayout {
    let plans = build_section_plans(document, shaper);
    // Build one paginated run per section, each flowed at its own column width,
    // then paginate them into shared pages (column-aware, section boundaries
    // carried across pages).
    let runs = build_section_runs(document, shaper, &plans, review_view);
    finish_pagination(document, shaper, &plans, &runs, review_view)
}

/// The incremental counterpart to [`paginate_document`]: identical output, but the
/// single-section body galley is built through `cache` so unchanged paragraphs are
/// **not re-shaped** — an edit is `O(edit)` instead of `O(document)`.
///
/// Shaping is ~99% of the layout cost on a large prose document, so reusing the
/// shaped lines of the untouched paragraphs is what keeps a keystroke's per-page
/// repaint under budget. `dirty` force-reshapes the listed nodes even if their
/// content hash is unchanged (a belt-and-suspenders over the hash); pass an empty
/// set to rely on hash invalidation alone, which is already correct.
///
/// Documents with explicit section breaks or multi-column sections fall back to a
/// full re-shape (each section's block slice would need its own cache); numbered
/// and text-box paragraphs always re-shape (see [`build_galley_cached`]). Every
/// post-pagination pass runs identically to [`paginate_document`], so the result is
/// byte-for-byte the same.
#[must_use]
pub fn paginate_document_cached(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    cache: &mut GalleyCache,
    dirty: &DirtySet,
) -> crate::page::PaginatedLayout {
    let plans = build_section_plans(document, shaper);
    let runs = build_section_runs_cached(document, shaper, &plans, cache, dirty);
    finish_pagination(document, shaper, &plans, &runs, ReviewView::Editing)
}

/// The shared pagination tail: paginate the section runs into pages, then run the
/// post-pagination passes in the required order. Both [`paginate_document`] and
/// [`paginate_document_cached`] funnel through here so the only difference between
/// them is how the section-run galleys were built.
fn finish_pagination(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
    runs: &[SectionRun],
    review_view: ReviewView,
) -> crate::page::PaginatedLayout {
    let mut layout = finish_pagination_pass(document, shaper, plans, runs);
    let mut exclusions = paragraph_float_exclusions(document, shaper, plans, &layout);
    if exclusions.is_empty() {
        return layout;
    }

    // Float placement and paragraph line measures depend on each other. Three
    // bounded passes cover the practical page-relative/grouped-shape cases while
    // making termination independent of document input.
    let mut previous_exclusions = exclusions.clone();
    for _ in 0..3 {
        let applied_exclusions = exclusions.clone();
        let runs =
            build_section_runs_with_exclusions(document, shaper, plans, &exclusions, review_view);
        let next = finish_pagination_pass(document, shaper, plans, &runs);
        let next_exclusions = paragraph_float_exclusions(document, shaper, plans, &next);
        if next_exclusions == exclusions {
            return next;
        }
        layout = next;
        previous_exclusions = applied_exclusions;
        exclusions = next_exclusions;
        if exclusions.is_empty() {
            return layout;
        }
    }

    // Non-convergence uses a conservative edge envelope: the widest exclusion
    // observed on either side persists for the greatest observed clearance. One
    // final pagination cannot paint text into a previously observed float band.
    let conservative = conservative_exclusions(&previous_exclusions, &exclusions);
    let runs =
        build_section_runs_with_exclusions(document, shaper, plans, &conservative, review_view);
    finish_pagination_pass(document, shaper, plans, &runs)
}

fn finish_pagination_pass(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
    runs: &[SectionRun],
) -> crate::page::PaginatedLayout {
    let fallback_config = plans[0].config;
    let mut layout = if runs.iter().any(run_has_body_footnotes) {
        paginate_section_footnotes(document, shaper, runs)
    } else {
        paginate_columns(runs)
    };

    // Section `w:vAlign` (center/both/bottom): shift each page's placed body
    // content within its content area. Runs first, before any pass reads body
    // positions (float exclusions, anchored placement, the display list), since
    // every glyph/image/text-box origin is relative to its fragment's rect.
    apply_page_vertical_alignment(&mut layout, &document.definitions().sections);

    // Post-pagination passes, in the required order: running content is placed
    // first so its fields exist to stamp, then the field pass resolves every
    // `PAGE`/`NUMPAGES` (body and running content), then anchored drawings are
    // placed onto the pages their paragraphs landed on.
    let mut section_page_numbers: BTreeMap<SectionId, u32> = BTreeMap::new();
    for page in &mut layout.pages {
        let section_page_number = section_page_numbers.entry(page.section).or_default();
        *section_page_number = section_page_number.saturating_add(1);
        let plan = plan_for_section(plans, page.section);
        place_running_content_on_page(page, &plan.running, &plan.config, *section_page_number);
        // Resolve this section's `w:pgBorders` into a per-page frame, off the hot
        // path like the running content above (docs/46 §F6c).
        page.page_borders = crate::page_border::resolve_page_borders(
            &plan.page_borders,
            *section_page_number,
            page.page_size,
            page.content_area,
        );
    }
    // Per-page `PAGE` labels honoring each section's `w:pgNumType` (@fmt format +
    // @start restart); the same labels feed the anchored-field pass below so a
    // floating page-number box matches the body/footer.
    let page_labels = page_number_labels(&layout, &document.definitions().sections);
    resolve_fields_labeled(&mut layout, &page_labels, shaper);
    // Floating objects last: anchored pictures, floating text boxes, and DrawingML
    // groups, over body AND header/footer bands, each resolved to a rect + z-key
    // for the float layer to paint in order.
    place_floats(&mut layout, document, shaper, &fallback_config);
    // A floating text box (e.g. the SDS footer's positioned `v:textbox` page-number
    // box) can itself hold `PAGE`/`NUMPAGES` fields; resolve them now that the
    // floats — and their flowed block content — exist on each page.
    resolve_anchored_fields_labeled(&mut layout, &page_labels, shaper);

    layout
}

/// Applies each section's `w:vAlign` to its pages: shifts the placed body
/// content within the page's content area. `Top` (Word's default) is a no-op;
/// `Center` centers the content block, `Bottom` pushes it to the bottom, and
/// `Both` distributes the vertical slack evenly between the placed blocks
/// (vertical justification). The slack is the content-area height minus the
/// content's own height; a page whose content fills (or overflows) the area has
/// no slack and is left untouched. Every glyph/image/text-box origin is relative
/// to its fragment's `rect`, so moving `rect.origin.y` moves the whole block.
fn apply_page_vertical_alignment(
    layout: &mut crate::page::PaginatedLayout,
    sections: &[SectionBoundary],
) {
    for page in &mut layout.pages {
        let Some(align) = sections
            .iter()
            .find(|section| section.id == page.section)
            .and_then(|section| section.vertical_alignment)
        else {
            continue;
        };
        if matches!(align, PageVerticalAlignment::Top) || page.placed.is_empty() {
            continue;
        }
        let content_top = page.content_area.origin.y;
        let content_bottom = content_top + page.content_area.size.height;
        // The content's own extent: the lowest placed-fragment bottom.
        let used_bottom = page
            .placed
            .iter()
            .map(|placed| placed.rect.origin.y + placed.rect.size.height)
            .max()
            .unwrap_or(content_top);
        let slack = content_bottom - used_bottom;
        if slack <= Twip::ZERO {
            continue;
        }
        match align {
            PageVerticalAlignment::Center => {
                let offset = Twip(slack.raw() / 2);
                for placed in &mut page.placed {
                    placed.rect.origin.y = placed.rect.origin.y + offset;
                }
            }
            PageVerticalAlignment::Bottom => {
                for placed in &mut page.placed {
                    placed.rect.origin.y = placed.rect.origin.y + slack;
                }
            }
            PageVerticalAlignment::Both => {
                // Vertical justification: spread the slack across the gaps between
                // the placed blocks (block `i` of `n` moves down by `slack*i/(n-1)`).
                // A single block has no gap and stays at the top.
                let gaps = page.placed.len().saturating_sub(1);
                if gaps == 0 {
                    continue;
                }
                for (index, placed) in page.placed.iter_mut().enumerate() {
                    let offset = Twip((slack.raw() as i64 * index as i64 / gaps as i64) as i32);
                    placed.rect.origin.y = placed.rect.origin.y + offset;
                }
            }
            PageVerticalAlignment::Top => {}
        }
    }
}

fn paragraph_float_exclusions(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
    layout: &crate::page::PaginatedLayout,
) -> ParagraphFloatExclusions {
    let wraps = body_wrap_rects(layout, document, shaper, &plans[0].config);
    if wraps.is_empty() {
        return ParagraphFloatExclusions::new();
    }

    let body_order: BTreeMap<NodeId, usize> = document
        .body()
        .iter()
        .enumerate()
        .filter_map(|(index, block)| match block {
            BlockNode::Paragraph(paragraph) => Some((paragraph.id, index)),
            _ => None,
        })
        .collect();
    // Top-level body tables, so a page-relative float's exclusion can descend
    // into their cells' paragraphs (P1F-FLOAT-SQUARE-2) without also reaching
    // note-body or running-content tables, which share the same placed-fragment
    // list but are not real body-order content (mirroring `body_order` above).
    let body_table_ids: BTreeSet<NodeId> = document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Table(table) => Some(table.id),
            _ => None,
        })
        .collect();
    let mut paragraphs = Vec::new();
    for (page_index, page) in layout.pages.iter().enumerate() {
        for placed in &page.placed {
            match &placed.fragment {
                BlockFragment::Paragraph { id, .. } if body_order.contains_key(id) => {
                    paragraphs.push((page_index, *id, placed.rect));
                }
                BlockFragment::TableRow { table, cells, .. } if body_table_ids.contains(table) => {
                    collect_cell_paragraph_rects(cells, placed.rect, page_index, &mut paragraphs);
                }
                _ => {}
            }
        }
    }

    let mut exclusions = ParagraphFloatExclusions::new();
    for wrap in wraps {
        if !body_order.contains_key(&wrap.source) {
            continue;
        }
        let left = wrap.rect.origin.x - emu_to_twip(wrap.distances.start_emu);
        let right = wrap.rect.right() + emu_to_twip(wrap.distances.end_emu);
        let top = wrap.rect.origin.y - emu_to_twip(wrap.distances.top_emu);
        let bottom = wrap.rect.bottom() + emu_to_twip(wrap.distances.bottom_emu);
        for (page_index, paragraph, rect) in &paragraphs {
            // Page- and margin-relative objects can sit above their anchoring
            // paragraph (the right arrow in demo.docx is one such object), so
            // every overlapping top-level paragraph on the resolved page must be
            // considered. The bounded fixed point below makes that backward
            // dependency deterministic.
            if *page_index != wrap.page_index
                || top > rect.origin.y
                || bottom <= rect.origin.y
                || right <= rect.origin.x
                || left >= rect.right()
            {
                continue;
            }
            let paragraph_mid = rect.origin.x.raw() + rect.size.width.raw() / 2;
            let float_mid = left.raw() + (right.raw() - left.raw()) / 2;
            let (side, raw_width) = if float_mid <= paragraph_mid {
                (InlineFloatSide::Left, right.raw() - rect.origin.x.raw())
            } else {
                (InlineFloatSide::Right, rect.right().raw() - left.raw())
            };
            let max_width = (rect.size.width.raw() - 1).max(1);
            let exclusion = ParagraphFloatExclusion {
                side,
                width: Twip(raw_width.clamp(1, max_width)),
                height: Twip((bottom.raw() - rect.origin.y.raw()).max(1)),
            };
            let values = exclusions.entry(*paragraph).or_default();
            if !values.contains(&exclusion) {
                values.push(exclusion);
            }
        }
    }
    exclusions
}

/// Appends every paragraph fragment's absolute page rect nested under a
/// top-level table row's cells to `out`, so a page-relative float's exclusion
/// zone can reach text inside a table cell — not only top-level body
/// paragraphs. Descends through nested tables (a table inside a cell) to
/// arbitrary depth.
///
/// The geometry mirrors [`crate::compose::compose_page`]'s cell-content
/// placement exactly (top/bottom margin inset, `w:vAlign` slack, block
/// stacking by [`BlockFragment::height`]) so a computed exclusion always lines
/// up with what the page actually paints; a merged-away `w:vMerge` continuation
/// cell contributes no box, matching the painter.
fn collect_cell_paragraph_rects(
    cells: &[CellFragment],
    row_rect: Rect,
    page_index: usize,
    out: &mut Vec<(usize, NodeId, Rect)>,
) {
    for cell in cells {
        if matches!(cell.vertical_merge, CellVerticalMerge::Continue) {
            continue;
        }
        let cell_height = cell.box_height(row_rect.size.height);
        let content_width =
            Twip((cell.width.raw() - cell.margins.start.raw() - cell.margins.end.raw()).max(1));
        let x = row_rect.origin.x + cell.x + cell.margins.start;
        let mut y = row_rect.origin.y + cell.cell_spacing.top + cell.content_y_offset(cell_height);
        for block in &cell.blocks {
            collect_block_paragraph_rects(block, Point::new(x, y), content_width, page_index, out);
            y = y + block.height();
        }
    }
}

/// One block's contribution to [`collect_cell_paragraph_rects`]: a paragraph
/// records its rect directly; a nested table row recurses into its own cells.
fn collect_block_paragraph_rects(
    block: &BlockFragment,
    origin: Point,
    width: Twip,
    page_index: usize,
    out: &mut Vec<(usize, NodeId, Rect)>,
) {
    match block {
        BlockFragment::Paragraph { id, .. } => {
            out.push((
                page_index,
                *id,
                Rect::new(origin, Size::new(width, block.height())),
            ));
        }
        BlockFragment::TableRow { cells, .. } => {
            let row_rect = Rect::new(origin, Size::new(width, block.height()));
            collect_cell_paragraph_rects(cells, row_rect, page_index, out);
        }
    }
}

fn conservative_exclusions(
    first: &ParagraphFloatExclusions,
    second: &ParagraphFloatExclusions,
) -> ParagraphFloatExclusions {
    let mut result = ParagraphFloatExclusions::new();
    for source in [first, second] {
        for (paragraph, exclusions) in source {
            for exclusion in exclusions {
                let values = result.entry(*paragraph).or_default();
                if let Some(existing) = values
                    .iter_mut()
                    .find(|existing| existing.side == exclusion.side)
                {
                    existing.width = existing.width.max(exclusion.width);
                    existing.height = existing.height.max(exclusion.height);
                } else {
                    values.push(*exclusion);
                }
            }
        }
    }
    result
}

fn emu_to_twip(emu: i64) -> Twip {
    Twip((emu / 635).clamp(0, i64::from(i32::MAX)) as i32)
}

/// [`build_section_runs`], but the common **single-section** body is flowed through
/// the galley `cache` so unchanged paragraphs are reused rather than re-shaped.
/// Documents that carry explicit section breaks or referenced endnotes re-shape
/// fully (each section slice or synthetic endnote appendix would need its own
/// cache, and these are rarer than plain body edits).
fn build_section_runs_cached(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
    cache: &mut GalleyCache,
    dirty: &DirtySet,
) -> Vec<SectionRun> {
    if !document.definitions().sections.is_empty()
        || !referenced_endnotes(document.body()).is_empty()
    {
        return build_section_runs(document, shaper, plans, ReviewView::Editing);
    }
    // Single-section fast path: one full-width run over the whole body, built
    // incrementally. Mirrors the `sections.is_empty()` arm of `build_section_runs`,
    // swapping `build_galley_for_blocks` for the cached builder.
    let config = plans[0].config;
    let layout = ColumnLayout::single(config.content_area());
    let galley = build_galley_cached(document, shaper, layout.flow_width(), cache, dirty);
    vec![SectionRun {
        config,
        layout,
        galley,
        column_galleys: Vec::new(),
        starts_new_page: true,
    }]
}

#[cfg(test)]
mod cached_pagination_tests {
    use super::*;
    use crate::shape::ParleyShaper;
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::{
        BlockNode, Definitions, InlineNode, Note, NoteId, NoteKind, NoteReference, Paragraph,
        ParagraphProperties, Run, RunProperties,
    };

    fn node(id: u64) -> NodeId {
        NodeId::from_parts(id, 1).unwrap()
    }

    /// A body of paragraphs, each `texts[i]` as one run — the i-th paragraph keeps a
    /// stable node id across edits so the galley cache can key on it.
    fn doc(texts: &[&str]) -> Document {
        let body = texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let id = i as u64 + 1;
                BlockNode::Paragraph(Paragraph {
                    id: node(id),
                    properties: ParagraphProperties::default(),
                    inlines: vec![InlineNode::Run(Run {
                        id: node(id + 1_000),
                        properties: RunProperties::default(),
                        text: (*text).to_owned(),
                    })],
                })
            })
            .collect();
        Document::new(node(9_000), body, Definitions::default()).unwrap()
    }

    fn doc_with_endnote() -> Document {
        let endnote = NoteId::new(node(9_100));
        let mut definitions = Definitions::default();
        definitions.endnotes.insert(
            endnote,
            Note {
                blocks: vec![BlockNode::Paragraph(Paragraph {
                    id: node(9_101),
                    properties: ParagraphProperties::default(),
                    inlines: vec![InlineNode::Run(Run {
                        id: node(9_102),
                        properties: RunProperties::default(),
                        text: "cached endnote body".to_owned(),
                    })],
                })],
            },
        );
        let body = vec![
            BlockNode::Paragraph(Paragraph {
                id: node(9_103),
                properties: ParagraphProperties::default(),
                inlines: vec![InlineNode::Run(Run {
                    id: node(9_104),
                    properties: RunProperties::default(),
                    text: "body".to_owned(),
                })],
            }),
            BlockNode::Paragraph(Paragraph {
                id: node(9_105),
                properties: ParagraphProperties::default(),
                inlines: vec![InlineNode::NoteReference(NoteReference {
                    id: node(9_106),
                    kind: NoteKind::Endnote,
                    note: endnote,
                })],
            }),
        ];
        Document::new(node(9_107), body, definitions).unwrap()
    }

    // Enough prose to span several pages, so an edit that reuses cached paragraphs
    // genuinely skips work the full path would redo.
    fn prose(n: usize) -> Vec<String> {
        (0..n)
            .map(|i| format!("Paragraph {i}. The quick brown fox jumps over the lazy dog."))
            .collect()
    }

    /// The whole point: after a realistic single-paragraph edit, the incremental
    /// path must produce the byte-for-byte same pagination as a full re-shape —
    /// while re-shaping only the one paragraph that changed.
    #[test]
    fn cached_matches_full_after_a_paragraph_edit() {
        let shaper = ParleyShaper::new();
        let before: Vec<String> = prose(60);
        let doc_before = doc(&before.iter().map(String::as_str).collect::<Vec<_>>());

        // Warm the cache on the pre-edit document (mirrors an open + first paint).
        let mut cache = GalleyCache::new();
        let warm =
            paginate_document_cached(&doc_before, &shaper, &mut cache, &DirtySet::everything());
        assert_eq!(
            warm,
            paginate_document(&doc_before, &shaper),
            "a full-dirty cached build must equal the fresh build"
        );

        // Edit one paragraph's text (its node id is unchanged, its content hash is not).
        let mut after = before.clone();
        after[30] = "Paragraph 30. EDITED — a longer line that rewraps this paragraph.".to_owned();
        let doc_after = doc(&after.iter().map(String::as_str).collect::<Vec<_>>());

        let cached = paginate_document_cached(&doc_after, &shaper, &mut cache, &DirtySet::new());
        let full = paginate_document(&doc_after, &shaper);
        assert_eq!(
            cached, full,
            "incremental re-pagination diverged from a full re-shape"
        );
        assert_eq!(
            cache.shaped_last_build(),
            1,
            "only the edited paragraph should have been re-shaped"
        );
    }

    /// Inserting a paragraph (a new node id) reuses the cached fragments of the
    /// paragraphs that did not move and shapes only the newcomer — and still equals
    /// a full re-shape, so a structural edit is incremental and correct.
    #[test]
    fn cached_matches_full_after_a_paragraph_insert() {
        let shaper = ParleyShaper::new();
        let before = prose(40);
        let doc_before = doc(&before.iter().map(String::as_str).collect::<Vec<_>>());
        let mut cache = GalleyCache::new();
        let _ = paginate_document_cached(&doc_before, &shaper, &mut cache, &DirtySet::everything());

        // Splice a brand-new paragraph in the middle. Its node id (900) is not in
        // the cache, so it shapes; every original paragraph keeps its id and hits.
        let mut blocks = doc_before.body().to_vec();
        blocks.insert(
            20,
            BlockNode::Paragraph(Paragraph {
                id: node(900),
                properties: ParagraphProperties::default(),
                inlines: vec![InlineNode::Run(Run {
                    id: node(1_900),
                    properties: RunProperties::default(),
                    text: "A newly inserted paragraph of prose.".to_owned(),
                })],
            }),
        );
        let doc_after = Document::new(node(9_000), blocks, Definitions::default()).unwrap();

        let cached = paginate_document_cached(&doc_after, &shaper, &mut cache, &DirtySet::new());
        assert_eq!(cached, paginate_document(&doc_after, &shaper));
        assert_eq!(
            cache.shaped_last_build(),
            1,
            "only the inserted paragraph should have been shaped"
        );
    }

    #[test]
    fn cached_matches_full_when_endnotes_are_appended() {
        let shaper = ParleyShaper::new();
        let doc = doc_with_endnote();
        let mut cache = GalleyCache::new();

        let cached = paginate_document_cached(&doc, &shaper, &mut cache, &DirtySet::everything());
        let full = paginate_document(&doc, &shaper);

        assert_eq!(
            cached, full,
            "cached pagination must preserve the synthetic endnote appendix"
        );
        assert!(
            cached
                .pages
                .iter()
                .flat_map(|page| page.placed.iter())
                .any(|placed| placed.fragment.node_id() == node(9_101)),
            "the referenced endnote body should remain visible through the cached entry point"
        );
    }
}

#[cfg(test)]
mod cross_paragraph_float_tests {
    use super::*;
    use crate::shape::ParleyShaper;
    use casual_doc_model::v1::{
        AnchorHorizontal, AnchorVertical, AnchoredDrawing, Definitions, DrawingAnchor, Extent,
        GridColumn, HorizontalAlign, HorizontalAnchor, HorizontalPosition, InlineNode, MediaId,
        MediaReference, Paragraph, ParagraphProperties, Run, RunProperties, Table, TableCell,
        TableCellProperties, TableProperties, TableRow, TableRowProperties, VerticalAlign,
        VerticalAnchor, VerticalPosition, WrapDistances, WrapMode,
    };

    fn node(id: u64) -> NodeId {
        NodeId::from_parts(81, id).unwrap()
    }

    fn paragraph(id: u64, text: String, extra: Vec<InlineNode>) -> BlockNode {
        let mut inlines = vec![InlineNode::Run(Run {
            id: node(id + 100),
            properties: RunProperties::default(),
            text,
        })];
        inlines.extend(extra);
        BlockNode::Paragraph(Paragraph {
            id: node(id),
            properties: ParagraphProperties::default(),
            inlines,
        })
    }

    fn floating_document() -> (Document, NodeId) {
        let media = MediaId::new(node(900));
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rIdFloat".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/float.png".to_owned(),
            },
        );
        let drawing = InlineNode::AnchoredDrawing(AnchoredDrawing {
            id: node(901),
            media,
            extent: Extent {
                width_emu: 1_500 * 635,
                height_emu: 1_800 * 635,
            },
            anchor: DrawingAnchor {
                horizontal: AnchorHorizontal {
                    relative_from: HorizontalAnchor::Margin,
                    position: HorizontalPosition::Align(HorizontalAlign::Left),
                },
                vertical: AnchorVertical {
                    relative_from: VerticalAnchor::Paragraph,
                    position: VerticalPosition::Align(VerticalAlign::Top),
                },
                wrap: WrapMode::Square,
                wrap_distances: WrapDistances::default(),
                behind_doc: false,
            },
            descr: None,
            relative_height: None,
            crop: None,
        });
        let target = node(2);
        let prose = "following paragraph text wraps beside the floating object ".repeat(90);
        let document = Document::new(
            node(999),
            vec![
                paragraph(1, "anchor".to_owned(), vec![drawing]),
                paragraph(2, prose, Vec::new()),
                paragraph(3, "after".to_owned(), Vec::new()),
            ],
            definitions,
        )
        .unwrap();
        (document, target)
    }

    fn backward_margin_float_document() -> (Document, NodeId) {
        let media = MediaId::new(node(920));
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rIdBackwardFloat".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/backward-float.png".to_owned(),
            },
        );
        let drawing = InlineNode::AnchoredDrawing(AnchoredDrawing {
            id: node(921),
            media,
            extent: Extent {
                width_emu: 1_500 * 635,
                height_emu: 1_800 * 635,
            },
            anchor: DrawingAnchor {
                horizontal: AnchorHorizontal {
                    relative_from: HorizontalAnchor::Margin,
                    position: HorizontalPosition::Align(HorizontalAlign::Left),
                },
                vertical: AnchorVertical {
                    relative_from: VerticalAnchor::Margin,
                    position: VerticalPosition::Align(VerticalAlign::Top),
                },
                wrap: WrapMode::Square,
                wrap_distances: WrapDistances::default(),
                behind_doc: false,
            },
            descr: None,
            relative_height: None,
            crop: None,
        });
        let target = node(11);
        let document = Document::new(
            node(998),
            vec![
                paragraph(
                    11,
                    "paragraph before its later anchor must wrap around the margin object "
                        .repeat(18),
                    Vec::new(),
                ),
                paragraph(12, "later anchor".to_owned(), vec![drawing]),
            ],
            definitions,
        )
        .unwrap();
        (document, target)
    }

    #[test]
    fn square_float_excludes_intersecting_lines_in_following_paragraph() {
        let (document, target) = floating_document();
        let shaper = ParleyShaper::new();
        let layout = paginate_document(&document, &shaper);
        assert_eq!(
            layout,
            paginate_document(&document, &shaper),
            "bounded float reflow must be deterministic"
        );

        let page = &layout.pages[0];
        let float = page.anchored.first().expect("placed anchored drawing");
        let placed = page
            .placed
            .iter()
            .find(|placed| placed.fragment.node_id() == target)
            .expect("following paragraph");
        let BlockFragment::Paragraph { lines, .. } = &placed.fragment else {
            panic!("target should be a paragraph");
        };

        let mut shifted = 0;
        let mut restored = 0;
        for line in &lines.lines {
            let Some(run) = line.runs.first() else {
                continue;
            };
            let baseline = placed.rect.origin.y + run.origin.y;
            let x = placed.rect.origin.x + run.origin.x;
            if baseline < float.rect.bottom() {
                assert!(
                    x >= float.rect.right(),
                    "line at y={} starts at {}, inside float ending at {}",
                    baseline.raw(),
                    x.raw(),
                    float.rect.right().raw()
                );
                shifted += 1;
            } else if run.origin.x == Twip::ZERO {
                restored += 1;
            }
        }
        assert!(
            shifted >= 2,
            "the float should affect multiple following lines"
        );
        assert!(
            restored >= 1,
            "full paragraph measure should return below the float"
        );

        let mut cache = GalleyCache::new();
        let cached =
            paginate_document_cached(&document, &shaper, &mut cache, &DirtySet::everything());
        assert_eq!(
            cached, layout,
            "cached entry point uses the same fixed point"
        );
    }

    #[test]
    fn margin_float_can_exclude_a_paragraph_before_its_anchor() {
        let (document, target) = backward_margin_float_document();
        let layout = paginate_document(&document, &ParleyShaper::new());
        let page = &layout.pages[0];
        let float = page.anchored.first().expect("placed anchored drawing");
        let placed = page
            .placed
            .iter()
            .find(|placed| placed.fragment.node_id() == target)
            .expect("paragraph before the anchor");
        let BlockFragment::Paragraph { lines, .. } = &placed.fragment else {
            panic!("target should be a paragraph");
        };

        let intersecting: Vec<_> = lines
            .lines
            .iter()
            .filter_map(|line| line.runs.first())
            .filter(|run| placed.rect.origin.y + run.origin.y < float.rect.bottom())
            .collect();
        assert!(
            intersecting.len() >= 2,
            "the margin float should intersect multiple earlier lines"
        );
        assert!(
            intersecting
                .iter()
                .all(|run| { placed.rect.origin.x + run.origin.x >= float.rect.right() })
        );
    }

    /// A page/margin-relative float anchored in an ordinary paragraph, followed
    /// immediately by a one-cell table whose cell paragraph overlaps the float's
    /// resolved band. Returns the document, the table's node id, and the cell
    /// paragraph's node id (the exclusion target).
    fn margin_float_over_table_cell_document() -> (Document, NodeId, NodeId) {
        let media = MediaId::new(node(940));
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rIdTableFloat".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/table-float.png".to_owned(),
            },
        );
        let drawing = InlineNode::AnchoredDrawing(AnchoredDrawing {
            id: node(941),
            media,
            extent: Extent {
                width_emu: 1_500 * 635,
                height_emu: 1_800 * 635,
            },
            anchor: DrawingAnchor {
                horizontal: AnchorHorizontal {
                    relative_from: HorizontalAnchor::Margin,
                    position: HorizontalPosition::Align(HorizontalAlign::Left),
                },
                vertical: AnchorVertical {
                    relative_from: VerticalAnchor::Margin,
                    position: VerticalPosition::Align(VerticalAlign::Top),
                },
                wrap: WrapMode::Square,
                wrap_distances: WrapDistances::default(),
                behind_doc: false,
            },
            descr: None,
            relative_height: None,
            crop: None,
        });
        let table_id = node(950);
        let cell_paragraph = node(953);
        let table = BlockNode::Table(Table {
            id: table_id,
            grid: vec![GridColumn {
                width_twips: Some(9_000),
            }],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![TableRow {
                id: node(951),
                properties: TableRowProperties::default(),
                cells: vec![TableCell {
                    id: node(952),
                    properties: TableCellProperties::default(),
                    blocks: vec![BlockNode::Paragraph(Paragraph {
                        id: cell_paragraph,
                        properties: ParagraphProperties::default(),
                        inlines: vec![InlineNode::Run(Run {
                            id: node(954),
                            properties: RunProperties::default(),
                            text: "table cell text wraps beside the page relative float "
                                .repeat(20),
                        })],
                    })],
                }],
            }],
        });
        let document = Document::new(
            node(939),
            vec![paragraph(1, "anchor".to_owned(), vec![drawing]), table],
            definitions,
        )
        .unwrap();
        (document, table_id, cell_paragraph)
    }

    /// P1F-FLOAT-SQUARE-2: a page/margin-relative float's exclusion zone now
    /// reaches a paragraph nested inside a table cell, not just top-level body
    /// paragraphs.
    #[test]
    fn margin_float_excludes_a_paragraph_nested_in_a_table_cell() {
        let (document, table_id, target) = margin_float_over_table_cell_document();
        let shaper = ParleyShaper::new();
        let layout = paginate_document(&document, &shaper);

        let page = &layout.pages[0];
        let float = page.anchored.first().expect("placed anchored drawing");
        let placed_row = page
            .placed
            .iter()
            .find(|placed| {
                matches!(&placed.fragment, BlockFragment::TableRow { table, .. } if *table == table_id)
            })
            .expect("table row placed on the float's page");
        let BlockFragment::TableRow { cells, .. } = &placed_row.fragment else {
            unreachable!("matched above")
        };
        let cell = &cells[0];
        let cell_height = cell.box_height(placed_row.fragment.height());
        let content_origin = Point::new(
            placed_row.rect.origin.x + cell.x + cell.margins.start,
            placed_row.rect.origin.y + cell.cell_spacing.top + cell.content_y_offset(cell_height),
        );
        let BlockFragment::Paragraph { id, lines, .. } = &cell.blocks[0] else {
            panic!("expected the cell's paragraph fragment");
        };
        assert_eq!(*id, target);

        let mut shifted = 0;
        let mut restored = 0;
        for line in &lines.lines {
            let Some(run) = line.runs.first() else {
                continue;
            };
            let baseline = content_origin.y + run.origin.y;
            let x = content_origin.x + run.origin.x;
            if baseline < float.rect.bottom() {
                assert!(
                    x >= float.rect.right(),
                    "cell line at y={} starts at {}, inside float ending at {}",
                    baseline.raw(),
                    x.raw(),
                    float.rect.right().raw()
                );
                shifted += 1;
            } else if run.origin.x == Twip::ZERO {
                restored += 1;
            }
        }
        assert!(
            shifted >= 1,
            "the float should exclude at least one line inside the table cell"
        );
        assert!(
            restored >= 1,
            "the cell's full width should return below the float"
        );

        assert_eq!(
            layout,
            paginate_document(&document, &shaper),
            "bounded float reflow into a table cell must be deterministic"
        );
        let mut cache = GalleyCache::new();
        let cached =
            paginate_document_cached(&document, &shaper, &mut cache, &DirtySet::everything());
        assert_eq!(
            cached, layout,
            "cached entry point uses the same fixed point"
        );
    }
}
