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
    BlockNode, Document, GroupChild, HeaderFooterKind, HeaderFooterRef, InlineNode, NoteId,
    NoteKind, SectionBoundary,
};

use crate::anchor::{header_float_reserve_for_section, place_floats};
use crate::columns::{
    ColumnLayout, SectionRun, column_layout, paginate_columns, section_starts_new_page,
};
use crate::flow::{build_galley_cached, build_galley_for_blocks, flow_header_footer};
use crate::incremental::{DirtySet, GalleyCache};
use crate::notes::{paginate_single_column_footnotes, run_has_body_footnotes};
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
        let blocks = body_with_appended_endnotes(document, body, body);
        let galley = build_galley_for_blocks(document, shaper, &blocks, layout.flow_width());
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
            InlineNode::Revision(revision) => collect_inline_endnotes(&revision.inlines, out),
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
    let column_galleys = if layout.has_unequal_widths() {
        layout
            .flow_widths()
            .into_iter()
            .map(|width| build_galley_for_blocks(document, shaper, blocks, width))
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
    // Build one paginated run per section, each flowed at its own column width,
    // then paginate them into shared pages (column-aware, section boundaries
    // carried across pages).
    let runs = build_section_runs(document, shaper, &plans);
    finish_pagination(document, shaper, &plans, &runs)
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
    finish_pagination(document, shaper, &plans, &runs)
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
) -> crate::page::PaginatedLayout {
    let fallback_config = plans[0].config;
    let mut layout = if runs.iter().all(|run| run.layout.is_single_column())
        && runs.iter().any(run_has_body_footnotes)
    {
        paginate_single_column_footnotes(document, shaper, runs)
    } else {
        paginate_columns(runs)
    };

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
        return build_section_runs(document, shaper, plans);
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
