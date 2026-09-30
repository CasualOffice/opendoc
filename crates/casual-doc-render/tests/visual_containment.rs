use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::page::{Page, PlacedFragment};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use casual_doc_model::v1::BlockNode;
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use casual_doc_render::{BundledFontSource, MapMediaSource, Surface, render};

const VISUAL_CONTAINMENT_DOCX: &[u8] =
    include_bytes!("../../../fixtures/generated/visual-containment.docx");
const DPI: f32 = 96.0;
/// The rendered-pixel baseline.
///
/// # Re-blessed 2026-10-01 for the cell border reserve (FID-L-21)
///
/// Was `0x5230_4861_4c81_39a9`. A cell's horizontal borders used to occupy no
/// vertical space at all, so every bordered row was short by its borders'
/// thickness; this fixture's table declares `w:tblBorders` top/bottom/insideH as
/// `single w:sz="8"` — 20 twips — and its rows moved by exactly that:
///
/// ```text
/// row            height before -> after
/// page 2              4010 -> 4030   +20   its own top edge
/// page 3              5750 -> 5770   +20
/// page 4              5750 -> 5770   +20
/// page 5, first       2850 -> 2870   +20
/// page 5, second       530 ->  550   +20
/// page 5, third        530 ->  570   +40   top edge + the table's bottom perimeter
/// ```
///
/// Nothing else about the fixture changed: still five pages, and every
/// containment and split-row-ordering assertion below still holds — those are the
/// properties this test exists for, and they passed unchanged while only the hash
/// moved. A pixel hash cannot show *what* moved, which is why the twip figures
/// are written out here; they are reproducible by removing the reserve from
/// `CellFragment::occupied_height` and re-running.
///
/// # Re-blessed again, same day, for the line-box leading rule
///
/// Was `0xb368_c649_ad5b_235e`. A line's leading now sits entirely below its
/// baseline (Word/OOXML) instead of being centred (`parley`'s CSS half-leading) —
/// see `casual_doc_layout::shape::LineBox`. For this fixture's face the leading is
/// nearly zero, so the interesting part is not the size of the shift but that it
/// **removes an accumulating rounding error**. Each line's baseline used to creep
/// down by a fraction of a twip, and it compounded across the document:
///
/// ```text
/// first baseline on page   3     4     5
///                 before   972   975   978    (drifting)
///                 after    971   971   971    (exact, 290-twip pitch throughout)
/// ```
///
/// Still five pages, same number of painted runs per page, and every containment
/// and split-row assertion below unchanged.
const EXPECTED_RGBA_FNV1A64: u64 = 0xa3ae_e806_7550_7d76;

#[derive(Clone, Copy, Debug)]
struct InkLine {
    top: Twip,
    bottom: Twip,
    left: Twip,
    right: Twip,
}

fn paragraph_ink_lines(placed: &PlacedFragment) -> Vec<InkLine> {
    let BlockFragment::Paragraph {
        lines, box_metrics, ..
    } = &placed.fragment
    else {
        return Vec::new();
    };
    let content_x = placed.rect.origin.x + box_metrics.indent_start;
    let content_y = placed.rect.origin.y + box_metrics.space_before;
    lines
        .lines
        .iter()
        .filter_map(|line| {
            let baseline = line.runs.first()?.origin.y + content_y;
            let left = line.runs.iter().map(|run| content_x + run.origin.x).min()?;
            let right = line
                .runs
                .iter()
                .map(|run| {
                    run.glyphs
                        .iter()
                        .fold(content_x + run.origin.x, |x, glyph| x + glyph.advance)
                })
                .max()?;
            Some(InkLine {
                top: baseline - line.ascent,
                bottom: baseline + line.descent,
                left,
                right,
            })
        })
        .collect()
}

fn placed_paragraph(page: &Page, id: NodeId) -> &PlacedFragment {
    page.placed
        .iter()
        .find(|placed| placed.fragment.node_id() == id)
        .expect("fixture paragraph is placed on this page")
}

fn fnv1a64_extend(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

#[test]
fn generated_visual_containment_fixture_has_no_collisions_and_matches_baseline() {
    let mut package = DocxPackage::open(VISUAL_CONTAINMENT_DOCX, PackageLimits::default()).unwrap();
    let document = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .unwrap()
    .document;

    let [
        BlockNode::Paragraph(drop_cap),
        BlockNode::Paragraph(drop_cap_body),
        BlockNode::Paragraph(float_source),
        BlockNode::Paragraph(float_following),
        BlockNode::Table(table),
    ] = document.body()
    else {
        panic!("generated visual fixture keeps its five top-level blocks");
    };
    assert_eq!(table.rows.len(), 3);

    let shaper = ParleyShaper::new();
    let pages = paginate_document(&document, &shaper);
    assert_eq!(
        pages,
        paginate_document(&document, &shaper),
        "full pagination is field-for-field deterministic"
    );
    assert_eq!(
        pages.page_count(),
        5,
        "fixture geometry is intentionally pinned"
    );

    let first_page = &pages.pages[0];
    let drop_cap_placed = placed_paragraph(first_page, drop_cap.id);
    let BlockFragment::Paragraph {
        lines: drop_lines, ..
    } = &drop_cap_placed.fragment
    else {
        unreachable!();
    };
    assert_eq!(
        drop_cap_placed.rect.size.height,
        Twip::ZERO,
        "the framed initial does not advance the following body"
    );
    assert!(
        drop_lines.lines.iter().all(|line| !line.clip),
        "the large initial is never clipped to an ordinary line box"
    );
    assert!(
        drop_lines
            .lines
            .iter()
            .flat_map(|line| &line.runs)
            .any(|run| run.size >= Twip(1_170)),
        "the generated fixture keeps its 58.5pt initial"
    );

    let drop_body_lines = paragraph_ink_lines(placed_paragraph(first_page, drop_cap_body.id));
    assert!(
        drop_body_lines.first().unwrap().left > drop_body_lines.last().unwrap().left + Twip(500),
        "body text begins beside the initial and restores the full measure below it: {drop_body_lines:?}"
    );

    let anchor = first_page
        .anchored
        .first()
        .expect("generated left float is placed");
    let anchor_right = anchor.rect.origin.x + anchor.rect.size.width;
    let anchor_bottom = anchor.rect.origin.y + anchor.rect.size.height;
    let mut crossing_following_lines = 0;
    for paragraph_id in [float_source.id, float_following.id] {
        for line in paragraph_ink_lines(placed_paragraph(first_page, paragraph_id)) {
            if line.top < anchor_bottom && line.bottom > anchor.rect.origin.y {
                crossing_following_lines += usize::from(paragraph_id == float_following.id);
                assert!(
                    line.left >= anchor_right,
                    "line ink {:?} must not overlap left float {:?}",
                    line,
                    anchor.rect
                );
                assert!(
                    line.right
                        <= first_page.content_area.origin.x
                            + first_page.content_area.size.width
                            + Twip(60),
                    "narrowed line stays in the page content area: {line:?}, content {:?}",
                    first_page.content_area
                );
            }
        }
    }
    assert!(
        crossing_following_lines >= 2,
        "the gate exercises exclusion across a paragraph boundary"
    );

    let row_ids = [table.rows[0].id, table.rows[1].id, table.rows[2].id];
    let mut row_placements: [Vec<(usize, Twip, Twip)>; 3] = Default::default();
    for (page_index, page) in pages.pages.iter().enumerate() {
        let mut page_rows = Vec::new();
        for placed in &page.placed {
            let BlockFragment::TableRow {
                id, cells, height, ..
            } = &placed.fragment
            else {
                continue;
            };
            assert!(
                BlockFragment::cells_content_height(cells) <= *height,
                "split row cell content is contained by its emitted row fragment"
            );
            let bottom = placed.rect.origin.y + placed.rect.size.height;
            page_rows.push((placed.rect.origin.y, bottom));
            let row_index = row_ids
                .iter()
                .position(|candidate| candidate == id)
                .expect("all placed rows belong to the generated table");
            row_placements[row_index].push((page_index, placed.rect.origin.y, bottom));
        }
        page_rows.sort_by_key(|(top, _)| *top);
        for pair in page_rows.windows(2) {
            assert!(
                pair[0].1 <= pair[1].0,
                "successive row fragments must not overpaint one another"
            );
        }
    }
    assert!(
        row_placements[0].len() >= 2,
        "the first row actually splits over pages"
    );
    assert_eq!(row_placements[1].len(), 1);
    assert_eq!(row_placements[2].len(), 1);
    let flow_key = |placement: &(usize, Twip, Twip)| (placement.0, placement.1);
    assert!(
        flow_key(row_placements[0].last().unwrap()) < flow_key(&row_placements[1][0]),
        "the first successor starts after every fragment of the split row"
    );
    assert!(
        flow_key(&row_placements[1][0]) < flow_key(&row_placements[2][0]),
        "the second successor remains after the first"
    );

    let mut media = MapMediaSource::new();
    for (_, reference) in document.definitions().media.iter() {
        media.insert(
            reference.part_name.clone(),
            package.read_part(&reference.part_name).unwrap(),
        );
    }
    let mut hash = 0xcbf2_9ce4_8422_2325;
    for page in &pages.pages {
        let width = page.page_size.width.to_device_px(DPI).ceil() as u32;
        let height = page.page_size.height.to_device_px(DPI).ceil() as u32;
        let mut surface = Surface::new(width, height).unwrap();
        render(
            &compose_page(page),
            &mut surface,
            DPI,
            &BundledFontSource,
            &media,
        );
        fnv1a64_extend(&mut hash, &page.number.to_le_bytes());
        fnv1a64_extend(&mut hash, &width.to_le_bytes());
        fnv1a64_extend(&mut hash, &height.to_le_bytes());
        fnv1a64_extend(&mut hash, surface.data());
    }
    assert_eq!(
        hash, EXPECTED_RGBA_FNV1A64,
        "generated five-page RGBA baseline changed: {hash:016x}"
    );
}
