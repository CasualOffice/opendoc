//! Reflow layout and mapping families found by comparing reflowed pages with
//! their originals (`docs/182` §18.11): emphasis inside a paragraph, bevelled
//! table borders, the measured vertical rhythm, pictures larger than the
//! column, synthesised italics, centred headings and ruled row heights. Each
//! fixture is synthetic (`synth.rs`); each guard's mutation is in the commit
//! that added it.

use casual_doc_model::v1::{
    Alignment, BlockNode, Document, HeightRule, InlineNode, Paragraph, Spacing,
};

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
