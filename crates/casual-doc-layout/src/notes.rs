//! Footnote placement for the bounded single-section pagination slice.
//!
//! The body paginator owns page breaking. This module wraps it with a small
//! fixed-point loop: paginate with the current page reservations, inspect the
//! placed body lines for footnote markers, compute the bottom-band heights, and
//! repeat until the reservations stabilize or the hard cap is reached.

use std::collections::BTreeMap;

use casual_doc_model::v1::{Document, NoteId, NoteKind};

use crate::block::{BlockFragment, CellFragment};
use crate::columns::SectionRun;
use crate::flow::build_galley_for_blocks;
use crate::page::{Page, PaginatedLayout, PlacedFragment};
use crate::paginate::{PageConfig, paginate_with_footnote_reservations};
use crate::text::{LineLayout, LineShaper, NoteMarker};
use crate::units::{Point, Rect, Size, Twip};

/// Maximum fixed-point passes for page-local footnote reservation.
const MAX_RESERVATION_PASSES: usize = 6;

/// Keep at least a small body area available when a note is oversized. The note
/// body may overflow its band, but the convergence loop must remain bounded.
const MIN_BODY_HEIGHT_TWIPS: i32 = 720;

/// Whether a single-column section run has body footnote references.
#[must_use]
pub(crate) fn run_has_body_footnotes(run: &SectionRun) -> bool {
    run.galley.iter().any(fragment_has_footnote)
}

/// Paginates one single-column section run with page-local footnote bands.
#[must_use]
pub(crate) fn paginate_single_section_footnotes(
    document: &Document,
    shaper: &dyn LineShaper,
    run: &SectionRun,
) -> PaginatedLayout {
    let notes = build_footnote_galleys(document, shaper, run.config.content_area().size.width);
    if notes.is_empty() {
        return paginate_with_footnote_reservations(&run.galley, &run.config, &[]);
    }

    let mut reservations = Vec::new();
    for _ in 0..MAX_RESERVATION_PASSES {
        let mut layout =
            paginate_with_footnote_reservations(&run.galley, &run.config, &reservations);
        let next = page_reservations(&layout, &notes, &run.config);
        let merged = merge_reservations(&reservations, &next);
        if merged == reservations {
            place_footnotes(&mut layout, &notes, &next);
            return layout;
        }
        reservations = merged;
    }

    let layout = paginate_with_footnote_reservations(&run.galley, &run.config, &reservations);
    let final_reservations = merge_reservations(
        &reservations,
        &page_reservations(&layout, &notes, &run.config),
    );
    let mut layout =
        paginate_with_footnote_reservations(&run.galley, &run.config, &final_reservations);
    let placed_reservations = page_reservations(&layout, &notes, &run.config);
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

fn page_reservations(
    layout: &PaginatedLayout,
    notes: &BTreeMap<NoteId, Vec<BlockFragment>>,
    config: &PageConfig,
) -> Vec<Twip> {
    let cap = footnote_cap(config);
    layout
        .pages
        .iter()
        .map(|page| {
            page_footnote_ids(page)
                .into_iter()
                .filter_map(|id| notes.get(&id))
                .flat_map(|galley| galley.iter())
                .map(BlockFragment::height)
                .fold(Twip::ZERO, |a, h| a + h)
                .min(cap)
        })
        .collect()
}

fn footnote_cap(config: &PageConfig) -> Twip {
    let body = config.content_area().size.height.raw();
    Twip((body - MIN_BODY_HEIGHT_TWIPS).max(0))
}

fn place_footnotes(
    layout: &mut PaginatedLayout,
    notes: &BTreeMap<NoteId, Vec<BlockFragment>>,
    reservations: &[Twip],
) {
    for (index, page) in layout.pages.iter_mut().enumerate() {
        let band_height = reservations.get(index).copied().unwrap_or(Twip::ZERO);
        if band_height <= Twip::ZERO {
            continue;
        }
        let ids = page_footnote_ids(page);
        let band = Rect::new(
            Point::new(page.content_area.origin.x, page.content_area.bottom()),
            Size::new(page.content_area.size.width, band_height),
        );
        page.footnotes = stack_note_galleys(ids, notes, band);
    }
}

fn stack_note_galleys(
    ids: Vec<NoteId>,
    notes: &BTreeMap<NoteId, Vec<BlockFragment>>,
    band: Rect,
) -> Vec<PlacedFragment> {
    let mut placed = Vec::new();
    let mut y = band.origin.y;
    for id in ids {
        let Some(galley) = notes.get(&id) else {
            continue;
        };
        for fragment in galley {
            let height = fragment.height();
            placed.push(PlacedFragment {
                fragment: fragment.clone(),
                rect: Rect::new(
                    Point::new(band.origin.x, y),
                    Size::new(band.size.width, height),
                ),
            });
            y = y + height;
        }
    }
    placed
}

fn page_footnote_ids(page: &Page) -> Vec<NoteId> {
    let mut ids = Vec::new();
    for placed in &page.placed {
        collect_fragment_notes(&placed.fragment, &mut ids);
    }
    ids
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

fn collect_fragment_notes(fragment: &BlockFragment, ids: &mut Vec<NoteId>) {
    collect_fragment_note_markers(fragment, &mut |marker| {
        if marker.kind == NoteKind::Footnote {
            ids.push(marker.note);
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
        BlockNode, Definitions, InlineNode, Note, NoteReference, Paragraph, ParagraphProperties,
        Run, RunProperties,
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
                    kind: NoteKind::Footnote,
                    note,
                }),
            ],
        })
    }

    fn document(body: Vec<BlockNode>, definitions: Definitions) -> Document {
        Document::new(node(1), body, definitions).unwrap()
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
}
