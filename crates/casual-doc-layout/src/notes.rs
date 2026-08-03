//! Footnote placement for the bounded section/column pagination slice.
//!
//! The body paginator owns page breaking. This module wraps it with a small
//! fixed-point loop: paginate with the current page reservations, inspect the
//! placed body lines for footnote markers, compute the bottom-band heights, and
//! repeat until the reservations stabilize or the hard cap is reached.

use std::collections::{BTreeMap, VecDeque};

use casual_doc_model::v1::{Document, NoteId, NoteKind, SectionId};

use crate::block::{BlockFragment, CellFragment};
use crate::columns::{SectionRun, paginate_columns_with_reservations};
use crate::flow::build_galley_for_blocks;
use crate::page::{Page, PaginatedLayout, PlacedFragment};
use crate::paginate::PageConfig;
use crate::text::{LineLayout, LineShaper, NoteMarker};
use crate::units::{Point, Rect, Size, Twip};

/// Maximum fixed-point passes for page-local footnote reservation.
const MAX_RESERVATION_PASSES: usize = 6;

/// Keep at least a small body area available when a note is oversized. The note
/// body may overflow its band, but the convergence loop must remain bounded.
const MIN_BODY_HEIGHT_TWIPS: i32 = 720;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct NoteFlowKey {
    section: SectionId,
    width: Twip,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct NoteBandKey {
    section: SectionId,
    x: Twip,
    width: Twip,
}

#[derive(Clone, Copy, Debug)]
struct FootnoteRef {
    note: NoteId,
    band: NoteBandKey,
}

/// Whether a single-column section run has body footnote references.
#[must_use]
pub(crate) fn run_has_body_footnotes(run: &SectionRun) -> bool {
    run.galley.iter().any(fragment_has_footnote)
}

/// Paginates section runs with page-local footnote bands. Each section's footnote
/// bodies are flowed at that section's full content width, while the page
/// reservation loop remains global and bounded across section and column breaks.
#[must_use]
pub(crate) fn paginate_section_footnotes(
    document: &Document,
    shaper: &dyn LineShaper,
    runs: &[SectionRun],
) -> PaginatedLayout {
    let notes = build_section_footnote_galleys(document, shaper, runs);
    if notes.is_empty() {
        return paginate_columns_with_reservations(runs, &[]);
    }

    let mut reservations = Vec::new();
    for _ in 0..MAX_RESERVATION_PASSES {
        let mut layout = paginate_columns_with_reservations(runs, &reservations);
        let next = page_reservations(&layout, &notes, runs);
        let merged = merge_reservations(&reservations, &next);
        if merged == reservations {
            place_footnotes(&mut layout, &notes, &next);
            return layout;
        }
        reservations = merged;
    }

    let layout = paginate_columns_with_reservations(runs, &reservations);
    let final_reservations =
        merge_reservations(&reservations, &page_reservations(&layout, &notes, runs));
    let mut layout = paginate_columns_with_reservations(runs, &final_reservations);
    let placed_reservations = page_reservations(&layout, &notes, runs);
    place_footnotes(&mut layout, &notes, &placed_reservations);
    layout
}

fn merge_reservations(current: &[Twip], next: &[Twip]) -> Vec<Twip> {
    let len = current.len().max(next.len());
    (0..len)
        .map(|i| {
            current
                .get(i)
                .copied()
                .unwrap_or(Twip::ZERO)
                .max(next.get(i).copied().unwrap_or(Twip::ZERO))
        })
        .collect()
}

fn build_footnote_galleys(
    document: &Document,
    shaper: &dyn LineShaper,
    width: Twip,
) -> BTreeMap<NoteId, Vec<BlockFragment>> {
    document
        .definitions()
        .footnotes
        .iter()
        .map(|(id, note)| {
            (
                *id,
                build_galley_for_blocks(document, shaper, &note.blocks, width),
            )
        })
        .collect()
}

fn build_section_footnote_galleys(
    document: &Document,
    shaper: &dyn LineShaper,
    runs: &[SectionRun],
) -> BTreeMap<NoteFlowKey, BTreeMap<NoteId, Vec<BlockFragment>>> {
    let mut out = BTreeMap::new();
    for run in runs {
        for width in run.layout.flow_widths() {
            out.entry(NoteFlowKey {
                section: run.config.section,
                width,
            })
            .or_insert_with(|| build_footnote_galleys(document, shaper, width));
        }
    }
    out
}

fn page_reservations(
    layout: &PaginatedLayout,
    notes: &BTreeMap<NoteFlowKey, BTreeMap<NoteId, Vec<BlockFragment>>>,
    runs: &[SectionRun],
) -> Vec<Twip> {
    let mut queues: BTreeMap<NoteBandKey, VecDeque<BlockFragment>> = BTreeMap::new();
    let page_count = layout.pages.len();
    layout
        .pages
        .iter()
        .enumerate()
        .map(|(index, page)| {
            let cap = runs
                .iter()
                .find(|run| run.config.section == page.section)
                .map_or_else(
                    || footnote_cap_for_area(page.content_area),
                    |run| footnote_cap(&run.config),
                );
            enqueue_page_footnotes(page_footnote_refs(page), notes, &mut queues);
            consume_queues_for_reservation(&mut queues, cap, index + 1 == page_count)
                .into_iter()
                .max()
                .unwrap_or(Twip::ZERO)
                .min(cap)
        })
        .collect()
}

fn enqueue_page_footnotes(
    refs: Vec<FootnoteRef>,
    notes: &BTreeMap<NoteFlowKey, BTreeMap<NoteId, Vec<BlockFragment>>>,
    queues: &mut BTreeMap<NoteBandKey, VecDeque<BlockFragment>>,
) {
    for reference in refs {
        let Some(galley) = notes
            .get(&NoteFlowKey {
                section: reference.band.section,
                width: reference.band.width,
            })
            .and_then(|section| section.get(&reference.note))
        else {
            continue;
        };
        queues
            .entry(reference.band)
            .or_default()
            .extend(galley.iter().cloned());
    }
}

fn consume_queues_for_reservation(
    queues: &mut BTreeMap<NoteBandKey, VecDeque<BlockFragment>>,
    cap: Twip,
    is_last_page: bool,
) -> Vec<Twip> {
    queues
        .values_mut()
        .map(|queue| consume_queue_for_reservation(queue, cap, is_last_page))
        .collect()
}

fn consume_queue_for_reservation(
    queue: &mut VecDeque<BlockFragment>,
    cap: Twip,
    is_last_page: bool,
) -> Twip {
    if queue.is_empty() || cap <= Twip::ZERO {
        return Twip::ZERO;
    }
    if is_last_page {
        let total = queue
            .iter()
            .map(BlockFragment::height)
            .fold(Twip::ZERO, |a, h| a + h);
        queue.clear();
        return total.min(cap);
    }

    let mut used = Twip::ZERO;
    while let Some(fragment) = queue.front() {
        let height = fragment.height();
        if used + height <= cap {
            used = used + height;
            queue.pop_front();
        } else if used == Twip::ZERO {
            queue.pop_front();
            return cap;
        } else {
            break;
        }
    }
    used
}

fn footnote_cap(config: &PageConfig) -> Twip {
    footnote_cap_for_area(config.content_area())
}

fn footnote_cap_for_area(content: Rect) -> Twip {
    let body = content.size.height.raw();
    Twip((body - MIN_BODY_HEIGHT_TWIPS).max(0))
}

fn place_footnotes(
    layout: &mut PaginatedLayout,
    notes: &BTreeMap<NoteFlowKey, BTreeMap<NoteId, Vec<BlockFragment>>>,
    reservations: &[Twip],
) {
    let mut queues: BTreeMap<NoteBandKey, VecDeque<BlockFragment>> = BTreeMap::new();
    let page_count = layout.pages.len();
    for (index, page) in layout.pages.iter_mut().enumerate() {
        let band_height = reservations.get(index).copied().unwrap_or(Twip::ZERO);
        let refs = page_footnote_refs(page);
        enqueue_page_footnotes(refs, notes, &mut queues);
        if band_height <= Twip::ZERO {
            continue;
        }
        page.footnotes = stack_note_galleys(
            &mut queues,
            page.content_area.bottom(),
            band_height,
            index + 1 == page_count,
        );
    }
}

fn stack_note_galleys(
    queues: &mut BTreeMap<NoteBandKey, VecDeque<BlockFragment>>,
    band_top: Twip,
    band_height: Twip,
    is_last_page: bool,
) -> Vec<PlacedFragment> {
    let mut placed = Vec::new();
    for (band_key, queue) in queues.iter_mut() {
        let mut y = band_top;
        let band = Rect::new(
            Point::new(band_key.x, band_top),
            Size::new(band_key.width, band_height),
        );
        while let Some(fragment) = queue.front() {
            let height = fragment.height();
            let fits = y + height <= band_top + band_height;
            let first_in_band = y == band_top;
            if !fits && !is_last_page && !first_in_band {
                break;
            }
            let fragment = queue.pop_front().expect("front existed");
            placed.push(PlacedFragment {
                fragment,
                rect: Rect::new(
                    Point::new(band.origin.x, y),
                    Size::new(band.size.width, height),
                ),
                section: Some(band_key.section),
            });
            y = y + height;
        }
    }
    placed
}

fn page_footnote_refs(page: &Page) -> Vec<FootnoteRef> {
    let mut refs = Vec::new();
    for placed in &page.placed {
        let section = placed.section.unwrap_or(page.section);
        let band = NoteBandKey {
            section,
            x: placed.rect.origin.x,
            width: placed.rect.size.width,
        };
        collect_fragment_notes(&placed.fragment, band, &mut refs);
    }
    refs
}

fn fragment_has_footnote(fragment: &BlockFragment) -> bool {
    let mut found = false;
    collect_fragment_note_markers(fragment, &mut |marker| {
        if marker.kind == NoteKind::Footnote {
            found = true;
        }
    });
    found
}

fn collect_fragment_notes(
    fragment: &BlockFragment,
    band: NoteBandKey,
    refs: &mut Vec<FootnoteRef>,
) {
    collect_fragment_note_markers(fragment, &mut |marker| {
        if marker.kind == NoteKind::Footnote {
            refs.push(FootnoteRef {
                note: marker.note,
                band,
            });
        }
    });
}

fn collect_fragment_note_markers(fragment: &BlockFragment, f: &mut impl FnMut(NoteMarker)) {
    match fragment {
        BlockFragment::Paragraph { lines, .. } => collect_line_notes(lines, f),
        BlockFragment::TableRow { cells, .. } => {
            for cell in cells {
                collect_cell_notes(cell, f);
            }
        }
    }
}

fn collect_cell_notes(cell: &CellFragment, f: &mut impl FnMut(NoteMarker)) {
    for block in &cell.blocks {
        collect_fragment_note_markers(block, f);
    }
}

fn collect_line_notes(lines: &LineLayout, f: &mut impl FnMut(NoteMarker)) {
    for line in &lines.lines {
        for marker in &line.notes {
            f(*marker);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::{
        BlockNode, Definitions, DocGrid, GridColumn, HeaderFooter, HeaderFooterId,
        HeaderFooterKind, HeaderFooterRef, InlineNode, Note, NoteProperties, NoteReference,
        PageBorders, PageMargins, PageNumbering, PageSize, PaperSource, Paragraph,
        ParagraphProperties, Run, RunProperties, SectionBoundary, SectionColumns, SectionId,
        SectionType, Table, TableCell, TableCellProperties, TableProperties, TableRow,
        TableRowProperties,
    };

    use crate::document_layout::{document_page_config, paginate_document};
    use crate::shape::ParleyShaper;

    fn node(id: u64) -> NodeId {
        NodeId::from_parts(id, 1).unwrap()
    }

    fn paragraph(id: u64, text: &str) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id: node(id),
            properties: ParagraphProperties::default(),
            inlines: vec![InlineNode::Run(Run {
                id: node(id + 10_000),
                properties: RunProperties::default(),
                text: text.to_string(),
            })],
        })
    }

    fn note_ref_paragraph(id: u64, note: NoteId) -> BlockNode {
        typed_note_ref_paragraph(id, note, NoteKind::Footnote)
    }

    fn endnote_ref_paragraph(id: u64, note: NoteId) -> BlockNode {
        typed_note_ref_paragraph(id, note, NoteKind::Endnote)
    }

    fn typed_note_ref_paragraph(id: u64, note: NoteId, kind: NoteKind) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id: node(id),
            properties: ParagraphProperties::default(),
            inlines: vec![
                InlineNode::Run(Run {
                    id: node(id + 10_000),
                    properties: RunProperties::default(),
                    text: "reference".to_string(),
                }),
                InlineNode::NoteReference(NoteReference {
                    id: node(id + 20_000),
                    kind,
                    note,
                }),
            ],
        })
    }

    fn long_note_ref_paragraph(id: u64, note: NoteId, leading_words: usize) -> BlockNode {
        let text = (0..leading_words)
            .map(|_| "wrapped")
            .collect::<Vec<_>>()
            .join(" ");
        BlockNode::Paragraph(Paragraph {
            id: node(id),
            properties: ParagraphProperties::default(),
            inlines: vec![
                InlineNode::Run(Run {
                    id: node(id + 10_000),
                    properties: RunProperties::default(),
                    text,
                }),
                InlineNode::NoteReference(NoteReference {
                    id: node(id + 20_000),
                    kind: NoteKind::Footnote,
                    note,
                }),
                InlineNode::Run(Run {
                    id: node(id + 30_000),
                    properties: RunProperties::default(),
                    text: " tail".to_string(),
                }),
            ],
        })
    }

    fn document(body: Vec<BlockNode>, definitions: Definitions) -> Document {
        Document::new(node(1), body, definitions).unwrap()
    }

    fn section(id: u64) -> SectionBoundary {
        SectionBoundary {
            id: SectionId::new(node(id)),
            page_size: PageSize {
                width_twips: 12_240,
                height_twips: 15_840,
            },
            page_margins: PageMargins {
                top_twips: 1_440,
                bottom_twips: 1_440,
                start_twips: 1_440,
                end_twips: 1_440,
                header_twips: None,
                footer_twips: None,
                gutter_twips: None,
            },
            columns: SectionColumns {
                count: 1,
                space_twips: None,
                separator: None,
                equal_width: None,
                columns: Vec::new(),
            },
            headers: Vec::new(),
            footers: Vec::new(),
            section_type: None,
            title_page: None,
            vertical_alignment: None,
            page_numbering: PageNumbering::default(),
            doc_grid: DocGrid::default(),
            orientation: None,
            paper_source: PaperSource::default(),
            page_borders: PageBorders::default(),
            line_numbering: Default::default(),
            footnote_props: NoteProperties::default(),
            endnote_props: NoteProperties::default(),
            text_direction: None,
            bidi: false,
        }
    }

    fn section_with_width(id: u64, page_width_twips: i32, margin_twips: i32) -> SectionBoundary {
        let mut section = section(id);
        section.page_size.width_twips = page_width_twips;
        section.page_margins.start_twips = margin_twips;
        section.page_margins.end_twips = margin_twips;
        section
    }

    fn section_with_columns(id: u64, count: u16) -> SectionBoundary {
        let mut section = section(id);
        section.columns.count = count;
        section
    }

    fn section_with_header(header: HeaderFooterId) -> SectionBoundary {
        let mut section = section(900);
        section.headers.push(HeaderFooterRef {
            kind: HeaderFooterKind::Default,
            reference: header,
        });
        section
    }

    fn table_with_split_cell_note(note: NoteId) -> BlockNode {
        let mut blocks: Vec<_> = (0..70)
            .map(|i| paragraph(1_000 + i, "table cell body line"))
            .collect();
        blocks.push(note_ref_paragraph(1_200, note));
        blocks.extend((0..8).map(|i| paragraph(1_300 + i, "table tail line")));
        BlockNode::Table(Table {
            id: node(950),
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
                    blocks,
                }],
            }],
        })
    }

    #[test]
    fn footnote_body_is_placed_on_the_reference_page() {
        let note = NoteId::new(node(501));
        let mut definitions = Definitions::default();
        definitions.footnotes.insert(
            note,
            Note {
                blocks: vec![paragraph(601, "footnote body")],
            },
        );
        let doc = document(vec![note_ref_paragraph(10, note)], definitions);
        let shaper = ParleyShaper::new();

        let layout = paginate_document(&doc, &shaper);

        assert_eq!(layout.pages.len(), 1);
        assert_eq!(layout.pages[0].footnotes.len(), 1);
        assert_eq!(
            layout.pages[0].footnotes[0].fragment.node_id(),
            node(601),
            "the referenced footnote definition is stacked into the page band"
        );
        assert!(
            layout.pages[0].footnotes[0].rect.origin.y >= layout.pages[0].content_area.bottom(),
            "footnotes sit below the reserved body area"
        );
    }

    #[test]
    fn footnote_reservation_pushes_body_content_to_the_next_page() {
        let note = NoteId::new(node(701));
        let mut definitions = Definitions::default();
        definitions.footnotes.insert(
            note,
            Note {
                blocks: (0..18)
                    .map(|i| paragraph(800 + i, "reserved footnote line"))
                    .collect(),
            },
        );
        let mut body: Vec<_> = (0..42).map(|i| paragraph(100 + i, "body line")).collect();
        body.push(note_ref_paragraph(300, note));
        body.extend((0..8).map(|i| paragraph(400 + i, "tail line")));
        let doc = document(body, definitions);
        let shaper = ParleyShaper::new();

        let layout = paginate_document(&doc, &shaper);
        let default_content = document_page_config(&doc).content_area();

        assert!(
            layout.pages.len() >= 2,
            "the note reservation should force a page break instead of overlaying body text"
        );
        let note_page = layout
            .pages
            .iter()
            .find(|page| !page.footnotes.is_empty())
            .expect("expected a page-local footnote band");
        assert!(
            note_page.content_area.size.height < default_content.size.height,
            "the reference page body area is reduced by the footnote band"
        );
        assert!(
            !note_page.footnotes.is_empty(),
            "the note body is assigned to the page containing the reference"
        );
    }

    #[test]
    fn split_paragraph_reference_places_footnote_on_the_containing_page() {
        let note = NoteId::new(node(1_401));
        let mut definitions = Definitions::default();
        definitions.footnotes.insert(
            note,
            Note {
                blocks: vec![paragraph(1_402, "split paragraph footnote")],
            },
        );
        let doc = document(vec![long_note_ref_paragraph(1_410, note, 900)], definitions);
        let shaper = ParleyShaper::new();

        let layout = paginate_document(&doc, &shaper);

        assert!(
            layout.pages.len() >= 2,
            "the long paragraph must split across pages"
        );
        assert!(
            layout.pages[0].footnotes.is_empty(),
            "the first split chunk precedes the note reference"
        );
        let note_page = layout
            .pages
            .iter()
            .position(|page| !page.footnotes.is_empty())
            .expect("expected a footnote on the split paragraph tail page");
        assert!(
            note_page > 0,
            "the footnote belongs to the later page containing the reference line"
        );
    }

    #[test]
    fn split_table_cell_reference_places_footnote_on_the_containing_page() {
        let note = NoteId::new(node(1_501));
        let mut definitions = Definitions::default();
        definitions.footnotes.insert(
            note,
            Note {
                blocks: vec![paragraph(1_502, "split table footnote")],
            },
        );
        let doc = document(vec![table_with_split_cell_note(note)], definitions);
        let shaper = ParleyShaper::new();

        let layout = paginate_document(&doc, &shaper);

        assert!(
            layout.pages.len() >= 2,
            "the tall table row must split across pages"
        );
        assert!(
            layout.pages[0].footnotes.is_empty(),
            "the first row chunk precedes the table-cell note reference"
        );
        let note_page = layout
            .pages
            .iter()
            .position(|page| !page.footnotes.is_empty())
            .expect("expected a footnote on the row chunk containing the reference");
        assert!(
            note_page > 0,
            "the footnote follows the page containing the split table-cell reference"
        );
    }

    #[test]
    fn later_section_footnote_uses_that_sections_body_width() {
        let note = NoteId::new(node(1_551));
        let first_section = SectionId::new(node(1_552));
        let second_section = SectionId::new(node(1_553));
        let mut definitions = Definitions::default();
        definitions.footnotes.insert(
            note,
            Note {
                blocks: vec![paragraph(1_554, "later section footnote body")],
            },
        );
        definitions.sections.push(section(1_552));
        definitions
            .sections
            .push(section_with_width(1_553, 8_000, 1_000));
        let mut first = paragraph(1_555, "first section");
        if let BlockNode::Paragraph(paragraph) = &mut first {
            paragraph.properties.section_break = Some(first_section);
        }
        let doc = document(vec![first, note_ref_paragraph(1_556, note)], definitions);
        let shaper = ParleyShaper::new();

        let layout = paginate_document(&doc, &shaper);
        let note_page = layout
            .pages
            .iter()
            .find(|page| !page.footnotes.is_empty())
            .expect("expected a later-section footnote band");

        assert_eq!(
            note_page.section, second_section,
            "the footnote belongs to the page carrying the later section"
        );
        assert_eq!(
            note_page.footnotes[0].rect.size.width, note_page.content_area.size.width,
            "the footnote body is flowed and placed at the later section content width"
        );
        assert_eq!(
            note_page.footnotes[0].fragment.node_id(),
            node(1_554),
            "the later-section note body is visible"
        );
    }

    #[test]
    fn continuous_later_section_footnote_reserves_on_shared_page() {
        let note = NoteId::new(node(1_571));
        let first_section = SectionId::new(node(1_572));
        let second_section = SectionId::new(node(1_573));
        let mut definitions = Definitions::default();
        definitions.footnotes.insert(
            note,
            Note {
                blocks: (0..8)
                    .map(|i| paragraph(1_580 + i, "continuous footnote body"))
                    .collect(),
            },
        );
        definitions.sections.push(section(1_572));
        let mut second = section(1_573);
        second.section_type = Some(SectionType::Continuous);
        definitions.sections.push(second);
        let mut first = paragraph(1_574, "first section");
        if let BlockNode::Paragraph(paragraph) = &mut first {
            paragraph.properties.section_break = Some(first_section);
        }
        let doc = document(
            vec![
                first,
                paragraph(1_575, "continuous lead"),
                note_ref_paragraph(1_576, note),
            ],
            definitions,
        );
        let shaper = ParleyShaper::new();

        let layout = paginate_document(&doc, &shaper);

        assert_eq!(
            layout.pages.len(),
            1,
            "the continuous section should share the first page"
        );
        assert_eq!(layout.pages[0].section, second_section);
        assert!(
            !layout.pages[0].footnotes.is_empty(),
            "a footnote in the continuous section reserves and places on the shared page"
        );
        assert!(
            layout.pages[0]
                .placed
                .iter()
                .all(|placed| placed.rect.bottom() <= layout.pages[0].content_area.bottom()),
            "body fragments remain above the reserved footnote band"
        );
        assert!(
            layout.pages[0]
                .footnotes
                .iter()
                .all(|placed| placed.rect.origin.y >= layout.pages[0].content_area.bottom()),
            "footnote fragments sit below the reserved shared-page body area"
        );
    }

    #[test]
    fn continuous_shared_page_uses_reference_fragment_section_for_footnotes() {
        let note = NoteId::new(node(1_611));
        let first_section = SectionId::new(node(1_612));
        let second_section = SectionId::new(node(1_613));
        let mut definitions = Definitions::default();
        definitions.footnotes.insert(
            note,
            Note {
                blocks: vec![paragraph(1_614, "early continuous footnote body")],
            },
        );
        definitions
            .sections
            .push(section_with_width(1_612, 12_240, 1_440));
        let mut second = section_with_width(1_613, 8_000, 1_000);
        second.section_type = Some(SectionType::Continuous);
        definitions.sections.push(second);
        let mut first = note_ref_paragraph(1_615, note);
        if let BlockNode::Paragraph(paragraph) = &mut first {
            paragraph.properties.section_break = Some(first_section);
        }
        let doc = document(
            vec![first, paragraph(1_616, "continuous tail")],
            definitions,
        );
        let shaper = ParleyShaper::new();

        let layout = paginate_document(&doc, &shaper);

        assert_eq!(
            layout.pages.len(),
            1,
            "the continuous section should share the first page"
        );
        assert_eq!(
            layout.pages[0].section, second_section,
            "the page-level section remains the later continuous section"
        );
        assert_eq!(
            layout.pages[0].placed[0].section,
            Some(first_section),
            "the body fragment retains its source section"
        );
        assert_eq!(
            layout.pages[0].footnotes[0].section,
            Some(first_section),
            "the footnote body uses the reference fragment's source section"
        );
        assert_eq!(
            layout.pages[0].footnotes[0].rect.size.width,
            Twip(9_360),
            "the earlier-section note is placed at the earlier section content width"
        );
    }

    #[test]
    fn two_column_footnote_uses_the_reference_column_band() {
        let note = NoteId::new(node(1_631));
        let mut definitions = Definitions::default();
        definitions.footnotes.insert(
            note,
            Note {
                blocks: (0..10)
                    .map(|i| paragraph(1_640 + i, "two column footnote body"))
                    .collect(),
            },
        );
        definitions.sections.push(section_with_columns(1_632, 2));
        let mut body: Vec<_> = (0..26)
            .map(|i| paragraph(1_660 + i, "two column body line"))
            .collect();
        body.push(note_ref_paragraph(1_690, note));
        body.extend((0..10).map(|i| paragraph(1_700 + i, "two column tail line")));
        let doc = document(body, definitions);
        let shaper = ParleyShaper::new();

        let layout = paginate_document(&doc, &shaper);
        let default_content = document_page_config(&doc).content_area();
        let note_page = layout
            .pages
            .iter()
            .find(|page| !page.footnotes.is_empty())
            .expect("expected a multi-column footnote band");

        assert!(
            note_page.content_area.size.height < default_content.size.height,
            "the multi-column reference page body area is reduced by the note band"
        );
        let reference = note_page
            .placed
            .iter()
            .find(|placed| placed.fragment.node_id() == node(1_690))
            .expect("expected placed footnote reference paragraph");
        assert_eq!(
            note_page.footnotes[0].rect.origin.x, reference.rect.origin.x,
            "multi-column footnote bodies start under the reference column"
        );
        assert_eq!(
            note_page.footnotes[0].rect.size.width, reference.rect.size.width,
            "multi-column footnote bodies are flowed and placed at the reference column width"
        );
        assert!(
            note_page
                .placed
                .iter()
                .all(|placed| placed.rect.bottom() <= note_page.content_area.bottom()),
            "body fragments remain above the multi-column footnote band"
        );
        assert!(
            note_page
                .footnotes
                .iter()
                .all(|placed| placed.rect.origin.y >= note_page.content_area.bottom()),
            "footnote fragments sit below the reserved multi-column body area"
        );
    }

    #[test]
    fn long_footnote_continues_onto_the_next_body_page() {
        let note = NoteId::new(node(1_721));
        let note_blocks = 90;
        let mut definitions = Definitions::default();
        definitions.footnotes.insert(
            note,
            Note {
                blocks: (0..note_blocks)
                    .map(|i| paragraph(1_730 + i, "continued footnote body"))
                    .collect(),
            },
        );
        definitions.sections.push(section(1_722));
        let mut body = vec![note_ref_paragraph(1_723, note)];
        body.extend((0..130).map(|i| paragraph(1_900 + i, "body after long footnote")));
        let doc = document(body, definitions);
        let shaper = ParleyShaper::new();

        let layout = paginate_document(&doc, &shaper);
        let first_note_page = layout
            .pages
            .iter()
            .position(|page| !page.footnotes.is_empty())
            .expect("expected the referenced footnote page");
        let continuation_page = first_note_page + 1;

        assert!(
            !layout.pages[continuation_page].footnotes.is_empty(),
            "the note body should continue onto the next body page"
        );
        assert!(
            page_footnote_refs(&layout.pages[continuation_page]).is_empty(),
            "the continuation page should not need another body reference"
        );
        assert!(
            layout.pages[continuation_page]
                .footnotes
                .iter()
                .all(|placed| placed.rect.origin.y
                    >= layout.pages[continuation_page].content_area.bottom()),
            "continued footnote fragments sit below the reserved body area"
        );
        let placed_note_blocks = layout
            .pages
            .iter()
            .flat_map(|page| page.footnotes.iter())
            .filter(|placed| {
                let raw = placed.fragment.node_id().as_u128();
                raw >= node(1_730).as_u128() && raw < node(1_730 + note_blocks).as_u128()
            })
            .count();
        assert_eq!(
            placed_note_blocks, note_blocks as usize,
            "all multi-block footnote content remains visible across continuation pages"
        );
    }

    #[test]
    fn oversized_single_block_footnote_overflows_in_place() {
        let note = NoteId::new(node(2_021));
        let long_text = (0..3_000)
            .map(|_| "oversized")
            .collect::<Vec<_>>()
            .join(" ");
        let mut definitions = Definitions::default();
        definitions.footnotes.insert(
            note,
            Note {
                blocks: vec![paragraph(2_022, &long_text)],
            },
        );
        definitions.sections.push(section(2_023));
        let doc = document(vec![note_ref_paragraph(2_024, note)], definitions);
        let shaper = ParleyShaper::new();

        let layout = paginate_document(&doc, &shaper);
        let default_content = document_page_config(&doc).content_area();
        let band_height = default_content.size.height - layout.pages[0].content_area.size.height;

        assert_eq!(layout.pages[0].footnotes.len(), 1);
        assert_eq!(layout.pages[0].footnotes[0].fragment.node_id(), node(2_022));
        assert!(
            layout.pages[0].footnotes[0].rect.size.height > band_height,
            "an over-band single block is consumed and allowed to overflow the band"
        );
    }

    #[test]
    fn header_note_reference_is_visible_but_does_not_reserve_body_footnotes() {
        let note = NoteId::new(node(1_601));
        let header = HeaderFooterId::new(node(1_602));
        let mut definitions = Definitions::default();
        definitions.footnotes.insert(
            note,
            Note {
                blocks: vec![paragraph(1_603, "header footnote body")],
            },
        );
        definitions.headers.insert(
            header,
            HeaderFooter {
                blocks: vec![note_ref_paragraph(1_604, note)],
            },
        );
        definitions.sections.push(section_with_header(header));
        let doc = document(vec![paragraph(1_605, "body")], definitions);
        let shaper = ParleyShaper::new();

        let layout = paginate_document(&doc, &shaper);

        assert_eq!(layout.pages.len(), 1);
        assert!(
            layout.pages[0].footnotes.is_empty(),
            "header note markers are visible metadata but do not reserve body footnotes in this slice"
        );
        assert!(
            layout.pages[0].header.iter().any(|placed| {
                matches!(
                    &placed.fragment,
                    BlockFragment::Paragraph { lines, .. }
                        if lines.lines.iter().any(|line| !line.notes.is_empty())
                )
            }),
            "the header still exposes the visible note marker metadata"
        );
    }

    #[test]
    fn endnote_body_is_appended_as_ordinary_body_content() {
        let endnote = NoteId::new(node(1_701));
        let mut definitions = Definitions::default();
        definitions.endnotes.insert(
            endnote,
            Note {
                blocks: vec![paragraph(1_702, "endnote body")],
            },
        );
        let doc = document(
            vec![
                paragraph(1_703, "body"),
                endnote_ref_paragraph(1_704, endnote),
            ],
            definitions,
        );
        let shaper = ParleyShaper::new();

        let layout = paginate_document(&doc, &shaper);
        let page = layout.pages.last().expect("expected a page");

        assert!(
            layout.pages.iter().all(|page| page.footnotes.is_empty()),
            "endnotes use ordinary body pagination, not Page::footnotes"
        );
        assert!(
            page.placed
                .iter()
                .any(|placed| placed.fragment.node_id() == node(1_702)),
            "the referenced endnote body is appended after body content"
        );
    }

    #[test]
    fn endnote_bodies_append_once_in_first_reference_order() {
        let first = NoteId::new(node(1_801));
        let second = NoteId::new(node(1_802));
        let mut definitions = Definitions::default();
        definitions.endnotes.insert(
            first,
            Note {
                blocks: vec![paragraph(1_811, "first endnote body")],
            },
        );
        definitions.endnotes.insert(
            second,
            Note {
                blocks: vec![paragraph(1_812, "second endnote body")],
            },
        );
        let doc = document(
            vec![
                endnote_ref_paragraph(1_821, second),
                endnote_ref_paragraph(1_822, first),
                endnote_ref_paragraph(1_823, second),
            ],
            definitions,
        );
        let shaper = ParleyShaper::new();

        let layout = paginate_document(&doc, &shaper);
        let appended: Vec<_> = layout
            .pages
            .iter()
            .flat_map(|page| page.placed.iter())
            .map(|placed| placed.fragment.node_id())
            .filter(|id| *id == node(1_811) || *id == node(1_812))
            .collect();

        assert_eq!(
            appended,
            vec![node(1_812), node(1_811)],
            "endnote definitions append once in first-reference order"
        );
    }

    #[test]
    fn endnote_referenced_before_final_section_appends_after_final_body() {
        let endnote = NoteId::new(node(1_901));
        let first_section = SectionId::new(node(1_902));
        let mut definitions = Definitions::default();
        definitions.endnotes.insert(
            endnote,
            Note {
                blocks: vec![paragraph(1_903, "early-section endnote body")],
            },
        );
        definitions.sections.push(section(1_902));
        definitions.sections.push(section(1_904));
        let mut first = endnote_ref_paragraph(1_905, endnote);
        if let BlockNode::Paragraph(paragraph) = &mut first {
            paragraph.properties.section_break = Some(first_section);
        }
        let doc = document(
            vec![first, paragraph(1_906, "final section body")],
            definitions,
        );
        let shaper = ParleyShaper::new();

        let layout = paginate_document(&doc, &shaper);
        let placed: Vec<_> = layout
            .pages
            .iter()
            .flat_map(|page| page.placed.iter())
            .map(|placed| placed.fragment.node_id())
            .collect();
        let final_body = placed
            .iter()
            .position(|id| *id == node(1_906))
            .expect("expected final section body");
        let endnote_body = placed
            .iter()
            .position(|id| *id == node(1_903))
            .expect("expected appended endnote body");

        assert!(
            endnote_body > final_body,
            "endnotes referenced in earlier sections append after the final body section"
        );
    }
}
