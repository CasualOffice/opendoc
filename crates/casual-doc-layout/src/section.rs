//! Multi-section pagination — laying a document whose body is divided into
//! sections, each with its own page geometry, columns, and header/footer set.
//!
//! A DOCX body is a sequence of **sections** (`w:sectPr`): a landscape insert, a
//! change of margins or columns, a different running-header set — each is a new
//! section with its own [`PageConfig`]. The single-section paginator
//! ([`crate::paginate::paginate`]) takes one geometry and one galley; this module
//! drives it once **per section** and stitches the per-section page lists into one
//! [`PaginatedLayout`].
//!
//! ## Why a per-section decomposition (and not one combined walk)
//!
//! The single-section paginator's crown jewel is its **bounded incremental
//! re-pagination** with the golden invariant `repaginate == paginate` (the
//! stabilization halt, `docs/43-…` §3.4). Re-implementing that over a combined,
//! multi-geometry galley would mean teaching the halt/prefix/splice core about
//! geometry changes mid-stream — exactly the kind of rework that risks the
//! invariant.
//!
//! Instead a **section boundary is a hard break** (a new section always begins a
//! fresh page, except `continuous`), so each section paginates *independently*
//! over its own fragments under its own geometry. This makes the boundary a
//! natural clean resume point and stabilization point **for free**:
//!
//! - [`repaginate_sections`] re-paginates each section with the untouched
//!   [`crate::paginate::repaginate`] core, so the halt/prefix/splice logic is
//!   reused verbatim — never extended, never at risk.
//! - Because a section's [`repaginate`](crate::paginate::repaginate) only ever
//!   sees that one section's galley and config, a page from section A's geometry
//!   **can never be spliced into section B** — the cross-geometry hazard is ruled
//!   out *structurally*, not by an added guard.
//! - `repaginate_sections == paginate_sections` then holds by composition: both
//!   are `stitch(per-section pagination)`, and per section
//!   `repaginate == paginate` already holds field-for-field.
//!
//! A page's provenance is unambiguous even though fragment indices are
//! section-local: every [`Page`] records its [`section`](Page::section), so a page
//! is identified by `(section, flow)`, never by a bare galley index.
//!
//! Page numbering, `evenPage`/`oddPage` parity blanks, and per-section
//! `w:pgNumType` restarts are applied in `stitch` — a pure function of the
//! per-section page lists, exactly like the [`crate::running`] and
//! [`crate::paginate::resolve_fields`] post-passes — so they compose cleanly with
//! the incremental path and leave the pagination hot path untouched.

use std::collections::HashMap;

use casual_doc_model::v1::{SectionId, SectionType};

use crate::block::BlockFragment;
use crate::model::ModelPos;
use crate::page::{FlowPos, FlowSpan, Page, PaginatedLayout};
use crate::paginate::{PageConfig, RepaginateStats, paginate, repaginate_with_stats};

/// One section of a document: its page geometry, the galley of block fragments
/// that belong to it (already shaped at *this* section's content width), how the
/// section begins (`w:type`), and its page-number restart (`w:pgNumType`).
///
/// Build these with [`crate::flow::build_sections`] from a document, or construct
/// them directly for tests/embedders.
#[derive(Clone, Debug)]
pub struct Section {
    /// This section's page geometry (page box, margins, header/footer bands).
    pub config: PageConfig,
    /// The section's flow content, shaped at its own content width.
    pub fragments: Vec<BlockFragment>,
    /// Where the section begins (`w:type`): `nextPage` (default), `continuous`,
    /// `evenPage`, `oddPage`, `nextColumn`.
    pub break_type: SectionType,
    /// Restarts page numbering at this value on the section's first page
    /// (`w:pgNumType/@w:start`); `None` continues the running count.
    pub page_number_start: Option<u32>,
}

impl Section {
    /// A section with the default `nextPage` break and continuous page numbering.
    #[must_use]
    pub fn new(config: PageConfig, fragments: Vec<BlockFragment>) -> Self {
        Self {
            config,
            fragments,
            break_type: SectionType::NextPage,
            page_number_start: None,
        }
    }
}

/// Paginates a multi-section document: each section is laid out under its own
/// geometry, then the per-section page lists are stitched into one layout with
/// global page numbers, `evenPage`/`oddPage` parity blanks, and any per-section
/// `w:pgNumType` restarts (see `stitch`).
///
/// The single-section [`paginate`] is the one-section case of this
/// (`paginate_sections(&[Section::new(config, fragments)])` differs from
/// `paginate(&fragments, &config)` only in that `stitch` re-stamps page numbers to
/// the same 1-based values — the placements are identical).
#[must_use]
pub fn paginate_sections(sections: &[Section]) -> PaginatedLayout {
    let per: Vec<Vec<Page>> = sections
        .iter()
        .map(|s| paginate(&s.fragments, &s.config).pages)
        .collect();
    stitch(sections, per)
}

/// Re-paginates a multi-section document incrementally, preserving the golden
/// invariant `repaginate_sections == paginate_sections`.
///
/// Each section is matched to its previous layout **by [`SectionId`]** and
/// re-paginated with the untouched [`crate::paginate::repaginate`] core, so:
///
/// - an edit confined to one section re-flows only that section (the others are
///   reused, their internal layout unchanged — only their page numbers shift in
///   `stitch`, which is a pure post-pass);
/// - a section whose page count changed still lets its *own* stabilization halt
///   reuse its unchanged tail; and
/// - because each section's re-pagination only ever sees its own galley + config,
///   no page is ever spliced across a geometry change.
///
/// A section with no matching previous section (added/reordered) or whose geometry
/// changed falls back to a full [`paginate`] for that section — always correct,
/// simply reusing nothing (mirroring [`crate::paginate::repaginate`]'s own
/// geometry-change fallback).
#[must_use]
pub fn repaginate_sections(
    prev: &PaginatedLayout,
    prev_sections: &[Section],
    new_sections: &[Section],
) -> PaginatedLayout {
    repaginate_sections_with_stats(prev, prev_sections, new_sections).0
}

/// [`repaginate_sections`] plus the per-section [`RepaginateStats`] (in
/// `new_sections` order) — the incremental cost of each section's re-pagination,
/// for telemetry and for asserting work stays bounded to the edited section.
#[must_use]
pub fn repaginate_sections_with_stats(
    prev: &PaginatedLayout,
    prev_sections: &[Section],
    new_sections: &[Section],
) -> (PaginatedLayout, Vec<RepaginateStats>) {
    // Recover each previous section's page list from the flat layout: pages are
    // contiguous per section and every page names its `section`, so grouping by id
    // (in page order) reconstructs exactly what `paginate` produced for it. Parity
    // blanks (empty `placed`, an artifact of `stitch`) are skipped — they are
    // recomputed fresh, never fed back into a section's `repaginate`.
    let mut prev_pages: HashMap<SectionId, Vec<Page>> = HashMap::new();
    for page in &prev.pages {
        if page.placed.is_empty() {
            continue;
        }
        prev_pages
            .entry(page.section)
            .or_default()
            .push(page.clone());
    }
    let prev_frags: HashMap<SectionId, &[BlockFragment]> = prev_sections
        .iter()
        .map(|s| (s.config.section, s.fragments.as_slice()))
        .collect();

    let mut per = Vec::with_capacity(new_sections.len());
    let mut stats = Vec::with_capacity(new_sections.len());
    for section in new_sections {
        let id = section.config.section;
        let prev_layout = PaginatedLayout {
            pages: prev_pages.get(&id).cloned().unwrap_or_default(),
        };
        let prev_fragments: &[BlockFragment] = prev_frags.get(&id).copied().unwrap_or(&[]);
        let (layout, st) = repaginate_with_stats(
            &prev_layout,
            prev_fragments,
            &section.fragments,
            &section.config,
        );
        per.push(layout.pages);
        stats.push(st);
    }
    (stitch(new_sections, per), stats)
}

/// Stitches per-section page lists (in section order) into one layout: assigns
/// global 1-based page numbers, honors each section's `w:pgNumType` restart, and
/// inserts a blank page before an `evenPage`/`oddPage` section when its first page
/// would otherwise land on the wrong parity.
///
/// This is a **pure function of the per-section page lists and each section's
/// metadata** — it re-stamps `Page::number` and inserts parity blanks but never
/// changes a page's placements or flow span. So it produces identical output after
/// a full [`paginate_sections`] and an incremental [`repaginate_sections`],
/// which is what keeps `repaginate_sections == paginate_sections`.
///
/// ## Break-type handling
///
/// - `nextPage` (default), `continuous`, `nextColumn`: the section begins on a
///   fresh page — the natural result of paginating each section independently. A
///   true `continuous` merge (section content flowing onto the previous section's
///   last page) and real multi-column `nextColumn` advance are follow-ups; today
///   both begin a new page (a documented simplification, correct if coarser than
///   Word for compatible geometry).
/// - `evenPage` / `oddPage`: if the section's first page would not have the
///   required parity, a blank page is inserted so it does.
fn stitch(sections: &[Section], per: Vec<Vec<Page>>) -> PaginatedLayout {
    let mut pages = Vec::new();
    let mut next = 1u32;
    for (index, (section, section_pages)) in sections.iter().zip(per).enumerate() {
        // `w:pgNumType/@w:start` restarts the running count at the section head.
        if let Some(start) = section.page_number_start {
            next = start.max(1);
        }
        // `evenPage`/`oddPage`: land the section's first page on the right parity by
        // inserting a blank. Not applied to the very first section (there is nothing
        // before page 1 to break from).
        if index > 0
            && let Some(remainder) = parity_remainder(section.break_type)
            && next % 2 != remainder
        {
            pages.push(blank_page(next, &section.config));
            next += 1;
        }
        for mut page in section_pages {
            page.number = next;
            next += 1;
            pages.push(page);
        }
    }
    PaginatedLayout { pages }
}

/// The page-number parity an `evenPage`/`oddPage` section requires (`0` = even,
/// `1` = odd), or `None` for break types that impose no parity.
fn parity_remainder(break_type: SectionType) -> Option<u32> {
    match break_type {
        SectionType::EvenPage => Some(0),
        SectionType::OddPage => Some(1),
        SectionType::NextPage | SectionType::Continuous | SectionType::NextColumn => None,
    }
}

/// Builds an empty page (an `evenPage`/`oddPage` parity blank) under a section's
/// geometry. It carries no flow content; its model/flow anchors are the section's
/// own node so provenance stays well-defined, and its empty `placed` marks it as a
/// stitch artifact (so the incremental re-split skips it).
fn blank_page(number: u32, config: &PageConfig) -> Page {
    let anchor = ModelPos::new(config.section.node_id(), 0);
    Page {
        number,
        section: config.section,
        content_area: config.content_area(),
        placed: Vec::new(),
        header: Vec::new(),
        footer: Vec::new(),
        footnotes: Vec::new(),
        start: anchor,
        end: anchor,
        flow: FlowSpan {
            start: FlowPos::at(0),
            end: FlowPos::at(0),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_model::NodeId;

    use crate::block::{BoxMetrics, BreakControl, ParagraphDecor};
    use crate::model::{ModelPos, ModelRange};
    use crate::text::{Line, LineBreak, LineLayout};
    use crate::units::{Size, Twip};

    /// A portrait US-Letter section geometry (1152×12960-twip content area) with a
    /// caller-chosen section id.
    fn portrait(section: u64) -> PageConfig {
        PageConfig {
            section: SectionId::new(NodeId::from_parts(section, 1).unwrap()),
            page_size: Size::new(Twip(12_240), Twip(15_840)),
            margin_top: Twip(1_440),
            margin_bottom: Twip(1_440),
            margin_start: Twip(1_440),
            margin_end: Twip(1_440),
            header_height: Twip::ZERO,
            footer_height: Twip::ZERO,
        }
    }

    /// A landscape US-Letter section geometry (width and height swapped) — a
    /// different content area from [`portrait`], so cross-geometry reuse is a bug.
    fn landscape(section: u64) -> PageConfig {
        PageConfig {
            page_size: Size::new(Twip(15_840), Twip(12_240)),
            ..portrait(section)
        }
    }

    /// A one-line paragraph fragment of the given height and node id.
    fn para(id: u64, height: Twip) -> BlockFragment {
        let node = NodeId::from_parts(id, 1).unwrap();
        let line = Line {
            runs: Vec::new(),
            ascent: height,
            descent: Twip::ZERO,
            height,
            range: ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
            line_break: LineBreak::ParagraphEnd,
            page_break_after: false,
            bars: Vec::new(),
            images: Vec::new(),
            fields: Vec::new(),
        };
        BlockFragment::Paragraph {
            id: node,
            lines: LineLayout { lines: vec![line] },
            box_metrics: BoxMetrics::default(),
            break_control: BreakControl::default(),
            decor: ParagraphDecor::default(),
        }
    }

    /// A galley of `n` paragraphs of `height`, node ids offset by `base`.
    fn galley(base: u64, n: usize, height: Twip) -> Vec<BlockFragment> {
        (0..n as u64).map(|i| para(base + i, height)).collect()
    }

    fn section_with(config: PageConfig, fragments: Vec<BlockFragment>, ty: SectionType) -> Section {
        Section {
            config,
            fragments,
            break_type: ty,
            page_number_start: None,
        }
    }

    /// The golden invariant for multi-section: `repaginate_sections` is
    /// field-for-field identical to a full `paginate_sections` of the new sections,
    /// and every page is accounted for across the per-section stats plus any parity
    /// blanks. Returns the per-section stats.
    fn golden(prev: &[Section], new: &[Section]) -> Vec<RepaginateStats> {
        let prev_layout = paginate_sections(prev);
        let (inc, stats) = repaginate_sections_with_stats(&prev_layout, prev, new);
        let full = paginate_sections(new);
        assert_eq!(
            inc, full,
            "incremental multi-section pagination must equal a full paginate_sections"
        );
        // The reflowed/reused page tallies (plus parity blanks) cover every page.
        let accounted: usize = stats
            .iter()
            .map(|s| s.reused_prefix + s.reflowed + s.reused_tail)
            .sum();
        let blanks = full.pages.iter().filter(|p| p.placed.is_empty()).count();
        assert_eq!(
            accounted + blanks,
            full.page_count(),
            "every page is a reused/reflowed section page or a parity blank"
        );
        stats
    }

    #[test]
    fn each_section_paginates_under_its_own_geometry() {
        // Section 1 portrait (one page), section 2 landscape (one page). Section 2
        // starts on a fresh page (default nextPage) under its own content area.
        let s1 = section_with(
            portrait(100),
            galley(1, 2, Twip(400)),
            SectionType::NextPage,
        );
        let s2 = section_with(
            landscape(200),
            galley(50, 2, Twip(400)),
            SectionType::NextPage,
        );
        let layout = paginate_sections(&[s1.clone(), s2.clone()]);

        assert_eq!(layout.page_count(), 2, "one page per section");
        assert_eq!(layout.pages[0].section, s1.config.section);
        assert_eq!(layout.pages[1].section, s2.config.section);
        assert_eq!(layout.pages[0].number, 1);
        assert_eq!(layout.pages[1].number, 2);
        assert_eq!(
            layout.pages[0].content_area,
            s1.config.content_area(),
            "section 1 uses the portrait content area"
        );
        assert_eq!(
            layout.pages[1].content_area,
            s2.config.content_area(),
            "section 2 uses the landscape content area"
        );
        assert_ne!(
            layout.pages[0].content_area, layout.pages[1].content_area,
            "the two sections have genuinely different geometry"
        );
    }

    #[test]
    fn a_landscape_section_of_many_pages_keeps_its_geometry_throughout() {
        // Section 2 is tall enough to span several landscape pages; every one must
        // carry the landscape content area, none the portrait one.
        let s1 = section_with(
            portrait(100),
            galley(1, 1, Twip(400)),
            SectionType::NextPage,
        );
        // Landscape content height is 12240 - 2880 = 9360; 40×400 = 16000 -> >1 page.
        let s2 = section_with(
            landscape(200),
            galley(50, 40, Twip(400)),
            SectionType::NextPage,
        );
        let layout = paginate_sections(&[s1.clone(), s2.clone()]);
        assert!(
            layout.page_count() >= 3,
            "the landscape section spans pages"
        );
        for page in &layout.pages[1..] {
            assert_eq!(page.section, s2.config.section);
            assert_eq!(page.content_area, s2.config.content_area());
        }
        // Page numbers are globally contiguous across the boundary.
        let numbers: Vec<u32> = layout.pages.iter().map(|p| p.number).collect();
        assert_eq!(
            numbers,
            (1..=layout.page_count() as u32).collect::<Vec<_>>()
        );
    }

    #[test]
    fn even_page_section_inserts_a_blank_to_land_on_an_even_page() {
        // Section 1 is one page (page 1). An evenPage section 2 must begin on page 2
        // (already even) — no blank needed here.
        let s1 = section_with(
            portrait(100),
            galley(1, 1, Twip(400)),
            SectionType::NextPage,
        );
        let s2 = section_with(
            portrait(200),
            galley(50, 1, Twip(400)),
            SectionType::EvenPage,
        );
        let layout = paginate_sections(&[s1, s2]);
        assert_eq!(layout.page_count(), 2);
        assert_eq!(layout.pages[1].number, 2, "even section begins on page 2");
        assert!(!layout.pages[1].placed.is_empty(), "no blank was needed");

        // Now make section 1 two pages: the evenPage section would land on page 3
        // (odd), so a blank page 3 is inserted and the section begins on page 4.
        let s1_two = section_with(
            portrait(100),
            galley(1, 40, Twip(400)),
            SectionType::NextPage,
        );
        let s2b = section_with(
            portrait(200),
            galley(50, 1, Twip(400)),
            SectionType::EvenPage,
        );
        let layout = paginate_sections(&[s1_two, s2b]);
        let last = layout.pages.last().unwrap();
        assert_eq!(last.number % 2, 0, "the section lands on an even page");
        let blank = layout
            .pages
            .iter()
            .find(|p| p.placed.is_empty())
            .expect("a parity blank was inserted");
        assert_eq!(blank.number % 2, 1, "the inserted blank took the odd page");
    }

    #[test]
    fn odd_page_section_inserts_a_blank_to_land_on_an_odd_page() {
        // Section 1 is one page (page 1); an oddPage section 2 would fall on page 2
        // (even), so a blank page 2 is inserted and the section begins on page 3.
        let s1 = section_with(
            portrait(100),
            galley(1, 1, Twip(400)),
            SectionType::NextPage,
        );
        let s2 = section_with(
            portrait(200),
            galley(50, 1, Twip(400)),
            SectionType::OddPage,
        );
        let layout = paginate_sections(&[s1, s2]);
        assert_eq!(layout.page_count(), 3);
        assert!(
            layout.pages[1].placed.is_empty(),
            "page 2 is the parity blank"
        );
        assert_eq!(layout.pages[2].number, 3, "the section lands on odd page 3");
        assert!(!layout.pages[2].placed.is_empty());
    }

    #[test]
    fn pgnumtype_restart_renumbers_the_section() {
        let s1 = section_with(
            portrait(100),
            galley(1, 1, Twip(400)),
            SectionType::NextPage,
        );
        let mut s2 = section_with(
            portrait(200),
            galley(50, 1, Twip(400)),
            SectionType::NextPage,
        );
        s2.page_number_start = Some(10);
        let layout = paginate_sections(&[s1, s2]);
        assert_eq!(layout.pages[0].number, 1);
        assert_eq!(layout.pages[1].number, 10, "section 2 restarts at page 10");
    }

    #[test]
    fn single_section_paginate_sections_matches_paginate() {
        // The one-section case must place content identically to `paginate` (only
        // the page-number stamping is re-applied, to the same values).
        let config = portrait(100);
        let fragments = galley(1, 40, Twip(400));
        let sectioned = paginate_sections(&[Section::new(config, fragments.clone())]);
        let plain = crate::paginate::paginate(&fragments, &config);
        assert_eq!(
            sectioned, plain,
            "one-section stitch equals a bare paginate"
        );
    }

    // --- Multi-section incremental goldens -------------------------------------

    #[test]
    fn repaginate_sections_equals_full_for_an_identity_edit() {
        let prev = vec![
            section_with(
                portrait(100),
                galley(1, 30, Twip(400)),
                SectionType::NextPage,
            ),
            section_with(
                landscape(200),
                galley(50, 30, Twip(400)),
                SectionType::NextPage,
            ),
        ];
        let new = prev.clone();
        golden(&prev, &new);
    }

    #[test]
    fn repaginate_sections_reuses_section_two_when_section_one_changes_page_count() {
        // Grow section 1 so its page count changes; section 2's galley is untouched,
        // so it must be reused wholesale (its internal layout unchanged) even though
        // its global page numbers shift.
        let prev = vec![
            section_with(
                portrait(100),
                galley(1, 30, Twip(400)),
                SectionType::NextPage,
            ),
            section_with(
                landscape(200),
                galley(50, 30, Twip(400)),
                SectionType::NextPage,
            ),
        ];
        let mut new = prev.clone();
        new[0].fragments.push(para(999, Twip(12_000))); // pushes section 1 to a new page
        let stats = golden(&prev, &new);
        assert!(
            stats[1].reused_prefix + stats[1].reused_tail > 0 && stats[1].reflowed <= 1,
            "section 2 is reused, not re-flowed: {stats:?}"
        );
    }

    #[test]
    fn repaginate_sections_reuses_section_one_when_only_section_two_changes() {
        // Section 1 spans several pages; an edit isolated to section 2 must leave
        // section 1 almost entirely reused (its unchanged galley re-paginates with
        // its own prefix reuse — no work proportional to section 1's length).
        let prev = vec![
            section_with(
                portrait(100),
                galley(1, 90, Twip(400)),
                SectionType::NextPage,
            ),
            section_with(
                landscape(200),
                galley(200, 30, Twip(400)),
                SectionType::NextPage,
            ),
        ];
        let mut new = prev.clone();
        new[1].fragments[10] = para(210, Twip(1_600)); // grow one block in section 2
        let stats = golden(&prev, &new);
        assert!(
            stats[0].reused_prefix > 0 && stats[0].reflowed <= 1,
            "section 1 is reused (only its final page re-flows on an identity pass): {stats:?}"
        );
    }

    #[test]
    fn repaginate_sections_equals_full_across_an_even_page_blank() {
        // A parity blank sits between the sections; an edit in section 1 that shifts
        // its page count must still reproduce a full paginate (blank re-derived).
        let prev = vec![
            section_with(
                portrait(100),
                galley(1, 30, Twip(400)),
                SectionType::NextPage,
            ),
            section_with(
                portrait(200),
                galley(50, 30, Twip(400)),
                SectionType::EvenPage,
            ),
        ];
        let mut new = prev.clone();
        new[0].fragments.push(para(999, Twip(400)));
        golden(&prev, &new);
    }

    #[test]
    fn repaginate_sections_equals_full_when_a_section_is_edited_and_reordered_content() {
        // Insert and remove fragments in different sections at once.
        let prev = vec![
            section_with(
                portrait(100),
                galley(1, 30, Twip(400)),
                SectionType::NextPage,
            ),
            section_with(
                landscape(200),
                galley(50, 30, Twip(400)),
                SectionType::NextPage,
            ),
            section_with(
                portrait(300),
                galley(80, 20, Twip(400)),
                SectionType::OddPage,
            ),
        ];
        let mut new = prev.clone();
        new[0].fragments.insert(5, para(1000, Twip(500)));
        new[2].fragments.remove(3);
        golden(&prev, &new);
    }
}
