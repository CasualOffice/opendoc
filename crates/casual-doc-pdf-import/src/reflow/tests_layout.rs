//! Reflow layout and mapping families found by comparing reflowed pages with
//! their originals (`docs/182` §18.11): emphasis inside a paragraph, bevelled
//! table borders, the measured vertical rhythm, pictures larger than the
//! column, synthesised italics, centred headings and ruled row heights. Each
//! fixture is synthetic (`synth.rs`); each guard's mutation is in the commit
//! that added it.

use casual_doc_model::v1::{
    Alignment, BlockNode, Document, HeightRule, InlineNode, Paragraph, Spacing,
};
// Own line: sections and drawings.
use casual_doc_model::v1::{FieldKind, GroupChild, HorizontalAnchor, SectionType, VerticalAnchor};

use crate::synth::{self, Page, text};
use crate::{PdfImportError, PdfReadLimits, Reflowed, import_pdf};

fn reflow_of(bytes: Vec<u8>) -> Reflowed {
    let imported = import_pdf(bytes, PdfReadLimits::default()).expect("imports");
    let reflowed: Result<Reflowed, PdfImportError> = imported.reflow();
    let reflowed = reflowed.expect("reflows");
    reflowed.document.validate().expect("valid");
    reflowed
}

fn inline_text(inlines: &[InlineNode], out: &mut String) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => out.push_str(&run.text),
            InlineNode::Hyperlink(h) => inline_text(&h.inlines, out),
            _ => {}
        }
    }
}

fn para_text(p: &Paragraph) -> String {
    let mut out = String::new();
    inline_text(&p.inlines, &mut out);
    out
}

fn paragraphs(document: &Document) -> Vec<&Paragraph> {
    document
        .body()
        .iter()
        .filter_map(|b| match b {
            BlockNode::Paragraph(p) if !para_text(p).trim().is_empty() => Some(p),
            _ => None,
        })
        .collect()
}

fn style_name(document: &Document, p: &Paragraph) -> Option<String> {
    p.properties
        .style_ref
        .and_then(|id| document.definitions().styles.get(&id))
        .and_then(|s| s.name.clone())
}

fn normal_spacing(document: &Document) -> Spacing {
    document
        .definitions()
        .styles
        .iter()
        .find(|(_, s)| s.is_default)
        .and_then(|(_, s)| s.paragraph.as_ref())
        .and_then(|p| p.spacing)
        .expect("Normal has spacing")
}

/// Courier and Courier Bold: both advance 0.6 em, so every full line of
/// the fixtures below ends at the same x.
const COURIER: &str = "/Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >> /F2 << /Type /Font /Subtype /Type1 /BaseFont /Courier-Bold /Encoding /WinAnsiEncoding >> >>";

/// `seed` repeated (space separated) to exactly 60 characters.
fn full(seed: &str) -> String {
    let mut line = String::new();
    while line.len() < 60 {
        line.push_str(seed);
        line.push(' ');
    }
    line.truncate(60);
    line
}

/// Two paragraphs of six 11 pt lines at `leading`, `gap` extra points
/// between them; the fifth line of the first is Courier Bold from end to end
/// (an emphasised phrase that fills a whole line).
fn prose_pdf(leading: f64, gap: f64) -> Vec<u8> {
    let lines = [
        full("plain"),
        full("words"),
        full("again"),
        full("still"),
        full("EMPHASIS"),
        "and it ends here.".to_owned(),
    ];
    let mut content = String::new();
    let mut y = 700.0;
    for paragraph in 0..2 {
        for (i, line) in lines.iter().enumerate() {
            let bold = paragraph == 0 && i == 4;
            content.push_str(&text(
                if bold { "F2" } else { "F1" },
                11.0,
                72.0,
                y,
                leading,
                &[line],
            ));
            y -= leading;
        }
        y -= gap;
    }
    synth::document(&[Page::new(content)], COURIER, "")
}

#[test]
fn a_bold_phrase_that_fills_a_line_stays_inside_its_paragraph() {
    let reflowed = reflow_of(prose_pdf(14.0, 12.0));
    let document = &reflowed.document;
    let paras = paragraphs(document);
    // Two paragraphs, not four: the bold line neither ends the first nor
    // becomes a paragraph (or a heading) of its own.
    assert_eq!(
        paras.len(),
        2,
        "{:?}",
        paras.iter().map(|p| para_text(p)).collect::<Vec<_>>()
    );
    let first = para_text(paras[0]);
    assert!(first.contains("still EMPHASIS"), "{first}");
    assert!(first.ends_with("ends here."), "{first}");
    assert!(paras.iter().all(|p| style_name(document, p).is_none()));
    // The phrase keeps its weight as a run.
    let bold = paras[0].inlines.iter().any(|i| {
        matches!(i, InlineNode::Run(r) if r.text.contains("EMPHASIS") && r.properties.bold == Some(true))
    });
    assert!(bold);
}

#[test]
fn the_pdf_line_pitch_and_paragraph_gap_become_normal_spacing() {
    // 11 pt set on 18 pt (1.636 em, 142% of single) with 12 pt between
    // paragraphs.
    let spacing = normal_spacing(&reflow_of(prose_pdf(18.0, 12.0)).document);
    let percent = spacing.line_percent.expect("measured leading");
    assert!((140..=145).contains(&percent), "{percent}");
    let after = spacing.after_twips.expect("space after");
    assert!((220..=260).contains(&after), "{after}");
    // Single spacing and no gap stay single, with nothing after.
    let spacing = normal_spacing(&reflow_of(prose_pdf(12.65, 0.0)).document);
    assert_eq!(spacing.line_percent, None);
    assert!(spacing.after_twips.unwrap_or(0) <= 20, "{spacing:?}");
}

/// A filled bar with bevelled ends, LibreOffice's way of painting a table
/// border: six corners around the segment, `t` thick.
fn bar(x0: f64, y0: f64, x1: f64, y1: f64, t: f64) -> String {
    let h = t / 2.0;
    if (y0 - y1).abs() < 0.01 {
        format!(
            "{x1} {y0} m {x1} {a} l {x0} {a} l {xa} {y0} l {x0} {b} l {x1} {b} l {x1} {y0} l h f*\n",
            a = y0 - h,
            b = y0 + h,
            xa = x0 - h,
        )
    } else {
        format!(
            "{x0} {y1} m {a} {y1} l {a} {y0} l {x0} {ya} l {b} {y0} l {b} {y1} l {x0} {y1} l h f*\n",
            a = x0 - h,
            b = x0 + h,
            ya = y0 - h,
        )
    }
}

fn bevelled_table_pdf() -> Vec<u8> {
    let xs = [72.0, 200.0, 330.0, 460.0];
    let ys = [600.0, 571.0, 542.0, 513.0];
    let mut content = text("F2", 11.0, 72.0, 640.0, 14.0, &["A ruled table follows."]);
    content.push_str("0 0 0 rg\n");
    for y in ys {
        content.push_str(&bar(xs[0], y, xs[3], y, 0.2));
    }
    for x in xs {
        content.push_str(&bar(x, ys[3], x, ys[0], 0.2));
    }
    for (r, y) in ys.windows(2).enumerate() {
        for (c, x) in xs.windows(2).enumerate() {
            content.push_str(&text(
                "F2",
                10.0,
                x[0] + 4.0,
                y[0] - 14.0,
                12.0,
                &[&format!("R{}C{}", r + 1, c + 1)],
            ));
        }
    }
    content.push_str(&text(
        "F2",
        11.0,
        72.0,
        490.0,
        14.0,
        &["The paragraph under the table."],
    ));
    synth::document(&[Page::new(content)], "", "")
}

#[test]
fn bevelled_bars_rule_a_table_with_its_row_heights_and_the_space_under_it() {
    let reflowed = reflow_of(bevelled_table_pdf());
    let body = reflowed.document.body();
    let table = body
        .iter()
        .find_map(|b| match b {
            BlockNode::Table(t) => Some(t),
            _ => None,
        })
        .expect("the bars rule a table");
    assert_eq!(table.rows.len(), 3);
    assert!(table.rows.iter().all(|r| r.cells.len() == 3));
    // Each ruled row keeps the PDF's 29 pt as a minimum, and its cell
    // paragraphs add no space of their own.
    for row in &table.rows {
        assert_eq!(row.properties.height.rule, Some(HeightRule::AtLeast));
        let h = row.properties.height.value_twips.expect("height");
        assert!((570..=590).contains(&h), "{h}");
        for cell in &row.cells {
            for block in &cell.blocks {
                if let BlockNode::Paragraph(p) = block {
                    assert_eq!(
                        p.properties.spacing.as_ref().and_then(|s| s.after_twips),
                        Some(0)
                    );
                }
            }
        }
    }
    // The paragraph under the table keeps the space above it.
    let under = body
        .iter()
        .skip_while(|b| !matches!(b, BlockNode::Table(_)))
        .find_map(|b| match b {
            BlockNode::Paragraph(p) if !para_text(p).is_empty() => Some(p),
            _ => None,
        })
        .expect("a paragraph under the table");
    let before = under
        .properties
        .spacing
        .as_ref()
        .and_then(|s| s.before_twips);
    assert!(before.is_some_and(|b| b > 100), "{before:?}");
}

fn with_picture(content: String) -> Vec<u8> {
    let mut pdf = synth::Pdf::default();
    let image = pdf.stream(
        "/Type /XObject /Subtype /Image /Width 4 /Height 4 /ColorSpace /DeviceGray /BitsPerComponent 8",
        &[128u8; 16],
    );
    synth::document_in(
        &mut pdf,
        &[Page::new(content)],
        &format!("/XObject << /Im1 {image} 0 R >>"),
        "",
        |_| String::new(),
    )
}

#[test]
fn a_page_sized_picture_is_fitted_to_the_column_and_the_page() {
    // A scanned page: one picture the size of the page and no text.
    let reflowed = reflow_of(with_picture("q 612 0 0 792 0 0 cm /Im1 Do Q\n".to_owned()));
    let document = &reflowed.document;
    let section = document.definitions().sections.first().expect("a section");
    let m = section.page_margins;
    let column = f64::from(section.page_size.width_twips - m.start_twips - m.end_twips) / 20.0;
    let height = f64::from(section.page_size.height_twips - m.top_twips - m.bottom_twips) / 20.0;
    let extent = document
        .body()
        .iter()
        .find_map(|b| match b {
            BlockNode::Paragraph(p) => p.inlines.iter().find_map(|i| match i {
                InlineNode::Drawing(d) => d.extent,
                _ => None,
            }),
            _ => None,
        })
        .expect("an inline picture");
    let (w, h) = (
        extent.width_emu as f64 / 12_700.0,
        extent.height_emu as f64 / 12_700.0,
    );
    assert!(
        w <= column + 0.5 && h <= height,
        "{w} x {h} in {column} x {height}"
    );
    // The aspect is kept.
    assert!((w / h - 612.0 / 792.0).abs() < 0.01, "{w} x {h}");
}

#[test]
fn a_slanted_upright_face_is_italic_in_both_layouts() {
    // Times-Roman sheared by the text matrix: the italic LibreOffice and
    // Quartz synthesise for a family without an italic program.
    let mut content = text(
        "F2",
        11.0,
        72.0,
        700.0,
        14.0,
        &["Upright words come first,"],
    );
    content.push_str("BT /F2 11 Tf 1 0 0.2126 1 72 686 Tm (and these words lean.) Tj ET\n");
    let bytes = synth::document(&[Page::new(content)], "", "");
    let italic_run = |inlines: &[InlineNode]| {
        inlines.iter().any(|i| {
            matches!(i, InlineNode::Run(r) if r.text.contains("lean") && r.properties.italic == Some(true))
        })
    };
    let reflowed = reflow_of(bytes.clone());
    assert!(
        paragraphs(&reflowed.document)
            .iter()
            .any(|p| italic_run(&p.inlines))
    );
    assert!(
        !paragraphs(&reflowed.document).iter().any(|p| p.inlines.iter().any(|i| {
            matches!(i, InlineNode::Run(r) if r.text.contains("Upright") && r.properties.italic == Some(true))
        }))
    );
    // The fixed layout names the same face.
    let imported = import_pdf(bytes, PdfReadLimits::default()).expect("imports");
    let mut found = false;
    let mut stack: Vec<&[BlockNode]> = vec![imported.document.body()];
    while let Some(blocks) = stack.pop() {
        for block in blocks {
            if let BlockNode::Paragraph(p) = block {
                found |= italic_run(&p.inlines);
                for inline in &p.inlines {
                    if let InlineNode::TextBox(t) = inline {
                        stack.push(&t.blocks);
                    }
                }
            }
        }
    }
    assert!(found, "the fixed layout's run is italic too");
}

#[test]
fn a_centred_heading_stays_centred() {
    // Courier advances 0.6 em, so the geometry is exact: 71 characters of
    // 11 pt body run from 72 to 540.6, and the 13-character 22 pt title is
    // centred on 306.3 when it starts at 220.5.
    let body = |seed: &str| -> String {
        let mut line = String::new();
        while line.len() < 71 {
            line.push_str(seed);
            line.push(' ');
        }
        line.truncate(71);
        line
    };
    let (a, b) = (body("words"), body("prose"));
    let lines = [a.as_str(), b.as_str(), a.as_str(), "ends here."];
    let mut content = text("F6", 22.0, 220.5, 720.0, 26.0, &["Centred Title"]);
    content.push_str(&text("F6", 15.0, 72.0, 680.0, 18.0, &["First section"]));
    content.push_str(&text("F6", 11.0, 72.0, 660.0, 14.0, &lines));
    content.push_str(&text("F6", 15.0, 72.0, 590.0, 18.0, &["Second section"]));
    content.push_str(&text("F6", 11.0, 72.0, 570.0, 14.0, &lines));
    let reflowed = reflow_of(synth::document(&[Page::new(content)], "", ""));
    let document = &reflowed.document;
    let title = paragraphs(document)
        .into_iter()
        .find(|p| para_text(p) == "Centred Title")
        .expect("the title");
    assert_eq!(style_name(document, title).as_deref(), Some("Title"));
    assert_eq!(title.properties.alignment, Some(Alignment::Center));
}

/// Every inline drawing or shape group in the body (none is expected where
/// a fill or a rule became formatting).
fn graphics(document: &Document) -> usize {
    document
        .body()
        .iter()
        .filter_map(|b| match b {
            BlockNode::Paragraph(p) => Some(
                p.inlines
                    .iter()
                    .filter(|i| {
                        matches!(
                            i,
                            InlineNode::Drawing(_)
                                | InlineNode::AnchoredDrawing(_)
                                | InlineNode::Group(_)
                        )
                    })
                    .count(),
            ),
            _ => None,
        })
        .sum()
}

fn run_with<'a>(document: &'a Document, word: &str) -> &'a casual_doc_model::v1::Run {
    paragraphs(document)
        .into_iter()
        .flat_map(|p| p.inlines.iter())
        .find_map(|i| match i {
            InlineNode::Run(r) if r.text.contains(word) => Some(r),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a run holding {word}"))
}

/// Courier at 11 pt advances 6.6 pt a character, so the rectangles below
/// sit exactly under, through and behind the words they decorate.
fn decorations_pdf() -> Vec<u8> {
    let mut content = String::new();
    // "HIGH" (characters 28 to 31) on a yellow fill, painted first.
    content.push_str("1 1 0 rg 256.8 697 26.4 11 re f 0 g\n");
    content.push_str(&text(
        "F1",
        11.0,
        72.0,
        700.0,
        14.0,
        &["aaaa UNDER bbbb STRIKE cccc HIGH dddd."],
    ));
    // "UNDER" (5 to 9) underlined 1.5 pt below the baseline; "STRIKE" (16
    // to 21) struck 3 pt above it.
    content.push_str("105 697.7 33 0.8 re f\n177.6 703 39.6 0.7 re f\n");
    // A short paragraph on a pale fill as wide as the column, with a blue
    // rule under it: its paragraph's shading and bottom border.
    content.push_str("0.9 0.9 1 rg 66 656 474 14 re f 0 g\n");
    content.push_str(&text("F1", 11.0, 72.0, 660.0, 14.0, &["Shaded paragraph."]));
    content.push_str("0 0 1 rg 66 652 474 1 re f 0 g\n");
    content.push_str(&text(
        "F1",
        11.0,
        72.0,
        620.0,
        14.0,
        &["A plain paragraph after it."],
    ));
    synth::document(&[Page::new(content)], COURIER, "")
}

#[test]
fn rules_and_fills_drawn_beside_text_become_its_formatting() {
    let reflowed = reflow_of(decorations_pdf());
    let document = &reflowed.document;
    assert_eq!(run_with(document, "UNDER").properties.underline, Some(true));
    assert_eq!(run_with(document, "STRIKE").properties.strike, Some(true));
    assert_eq!(
        run_with(document, "HIGH").properties.shading.fill,
        Some(casual_doc_model::v1::RgbColor {
            r: 255,
            g: 255,
            b: 0
        })
    );
    // The words between keep none of it.
    let plain = run_with(document, "bbbb");
    assert_eq!(plain.properties.underline, None);
    assert_eq!(plain.properties.strike, None);
    assert_eq!(plain.properties.shading.fill, None);
    let shaded = paragraphs(document)
        .into_iter()
        .find(|p| para_text(p) == "Shaded paragraph.")
        .expect("the shaded paragraph");
    assert_eq!(
        shaded.properties.shading.fill,
        Some(casual_doc_model::v1::RgbColor {
            r: 230,
            g: 230,
            b: 255
        })
    );
    let border = shaded
        .properties
        .borders
        .bottom
        .clone()
        .expect("a bottom border");
    assert_eq!(
        border.color,
        Some(casual_doc_model::v1::RgbColor { r: 0, g: 0, b: 255 })
    );
    let after = paragraphs(document)
        .into_iter()
        .find(|p| para_text(p).starts_with("A plain"))
        .expect("the paragraph after");
    assert!(after.properties.borders.bottom.is_none());
    assert_eq!(after.properties.shading.fill, None);
    // None of the rectangles is placed as a drawing.
    assert_eq!(graphics(document), 0);
}

/// A borderless table of four rows 20 pt apart, its header row on a fill
/// that runs from 60 to 540, wider than its text (72 to 396).
fn banded_table_pdf() -> Vec<u8> {
    let mut content = text(
        "F1",
        11.0,
        72.0,
        720.0,
        14.0,
        &["A borderless table follows."],
    );
    content.push_str("0.9 0.9 0.9 rg 60 643 480 16 re f 0 g\n");
    let rows = [
        ["Name", "Kind", "Result"],
        ["alpha", "first", "pass"],
        ["beta", "second", "review"],
        ["gamma", "third", "pass"],
    ];
    for (r, row) in rows.iter().enumerate() {
        let y = 647.0 - 20.0 * r as f64;
        for (c, cell) in row.iter().enumerate() {
            content.push_str(&text("F1", 10.0, [72.0, 200.0, 330.0][c], y, 12.0, &[cell]));
        }
    }
    content.push_str(&text("F1", 11.0, 72.0, 540.0, 14.0, &["After the table."]));
    synth::document(&[Page::new(content)], COURIER, "")
}

#[test]
fn a_borderless_table_takes_its_fills_width_and_its_rows_pitch() {
    let reflowed = reflow_of(banded_table_pdf());
    let document = &reflowed.document;
    let table = document
        .body()
        .iter()
        .find_map(|b| match b {
            BlockNode::Table(t) => Some(t),
            _ => None,
        })
        .expect("a table");
    assert_eq!(table.rows.len(), 4);
    // The text starts 12 pt into the fill, and no cell adds Word's default
    // margins to text that was measured to its column.
    let margins = table.properties.cell_margins;
    assert_eq!(margins.start_twips, Some(240));
    assert_eq!(margins.end_twips, Some(0));
    // The last column reaches the fill's right edge, not its text's.
    let widths: Vec<i32> = table.grid.iter().filter_map(|g| g.width_twips).collect();
    let total: i32 = widths.iter().sum();
    assert!(total >= twips(540.0 - 72.0) - 20, "{widths:?}");
    // The header row carries the fill; the others none.
    let fill = |r: usize| table.rows[r].cells[0].properties.shading.fill;
    assert!(fill(0).is_some());
    assert!(fill(1).is_none());
    // Each row is the PDF's 20 pt apart, as a minimum.
    for row in &table.rows {
        assert_eq!(row.properties.height.rule, Some(HeightRule::AtLeast));
        assert_eq!(row.properties.height.value_twips, Some(400));
    }
    assert_eq!(graphics(document), 0);
}

fn twips(points: f64) -> i32 {
    (points * 20.0).round() as i32
}

#[test]
fn a_line_set_with_leading_spaces_keeps_its_indent() {
    let content = text(
        "F1",
        11.0,
        72.0,
        700.0,
        14.0,
        &[
            "This paragraph sets the column: it runs to its right edge.",
            "",
            "fn main() {",
            "    body();",
            "}",
        ],
    );
    let reflowed = reflow_of(synth::document(&[Page::new(content)], COURIER, ""));
    let body = paragraphs(&reflowed.document)
        .into_iter()
        .find(|p| para_text(p).contains("body();"))
        .expect("the indented line");
    // Four Courier spaces at 11 pt: 26.4 pt.
    let start = body
        .properties
        .indentation
        .as_ref()
        .and_then(|i| i.start_twips)
        .unwrap_or_else(|| panic!("{:?}", body.properties));
    assert!((start - 528).abs() <= 4, "{start}");
}

#[test]
fn a_landscape_page_is_its_own_landscape_section() {
    // A title page without the running head, a landscape page and two
    // upright pages with it; every page but the last stops early (so each
    // would start with a page break), the last runs down the page.
    let head = |height: f64| text("F1", 9.0, 72.0, height - 30.0, 11.0, &["Running head"]);
    let page = |x: &str, y: f64| text("F1", 11.0, 72.0, y, 14.0, &[x]);
    let prose = full("prose");
    let long: Vec<&str> = std::iter::repeat_n(prose.as_str(), 40).collect();
    let landscape = Page {
        content: format!(
            "{}{}",
            head(612.0),
            page("A wide page in the middle.", 540.0)
        ),
        extra: "/MediaBox [0 0 792 612]".to_owned(),
        ..Page::default()
    };
    let bytes = synth::document(
        &[
            Page::new(page("The first page is upright.", 720.0)),
            landscape,
            Page::new(format!("{}{}", head(792.0), page("Upright again.", 720.0))),
            Page::new(format!(
                "{}{}",
                head(792.0),
                text("F1", 11.0, 72.0, 720.0, 14.0, &long)
            )),
        ],
        COURIER,
        "",
    );
    let reflowed = reflow_of(bytes);
    let document = &reflowed.document;
    let sections = &document.definitions().sections;
    let shape: Vec<(i32, i32)> = sections
        .iter()
        .map(|s| (s.page_size.width_twips, s.page_size.height_twips))
        .collect();
    assert_eq!(
        shape,
        vec![(12_240, 15_840), (15_840, 12_240), (12_240, 15_840)]
    );
    // Each change of size starts a new page, once: the paragraph opening a
    // section of another size carries no page break of its own.
    assert_eq!(sections[1].section_type, Some(SectionType::NextPage));
    assert_eq!(sections[2].section_type, Some(SectionType::NextPage));
    let opening = |text: &str| {
        paragraphs(document)
            .into_iter()
            .find(|p| para_text(p).starts_with(text))
            .unwrap_or_else(|| panic!("{text}"))
            .properties
            .page_break_before
    };
    assert_eq!(opening("A wide page"), None);
    assert_eq!(opening("Upright again"), None);
    // The first page is a title page; the later sections' first pages carry
    // the running head.
    assert_eq!(sections[0].title_page, Some(true));
    assert!(sections[1..].iter().all(|s| s.title_page.is_none()));
}

/// A chart closing its page: a frame, three bars painted between their
/// labels, axis values just left of the frame (5 pt off) and a legend inside
/// it, under a paragraph of prose; the next page is a full page of prose.
fn chart_pdf() -> Vec<u8> {
    let mut content = text(
        "F1",
        11.0,
        72.0,
        720.0,
        14.0,
        &["A paragraph of prose comes before the chart, and it ends here."],
    );
    content.push_str("0.5 G 0.5 w 150 100 m 450 100 l 450 300 l 150 300 l h S 0 G\n");
    content.push_str(&text("F1", 8.0, 140.0, 100.0, 10.0, &["0"]));
    content.push_str("0 0 1 rg 180 100 40 120 re f 0 g\n");
    content.push_str(&text("F1", 8.0, 135.0, 200.0, 10.0, &["50"]));
    content.push_str("1 0 0 rg 260 100 40 160 re f 0 g\n");
    content.push_str(&text("F1", 8.0, 130.0, 290.0, 10.0, &["100"]));
    content.push_str("0 1 0 rg 340 100 40 80 re f 0 g\n");
    content.push_str(&text("F1", 8.0, 390.0, 270.0, 10.0, &["Series A"]));
    content.push_str("0 0 1 rg 380 270 6 6 re f 0 g\n");
    let prose = full("prose");
    let lines: Vec<&str> = std::iter::repeat_n(prose.as_str(), 40)
        .chain(["and the prose ends here."])
        .collect();
    let next = text("F1", 11.0, 72.0, 720.0, 14.0, &lines);
    synth::document(&[Page::new(content), Page::new(next)], COURIER, "")
}

#[test]
fn a_charts_labels_are_drawn_with_it_and_its_pieces_are_one_drawing() {
    let reflowed = reflow_of(chart_pdf());
    let document = &reflowed.document;
    let texts: Vec<String> = paragraphs(document).iter().map(|p| para_text(p)).collect();
    // No label is a paragraph of its own.
    for label in ["0", "50", "100", "Series A"] {
        assert!(!texts.iter().any(|t| t == label), "{texts:?}");
    }
    assert_eq!(
        texts.len(),
        2,
        "{:?}",
        texts
            .iter()
            .map(|t| t.chars().take(20).collect::<String>())
            .collect::<Vec<_>>()
    );
    // One group holds the frame, the bars, the key and the four labels.
    let groups: Vec<&casual_doc_model::v1::WordprocessingGroup> = document
        .body()
        .iter()
        .filter_map(|b| match b {
            BlockNode::Paragraph(p) => Some(p.inlines.iter().filter_map(|i| match i {
                InlineNode::Group(g) => Some(g.as_ref()),
                _ => None,
            })),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(groups.len(), 1);
    let boxes: Vec<String> = groups[0]
        .children
        .iter()
        .filter_map(|c| match c {
            GroupChild::TextBox(t) => t.blocks.iter().find_map(|b| match b {
                BlockNode::Paragraph(p) => Some(para_text(p)),
                _ => None,
            }),
            _ => None,
        })
        .collect();
    assert_eq!(boxes.len(), 4, "{boxes:?}");
    assert!(boxes.iter().any(|b| b == "Series A"));
    let shapes = groups[0]
        .children
        .iter()
        .filter(|c| matches!(c, GroupChild::Shape(_)))
        .count();
    assert!(shapes >= 5, "{shapes}");
    // A chart closing its page is content: the page did not end early, so
    // the next page's prose is no page break.
    assert!(
        paragraphs(document)
            .iter()
            .all(|p| p.properties.page_break_before != Some(true))
    );
}

#[test]
fn a_tagged_table_with_a_filled_header_stays_a_table() {
    // NISTIR 8105 (a US federal work, committed in the reflow corpus): its
    // Table 1 is tagged, its header row a black fill, its cells ruled. The
    // fill and the rules once made it a figure and its cells that figure's
    // labels, so the table vanished into a drawing.
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/pdf-reflow-corpus/nist-ir8105.pdf"),
    )
    .expect("the committed corpus file");
    let reflowed = reflow_of(bytes);
    let table = reflowed
        .document
        .body()
        .iter()
        .find_map(|b| match b {
            BlockNode::Table(t)
                if t.rows.first().is_some_and(|r| {
                    r.cells.first().is_some_and(|c| {
                        c.blocks.iter().any(|b| {
                            matches!(b, BlockNode::Paragraph(p) if para_text(p).contains("Cryptographic Algorithm"))
                        })
                    })
                }) =>
            {
                Some(t)
            }
            _ => None,
        })
        .expect("Table 1 is a table");
    assert_eq!(table.rows.len(), 6);
    assert!(table.rows.iter().all(|r| r.cells.len() == 4));
}

/// Three pages, each with the same ruled title block at the top ("Form 7",
/// "Page N of 3") beside the same logo, and body prose under it.
fn running_table_pdf() -> Vec<u8> {
    let mut pdf = synth::Pdf::default();
    let image = pdf.stream(
        "/Type /XObject /Subtype /Image /Width 4 /Height 4 /ColorSpace /DeviceGray /BitsPerComponent 8",
        &[128u8; 16],
    );
    let prose = full("prose");
    let lines: Vec<&str> = std::iter::repeat_n(prose.as_str(), 30).collect();
    let pages: Vec<Page> = (1..=3)
        .map(|n| {
            let mut content = String::from("q 40 0 0 40 72 712 cm /Im1 Do Q\n0 g\n");
            for y in [752.0, 732.0, 712.0] {
                content.push_str(&synth::line(130.0, y, 540.0, y));
            }
            for x in [130.0, 330.0, 540.0] {
                content.push_str(&synth::line(x, 712.0, x, 752.0));
            }
            content.push_str(&text("F1", 10.0, 135.0, 738.0, 12.0, &["Form 7"]));
            content.push_str(&text("F1", 10.0, 335.0, 738.0, 12.0, &["Revision 2"]));
            content.push_str(&text("F1", 10.0, 135.0, 718.0, 12.0, &["Consent"]));
            content.push_str(&text(
                "F1",
                10.0,
                335.0,
                718.0,
                12.0,
                &[&format!("Page {n} of 3")],
            ));
            content.push_str(&text("F1", 11.0, 72.0, 680.0, 14.0, &lines));
            Page::new(content)
        })
        .collect();
    synth::document_in(
        &mut pdf,
        &pages,
        &format!("{COURIER} /XObject << /Im1 {image} 0 R >>"),
        "",
        |_| String::new(),
    )
}

#[test]
fn a_repeated_title_block_and_logo_are_the_header_with_page_fields() {
    let reflowed = reflow_of(running_table_pdf());
    let document = &reflowed.document;
    // The body holds neither the title block nor the logo.
    assert!(
        !document
            .body()
            .iter()
            .any(|b| matches!(b, BlockNode::Table(_))),
        "the title block left the body"
    );
    assert_eq!(graphics(document), 0);
    // The default header holds both, the page number and count as fields.
    let headers = &document.definitions().headers;
    assert_eq!(headers.len(), 1, "one header part");
    let header = headers.iter().next().map(|(_, h)| h).expect("a header");
    let table = header
        .blocks
        .iter()
        .find_map(|b| match b {
            BlockNode::Table(t) => Some(t),
            _ => None,
        })
        .expect("the title block is in the header");
    let mut kinds = Vec::new();
    for cell in table.rows.iter().flat_map(|r| r.cells.iter()) {
        for block in &cell.blocks {
            if let BlockNode::Paragraph(p) = block {
                for inline in &p.inlines {
                    if let InlineNode::Field(f) = inline {
                        kinds.push(f.kind.clone());
                    }
                }
            }
        }
    }
    assert_eq!(kinds, vec![FieldKind::Page, FieldKind::NumPages]);
    let logo = header.blocks.iter().any(|b| match b {
        BlockNode::Paragraph(p) => p
            .inlines
            .iter()
            .any(|i| matches!(i, InlineNode::AnchoredDrawing(_))),
        _ => false,
    });
    assert!(logo, "the logo is in the header");
}

/// `pages` pages, each a picture the size of the page (a letterhead) drawn
/// first, with prose over it; the last page's picture is `last` instead.
fn letterhead_pdf(pages: usize) -> Vec<u8> {
    let mut pdf = synth::Pdf::default();
    let image = pdf.stream(
        "/Type /XObject /Subtype /Image /Width 4 /Height 4 /ColorSpace /DeviceGray /BitsPerComponent 8",
        &[200u8; 16],
    );
    let prose = full("prose");
    let lines: Vec<&str> = std::iter::repeat_n(prose.as_str(), 20).collect();
    let content = format!(
        "q 612 0 0 792 0 0 cm /Im1 Do Q\n{}",
        text("F1", 11.0, 72.0, 640.0, 14.0, &lines)
    );
    let all: Vec<Page> = (0..pages).map(|_| Page::new(content.clone())).collect();
    synth::document_in(
        &mut pdf,
        &all,
        &format!("{COURIER} /XObject << /Im1 {image} 0 R >>"),
        "",
        |_| String::new(),
    )
}

fn anchored(inlines: &[InlineNode]) -> Vec<&casual_doc_model::v1::AnchoredDrawing> {
    inlines
        .iter()
        .filter_map(|i| match i {
            InlineNode::AnchoredDrawing(d) => Some(d.as_ref()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_page_sized_picture_under_text_is_a_background_not_an_inline_picture() {
    // One page: the picture is behind the text, page-anchored where the PDF
    // drew it, and pushes nothing (no inline picture in the body).
    let reflowed = reflow_of(letterhead_pdf(1));
    let document = &reflowed.document;
    assert!(
        !document.body().iter().any(|b| matches!(
            b,
            BlockNode::Paragraph(p) if p.inlines.iter().any(|i| matches!(i, InlineNode::Drawing(_)))
        )),
        "no inline picture"
    );
    let backgrounds: Vec<_> = paragraphs(document)
        .into_iter()
        .flat_map(|p| anchored(&p.inlines))
        .collect();
    assert_eq!(backgrounds.len(), 1);
    let anchor = &backgrounds[0].anchor;
    assert!(anchor.behind_doc);
    assert_eq!(anchor.horizontal.relative_from, HorizontalAnchor::Page);
    assert_eq!(anchor.vertical.relative_from, VerticalAnchor::Page);
}

#[test]
fn a_background_on_every_page_is_the_headers_letterhead() {
    // Three pages with the same background: one copy, in the header, behind
    // the text of every page; none left in the body.
    let reflowed = reflow_of(letterhead_pdf(3));
    let document = &reflowed.document;
    let in_body: usize = paragraphs(document)
        .into_iter()
        .map(|p| anchored(&p.inlines).len())
        .sum();
    assert_eq!(in_body, 0);
    let headers = &document.definitions().headers;
    let behind: Vec<bool> = headers
        .iter()
        .flat_map(|(_, h)| h.blocks.iter())
        .filter_map(|b| match b {
            BlockNode::Paragraph(p) => Some(anchored(&p.inlines)),
            _ => None,
        })
        .flatten()
        .map(|d| d.anchor.behind_doc)
        .collect();
    assert_eq!(behind, vec![true], "{headers:?}");
}

#[test]
fn the_body_clears_a_letterheads_logo_band() {
    // Three pages, each a page-sized letterhead whose top tenth is a dark
    // logo band (a 4 x 40 grey picture, its top four rows black), with prose
    // over it starting at 72 pt, inside the band.
    let mut pdf = synth::Pdf::default();
    let mut samples = vec![255u8; 4 * 40];
    samples[..16].fill(0);
    let image = pdf.stream(
        "/Type /XObject /Subtype /Image /Width 4 /Height 40 /ColorSpace /DeviceGray /BitsPerComponent 8",
        &samples,
    );
    let prose = full("prose");
    let lines: Vec<&str> = std::iter::repeat_n(prose.as_str(), 20).collect();
    let content = format!(
        "q 612 0 0 792 0 0 cm /Im1 Do Q\n{}",
        text("F1", 11.0, 72.0, 712.0, 14.0, &lines)
    );
    let pages: Vec<Page> = (0..3).map(|_| Page::new(content.clone())).collect();
    let bytes = synth::document_in(
        &mut pdf,
        &pages,
        &format!("{COURIER} /XObject << /Im1 {image} 0 R >>"),
        "",
        |_| String::new(),
    );
    let reflowed = reflow_of(bytes);
    let section = reflowed
        .document
        .definitions()
        .sections
        .first()
        .expect("a section");
    // The band ends at 79.2 pt; the body starts below it.
    let top = f64::from(section.page_margins.top_twips) / 20.0;
    assert!((79.0..=90.0).contains(&top), "{top}");
}
