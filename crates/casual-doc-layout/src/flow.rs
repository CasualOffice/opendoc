//! The block/flow engine — turning a v1 [`Document`] into a shaped galley.
//!
//! This is the bridge from the semantic model to layout: each body paragraph's
//! inline runs (and their run properties) become [`StyledRun`]s, which the
//! [`LineShaper`] turns into positioned lines, yielding a [`BlockFragment`]. The
//! resulting galley is what the paginator ([`crate::paginate`]) slices into pages
//! and the renderer paints — so this closes the loop from imported DOCX to a
//! rendered page.
//!
//! Scope (`P1C-004`): body paragraphs of text runs (with size/color/decoration),
//! recursing through hyperlink/revision/content-control wrappers. Tables, inline
//! drawings, fields, and the fuller run/paragraph property mapping
//! (alignment/indent/tabs — `P1C-003`) and font resolution (`P1C-002`) are the
//! following slices; unmapped inline nodes contribute no text yet (never panic).

use casual_doc_model::v1::{
    Alignment, BlockNode, Color, Document, InlineNode, ParagraphProperties,
};

use crate::block::{BlockFragment, BoxMetrics, BreakControl};
use crate::model::{ModelPos, ModelRange};
use crate::text::{Decoration, LineConstraints, LineShaper, StyledRun, TextAlignment};
use crate::units::Twip;

/// Builds a galley of block fragments from a document's body, shaped to fit
/// `content_width` (the page content-area width, in twips). Only paragraphs are
/// laid out in this slice; other block kinds are skipped.
#[must_use]
pub fn build_galley(
    document: &Document,
    shaper: &dyn LineShaper,
    content_width: Twip,
) -> Vec<BlockFragment> {
    let mut galley = Vec::new();
    for block in document.body() {
        if let BlockNode::Paragraph(paragraph) = block {
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
                    max_width: content_width,
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
        // Tables and block content controls are laid out in later slices.
    }
    galley
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
