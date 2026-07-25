//! The block/flow engine — turning a v1 [`Document`] into a shaped galley.
//!
//! This is the bridge from the semantic model to layout: each body paragraph's
//! inline runs (and their run properties) become [`StyledRun`]s, which the
//! [`LineShaper`] turns into positioned lines, yielding a [`BlockFragment`]. The
//! resulting galley is what the paginator ([`crate::paginate`]) slices into pages
//! and the renderer paints — so this closes the loop from imported DOCX to a
//! rendered page.
//!
//! Scope: body paragraphs (runs with size/color/weight/decoration, recursing
//! through hyperlink/revision/content-control wrappers) and **tables** (rows and
//! nested tables, cells flowed at their grid-column width). Inline drawings,
//! fields, block content controls, indents/tabs (`P1C-003b`), and cross-page
//! table splitting (`P1D-003`) are the following slices; unmapped inline nodes
//! contribute no text yet (never panic).

use casual_doc_model::v1::{
    Alignment, BlockNode, Color, Document, InlineNode, ParagraphProperties, Table,
};

use crate::block::{BlockFragment, BoxMetrics, BreakControl, CellFragment};
use crate::model::{ModelPos, ModelRange};
use crate::text::{Decoration, LineConstraints, LineShaper, StyledRun, TextAlignment};
use crate::units::Twip;

/// Builds a galley of block fragments from a document's body, shaped to fit
/// `content_width` (the page content-area width, in twips).
#[must_use]
pub fn build_galley(
    document: &Document,
    shaper: &dyn LineShaper,
    content_width: Twip,
) -> Vec<BlockFragment> {
    flow_blocks(document.body(), shaper, content_width)
}

/// Flows a sequence of block nodes (a body or a table cell) into shaped
/// fragments at `width`. Paragraphs shape to lines; tables expand to their rows;
/// block content controls are laid out in a later slice.
fn flow_blocks(blocks: &[BlockNode], shaper: &dyn LineShaper, width: Twip) -> Vec<BlockFragment> {
    let mut galley = Vec::new();
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let mut runs = Vec::new();
                collect_runs(&paragraph.inlines, &mut runs);
                let range = ModelRange::new(
                    ModelPos::new(paragraph.id, 0),
                    ModelPos::new(paragraph.id, 0),
                );
                let spacing = paragraph.properties.spacing.as_ref();
                let lines = shaper.shape_paragraph(
                    &runs,
                    LineConstraints {
                        max_width: width,
                        rtl: false,
                        alignment: alignment(&paragraph.properties),
                        line_height_percent: spacing.and_then(|s| s.line_percent),
                    },
                    range,
                );
                galley.push(BlockFragment::Paragraph {
                    id: paragraph.id,
                    lines,
                    box_metrics: box_metrics(&paragraph.properties),
                    break_control: break_control(&paragraph.properties),
                });
            }
            BlockNode::Table(table) => flow_table(table, shaper, width, &mut galley),
            BlockNode::Sdt(_) => {}
        }
    }
    galley
}

/// Flows a table into one [`BlockFragment::TableRow`] per row. Column widths come
/// from the grid (`w:gridCol`), distributed evenly when unspecified; a cell's
/// content is flowed at the width of the grid columns it spans. Cross-page row
/// splitting and header repetition are `P1D-003`.
fn flow_table(
    table: &Table,
    shaper: &dyn LineShaper,
    width: Twip,
    galley: &mut Vec<BlockFragment>,
) {
    let widths = column_widths(table, width);
    // Cumulative left edge of each column.
    let mut edges = Vec::with_capacity(widths.len() + 1);
    let mut x = Twip::ZERO;
    for w in &widths {
        edges.push(x);
        x = x + *w;
    }
    edges.push(x);

    for row in &table.rows {
        let mut cells = Vec::new();
        let mut col = 0usize;
        for cell in &row.cells {
            let span = cell.properties.grid_span.unwrap_or(1).max(1) as usize;
            let cell_x = edges.get(col).copied().unwrap_or(Twip::ZERO);
            let cell_end = edges
                .get((col + span).min(edges.len() - 1))
                .copied()
                .unwrap_or(x);
            let cell_width = cell_end - cell_x;
            cells.push(CellFragment {
                id: cell.id,
                grid_span: span as u32,
                x: cell_x,
                width: cell_width,
                blocks: flow_blocks(&cell.blocks, shaper, cell_width),
            });
            col += span;
        }
        galley.push(BlockFragment::TableRow {
            id: row.id,
            cells,
            can_split: !row.properties.cant_split,
            header: row.properties.header,
        });
    }
}

/// The width of each grid column (twips). Declared widths are used as-is; columns
/// with no declared width share the remaining space evenly (at least 1 twip).
fn column_widths(table: &Table, total: Twip) -> Vec<Twip> {
    if table.grid.is_empty() {
        return vec![total];
    }
    let declared: i32 = table.grid.iter().filter_map(|c| c.width_twips).sum();
    let undeclared = table
        .grid
        .iter()
        .filter(|c| c.width_twips.is_none())
        .count();
    let leftover = (total.raw() - declared).max(0);
    let each = if undeclared > 0 {
        (leftover / undeclared as i32).max(1)
    } else {
        0
    };
    table
        .grid
        .iter()
        .map(|c| Twip(c.width_twips.unwrap_or(each).max(1)))
        .collect()
}

/// Flattens a paragraph's inline nodes into styled text runs, recursing through
/// the wrappers that carry inline content (hyperlinks, revisions, content
/// controls). Text-bearing runs and explicit tabs contribute text; other inline
/// nodes are not yet laid out.
fn collect_runs<'a>(inlines: &'a [InlineNode], out: &mut Vec<StyledRun<'a>>) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => out.push(styled_run(&run.text, &run.properties)),
            InlineNode::Tab(_) => out.push(styled_run("\t", &Default::default())),
            InlineNode::Hyperlink(hyperlink) => collect_runs(&hyperlink.inlines, out),
            InlineNode::Revision(revision) => collect_runs(&revision.inlines, out),
            InlineNode::Sdt(sdt) => collect_runs(&sdt.inlines, out),
            _ => {}
        }
    }
}

/// Maps a run's text + properties to a styled run (default font for now; font
/// resolution is `P1C-002`).
fn styled_run<'a>(
    text: &'a str,
    properties: &casual_doc_model::v1::RunProperties,
) -> StyledRun<'a> {
    // `w:sz` is in half-points; a half-point is 10 twips (a point is 20). Default
    // to 11pt (Word's default body size) when unset.
    let size = properties
        .size_half_points
        .map_or(Twip::from_points(11), |hp| Twip(hp as i32 * 10));
    let color = match properties.color {
        Some(Color::Rgb(rgb)) => [rgb.r, rgb.g, rgb.b, 255],
        _ => [0, 0, 0, 255],
    };
    let bold = properties.bold.unwrap_or(false);
    let italic = properties.italic.unwrap_or(false);
    StyledRun {
        text,
        // Select the bundled face matching the run's weight/style so the renderer
        // outlines the same face `parley` shapes with.
        font: crate::fonts::face_id(bold, italic),
        size,
        bold,
        italic,
        letter_spacing: properties.character_spacing_twips.map_or(Twip::ZERO, Twip),
        color,
        decoration: Decoration {
            underline: properties.underline.unwrap_or(false),
            strikethrough: properties.strike.unwrap_or(false),
        },
    }
}

/// Maps model paragraph alignment to the layout alignment.
fn alignment(properties: &ParagraphProperties) -> TextAlignment {
    match properties.alignment {
        Some(Alignment::Start) | None => TextAlignment::Start,
        Some(Alignment::End) => TextAlignment::End,
        Some(Alignment::Center) => TextAlignment::Center,
        Some(Alignment::Justify) => TextAlignment::Justify,
    }
}

/// Maps paragraph break properties to the fragment's break control.
fn break_control(properties: &ParagraphProperties) -> BreakControl {
    BreakControl {
        page_break_before: properties.page_break_before,
        keep_next: properties.keep_next,
        keep_lines: properties.keep_lines,
        widow_control: properties.widow_control,
    }
}

/// Maps paragraph spacing/indent to the fragment's box metrics.
fn box_metrics(properties: &ParagraphProperties) -> BoxMetrics {
    let spacing = properties.spacing.as_ref();
    let indent = properties.indentation.as_ref();
    BoxMetrics {
        space_before: spacing
            .and_then(|s| s.before_twips)
            .map_or(Twip::ZERO, Twip),
        space_after: spacing.and_then(|s| s.after_twips).map_or(Twip::ZERO, Twip),
        indent_start: indent.and_then(|i| i.start_twips).map_or(Twip::ZERO, Twip),
        indent_end: indent.and_then(|i| i.end_twips).map_or(Twip::ZERO, Twip),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::ParleyShaper;
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::{
        BlockNode, Definitions, Document, InlineNode, Paragraph, ParagraphProperties, Run,
        RunProperties,
    };

    fn run_node(id: u64, text: &str, properties: RunProperties) -> InlineNode {
        InlineNode::Run(Run {
            id: NodeId::from_parts(id, 1).unwrap(),
            properties,
            text: text.to_owned(),
        })
    }

    fn paragraph(id: u64, inlines: Vec<InlineNode>) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(id, 1).unwrap(),
            properties: ParagraphProperties::default(),
            inlines,
        })
    }

    fn document(body: Vec<BlockNode>) -> Document {
        Document::new(
            NodeId::from_parts(1, 1).unwrap(),
            body,
            Definitions::default(),
        )
        .unwrap()
    }

    #[test]
    fn a_table_flows_to_a_row_fragment_with_positioned_cells() {
        use casual_doc_model::v1::{
            GridColumn, Table, TableCell, TableCellProperties, TableProperties, TableRow,
            TableRowProperties,
        };
        let cell = |id: u64, text: &str| TableCell {
            id: NodeId::from_parts(id, 1).unwrap(),
            properties: TableCellProperties::default(),
            blocks: vec![paragraph(
                id + 100,
                vec![run_node(id + 200, text, RunProperties::default())],
            )],
        };
        let table = BlockNode::Table(Table {
            id: NodeId::from_parts(50, 1).unwrap(),
            grid: vec![
                GridColumn {
                    width_twips: Some(3000),
                },
                GridColumn {
                    width_twips: Some(3000),
                },
            ],
            properties: TableProperties::default(),
            rows: vec![TableRow {
                id: NodeId::from_parts(51, 1).unwrap(),
                properties: TableRowProperties::default(),
                cells: vec![cell(60, "left cell"), cell(61, "right cell")],
            }],
        });
        let shaper = ParleyShaper::new();
        let galley = build_galley(&document(vec![table]), &shaper, Twip::from_points(400));
        assert_eq!(galley.len(), 1, "the table flows to one row fragment");
        let BlockFragment::TableRow { cells, .. } = &galley[0] else {
            panic!("expected a table row");
        };
        assert_eq!(cells.len(), 2);
        // First cell at x=0 width 3000; second at x=3000.
        assert_eq!(cells[0].x, Twip::ZERO);
        assert_eq!(cells[0].width, Twip(3000));
        assert_eq!(cells[1].x, Twip(3000));
        // Each cell shaped its paragraph.
        assert!(!cells[0].blocks.is_empty() && !cells[1].blocks.is_empty());
    }

    #[test]
    fn builds_a_shaped_fragment_per_paragraph() {
        let doc = document(vec![
            paragraph(
                10,
                vec![run_node(11, "First paragraph.", RunProperties::default())],
            ),
            paragraph(
                20,
                vec![run_node(
                    21,
                    "Second one, a bit longer.",
                    RunProperties::default(),
                )],
            ),
        ]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        assert_eq!(galley.len(), 2, "one fragment per paragraph");
        for fragment in &galley {
            let BlockFragment::Paragraph { lines, .. } = fragment else {
                panic!("expected a paragraph fragment");
            };
            assert!(!lines.lines.is_empty(), "the paragraph shaped to lines");
            assert!(fragment.height().raw() > 0, "positive height");
        }
    }

    #[test]
    fn run_size_and_color_flow_into_the_shaped_run() {
        use casual_doc_model::v1::{Color, RgbColor};
        let props = RunProperties {
            size_half_points: Some(48), // 24pt
            color: Some(Color::Rgb(RgbColor {
                r: 200,
                g: 60,
                b: 20,
            })),
            ..RunProperties::default()
        };
        let doc = document(vec![paragraph(10, vec![run_node(11, "Big red", props)])]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!();
        };
        let run = &lines.lines[0].runs[0];
        assert_eq!(run.color, [200, 60, 20, 255], "run color flows to layout");
        assert!(
            run.size.raw() >= Twip::from_points(20).raw(),
            "24pt size flows through"
        );
    }

    #[test]
    fn hyperlink_and_revision_text_is_collected() {
        use casual_doc_model::v1::{
            Hyperlink, HyperlinkTarget, InternalTarget, Revision, RevisionKind,
        };
        let link = InlineNode::Hyperlink(Hyperlink {
            id: NodeId::from_parts(30, 1).unwrap(),
            target: HyperlinkTarget::Internal(InternalTarget {
                anchor: "a".to_owned(),
            }),
            tooltip: None,
            inlines: vec![run_node(31, "linked", RunProperties::default())],
        });
        let rev = InlineNode::Revision(Revision {
            id: NodeId::from_parts(40, 1).unwrap(),
            kind: RevisionKind::Insertion,
            author: None,
            date: None,
            revision_id: None,
            inlines: vec![run_node(41, " inserted", RunProperties::default())],
        });
        let doc = document(vec![paragraph(10, vec![link, rev])]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!();
        };
        let glyphs: usize = lines
            .lines
            .iter()
            .flat_map(|l| &l.runs)
            .map(|r| r.glyphs.len())
            .sum();
        assert!(
            glyphs >= 12,
            "hyperlink + revision text both shaped (got {glyphs})"
        );
    }
}
