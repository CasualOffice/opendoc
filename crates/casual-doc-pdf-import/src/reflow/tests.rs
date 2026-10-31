//! Reflow, per construct family, on rights-safe synthetic PDFs (`synth.rs`),
//! plus the tagged reading order, a loss-report fixture and the doubling
//! guards (SKILL §4, §8). Each guard's mutation and red output is recorded in
//! `docs/182` §18.

use casual_doc_model::v1::{
    BlockNode, Document, FieldKind, HyperlinkTarget, InlineNode, NumberFormat, Paragraph,
};
// Own line: floating pictures and vector art.
use casual_doc_model::v1::{GroupChild, HorizontalAnchor, VerticalAnchor, WrapMode};

use crate::report::{self, PdfModelOutcome};
use crate::synth::{self, PROSE, Page, line, text};
use crate::{PdfImport, PdfReadLimits, Reflowed, import_pdf};

fn reflow_of(bytes: Vec<u8>) -> (PdfImport, Reflowed) {
    let imported = import_pdf(bytes, PdfReadLimits::default()).expect("imports");
    let reflowed = imported.reflow().expect("reflows");
    reflowed.document.validate().expect("valid");
    (imported, reflowed)
}

fn inline_text(inlines: &[InlineNode], out: &mut String) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => out.push_str(&run.text),
            InlineNode::Tab(_) => out.push('\t'),
            InlineNode::Hyperlink(h) => inline_text(&h.inlines, out),
            InlineNode::Field(f) => {
                out.push('{');
                inline_text(&f.inlines, out);
                out.push('}');
            }
            InlineNode::NoteReference(_) => out.push_str("[note]"),
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
            BlockNode::Paragraph(p) => Some(p),
            _ => None,
        })
        .collect()
}

fn texts(document: &Document) -> Vec<String> {
    paragraphs(document)
        .into_iter()
        .map(para_text)
        .filter(|t| !t.is_empty())
        .collect()
}

fn style_name(document: &Document, p: &Paragraph) -> Option<String> {
    p.properties
        .style_ref
        .and_then(|id| document.definitions().styles.get(&id))
        .and_then(|s| s.name.clone())
}

// --- Headings, paragraphs, de-hyphenation ----------------------------------

fn headings_pdf() -> Vec<u8> {
    let body1 = [
        "The quick brown fox jumps over the lazy dog while the compre-",
        "hension of the committee grows with every quarterly figure it",
        "reads, because every sentence in this synthetic corpus is made",
        "here and regenerates on any machine.",
    ];
    let body2 = [
        "A second section opens with its own heading and two lines of",
        "body text that the grouping must keep as one paragraph.",
    ];
    let first = format!(
        "{}{}{}{}{}",
        text("F3", 20.0, 72.0, 720.0, 24.0, &["Annual Synthetic Review"]),
        text("F3", 14.0, 72.0, 680.0, 18.0, &["Background"]),
        text("F2", 11.0, 72.0, 660.0, 14.0, &body1),
        text("F3", 14.0, 72.0, 590.0, 18.0, &["Method"]),
        text("F2", 11.0, 72.0, 570.0, 14.0, &body2),
    );
    let second = format!(
        "{}{}",
        text("F3", 14.0, 72.0, 720.0, 18.0, &["Results"]),
        text("F2", 11.0, 72.0, 700.0, 14.0, &PROSE[4..8]),
    );
    synth::document(&[Page::new(first), Page::new(second)], "", "")
}

#[test]
fn headings_become_styles_and_lines_become_paragraphs() {
    let (_, reflowed) = reflow_of(headings_pdf());
    let document = &reflowed.document;
    let paras = paragraphs(document);
    let named: Vec<(Option<String>, String)> = paras
        .iter()
        .map(|p| (style_name(document, p), para_text(p)))
        .filter(|(_, t)| !t.is_empty())
        .collect();
    // The one 20 pt line over 14 pt headings is the Title; the headings
    // are the first level under it (no level is skipped).
    assert_eq!(
        named[0],
        (Some("Title".into()), "Annual Synthetic Review".into())
    );
    assert_eq!(named[1], (Some("heading 1".into()), "Background".into()));
    // Four PDF lines are one paragraph; the line-end hyphen is removed.
    assert_eq!(named[2].0, None);
    assert!(
        named[2].1.starts_with(
            "The quick brown fox jumps over the lazy dog while the comprehension of the committee"
        ),
        "{:?}",
        named[2].1
    );
    assert!(named[2].1.ends_with("regenerates on any machine."));
    assert_eq!(named[3], (Some("heading 1".into()), "Method".into()));
    assert_eq!(named[5], (Some("heading 1".into()), "Results".into()));
    // Normal carries the body font, and the heading styles derive from it.
    let styles = &document.definitions().styles;
    let normal = styles
        .iter()
        .find(|(_, s)| s.is_default)
        .map(|(_, s)| s)
        .unwrap();
    assert_eq!(normal.run.as_ref().unwrap().size_half_points, Some(22));
    for feature in [
        report::STYLE_INFERRED,
        report::HEADING_INFERRED,
        report::PARAGRAPH_BOUNDARY_INFERRED,
        report::DEHYPHENATION_INFERRED,
    ] {
        assert!(reflowed.report.has(feature), "{feature}");
    }
    assert!(!reflowed.report.has(report::LIST_INFERRED));
}

// --- Lists -------------------------------------------------------------------

fn lists_pdf() -> Vec<u8> {
    // Markers sit an indent tab from their text (18 pt), so the grouping sees
    // them as lines of their own; the second bullet wraps with a hanging
    // indent.
    let mut content = text(
        "F2",
        11.0,
        72.0,
        720.0,
        14.0,
        &["Steps to follow before the review:"],
    );
    let items: [(&str, &[&str]); 3] = [
        ("\\225", &["Collect the synthetic figures."]),
        (
            "\\225",
            &[
                "Check every number against the generator, line by line, and",
                "record each difference.",
            ],
        ),
        ("\\225", &["File the report."]),
    ];
    let mut y = 700.0;
    for (marker, lines) in items {
        content.push_str(&format!("BT /F2 11 Tf 90 {y} Td ({marker}) Tj ET\n"));
        content.push_str(&text("F2", 11.0, 108.0, y, 14.0, lines));
        y -= 14.0 * lines.len() as f64 + 4.0;
    }
    content.push_str(&text(
        "F2",
        11.0,
        72.0,
        y - 10.0,
        14.0,
        &["Then number the outcomes:"],
    ));
    y -= 30.0;
    for (n, item) in ["First outcome.", "Second outcome.", "Third outcome."]
        .iter()
        .enumerate()
    {
        content.push_str(&format!("BT /F2 11 Tf 90 {y} Td ({}.) Tj ET\n", n + 1));
        content.push_str(&text("F2", 11.0, 108.0, y, 14.0, &[item]));
        y -= 18.0;
    }
    synth::document(&[Page::new(content)], "", "")
}

#[test]
fn markers_with_a_hanging_indent_become_numbering() {
    let (_, reflowed) = reflow_of(lists_pdf());
    let document = &reflowed.document;
    let paras = paragraphs(document);
    let listed: Vec<(&Paragraph, String)> = paras
        .iter()
        .filter(|p| p.properties.numbering.is_some())
        .map(|p| (*p, para_text(p)))
        .collect();
    assert_eq!(listed.len(), 6, "{:?}", texts(document));
    // The marker is the numbering's now, not text.
    assert_eq!(listed[0].1, "Collect the synthetic figures.");
    assert_eq!(
        listed[1].1,
        "Check every number against the generator, line by line, and record each difference."
    );
    assert_eq!(listed[3].1, "First outcome.");
    let definitions = document.definitions();
    let format_of = |p: &Paragraph| {
        let numbering = p.properties.numbering.unwrap();
        let instance = definitions.numbering.get(&numbering.instance).unwrap();
        let abs = definitions
            .abstract_numbering
            .get(&instance.abstract_ref)
            .unwrap();
        let level = &abs.levels[usize::from(numbering.level)];
        (level.num_fmt.clone(), level.lvl_text.clone(), level.start)
    };
    assert_eq!(
        format_of(listed[0].0),
        (Some(NumberFormat::Bullet), Some("\u{2022}".into()), 1)
    );
    assert_eq!(
        format_of(listed[3].0),
        (Some(NumberFormat::Decimal), Some("%1.".into()), 1)
    );
    assert_ne!(
        listed[0].0.properties.numbering.unwrap().instance,
        listed[3].0.properties.numbering.unwrap().instance,
        "two lists"
    );
    // The hanging indent is the geometry's: text at 108 pt, marker at 90 pt,
    // column at 72 pt.
    let numbering = listed[0].0.properties.numbering.unwrap();
    let instance = definitions.numbering.get(&numbering.instance).unwrap();
    let abs = definitions
        .abstract_numbering
        .get(&instance.abstract_ref)
        .unwrap();
    let indentation = abs.levels[0]
        .paragraph_properties
        .as_ref()
        .unwrap()
        .indentation
        .unwrap();
    assert_eq!(indentation.start_twips, Some(720));
    assert_eq!(indentation.hanging_twips, Some(360));
    assert!(reflowed.report.has(report::LIST_INFERRED));
}

#[test]
fn a_lone_numbered_line_is_not_a_list() {
    let content = format!(
        "{}{}",
        text("F2", 11.0, 72.0, 720.0, 14.0, &["Intro text for the page."]),
        text(
            "F2",
            11.0,
            72.0,
            690.0,
            14.0,
            &["1. A lone numbered paragraph stays a paragraph."]
        ),
    );
    let (_, reflowed) = reflow_of(synth::document(&[Page::new(content)], "", ""));
    assert!(
        paragraphs(&reflowed.document)
            .iter()
            .all(|p| p.properties.numbering.is_none())
    );
    assert!(
        texts(&reflowed.document)
            .iter()
            .any(|t| t.starts_with("1. A lone"))
    );
}

// --- Tables ------------------------------------------------------------------

fn tables_pdf() -> Vec<u8> {
    let xs = [72.0, 200.0, 330.0, 460.0];
    let ys = [600.0, 580.0, 560.0, 540.0];
    let mut content = text("F2", 11.0, 72.0, 640.0, 14.0, &["A ruled table follows."]);
    for y in ys {
        content.push_str(&line(xs[0], y, xs[3], y));
    }
    for x in xs {
        content.push_str(&line(x, ys[0], x, ys[3]));
    }
    for (r, y) in ys.windows(2).enumerate() {
        for (c, x) in xs.windows(2).enumerate() {
            content.push_str(&text(
                "F2",
                10.0,
                x[0] + 4.0,
                y[1] + 6.0,
                12.0,
                &[&format!("R{}C{}", r + 1, c + 1)],
            ));
        }
    }
    // A whitespace table: three columns of short entries on shared baselines.
    let rows = [
        ["Item", "Qty", "Price"],
        ["Apples", "3", "1.20"],
        ["Pears", "12", "0.80"],
        ["Plums", "7", "2.05"],
    ];
    for (r, row) in rows.iter().enumerate() {
        let y = 480.0 - 14.0 * r as f64;
        for (c, cell) in row.iter().enumerate() {
            content.push_str(&text("F2", 10.0, [72.0, 220.0, 380.0][c], y, 12.0, &[cell]));
        }
    }
    synth::document(&[Page::new(content)], "", "")
}

fn cell_texts(document: &Document, which: usize) -> Vec<Vec<String>> {
    let tables: Vec<_> = document
        .body()
        .iter()
        .filter_map(|b| match b {
            BlockNode::Table(t) => Some(t),
            _ => None,
        })
        .collect();
    tables[which]
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| {
                    cell.blocks
                        .iter()
                        .filter_map(|b| match b {
                            BlockNode::Paragraph(p) => Some(para_text(p)),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("/")
                })
                .collect()
        })
        .collect()
}

#[test]
fn ruling_grids_and_aligned_columns_become_tables() {
    let (_, reflowed) = reflow_of(tables_pdf());
    let document = &reflowed.document;
    assert_eq!(
        cell_texts(document, 0),
        vec![
            vec!["R1C1", "R1C2", "R1C3"],
            vec!["R2C1", "R2C2", "R2C3"],
            vec!["R3C1", "R3C2", "R3C3"],
        ]
    );
    assert_eq!(
        cell_texts(document, 1),
        vec![
            vec!["Item", "Qty", "Price"],
            vec!["Apples", "3", "1.20"],
            vec!["Pears", "12", "0.80"],
            vec!["Plums", "7", "2.05"],
        ]
    );
    // Nothing of either table is left in the body text.
    let body = texts(document).join("\n");
    assert!(!body.contains("R2C2") && !body.contains("Apples"), "{body}");
    assert_eq!(
        reflowed
            .report
            .get(report::TABLE_INFERRED)
            .unwrap()
            .occurrences,
        2
    );
}

#[test]
fn two_column_prose_on_a_shared_grid_is_not_a_table() {
    let (_, reflowed) = reflow_of(columns_pdf());
    assert!(
        reflowed
            .document
            .body()
            .iter()
            .all(|b| !matches!(b, BlockNode::Table(_)))
    );
    assert!(!reflowed.report.has(report::TABLE_INFERRED));
}

// --- Header and footer -------------------------------------------------------

fn furniture_pdf() -> Vec<u8> {
    let pages: Vec<Page> = (1..=3)
        .map(|n| {
            Page::new(format!(
                "{}{}{}",
                text(
                    "F1",
                    9.0,
                    72.0,
                    750.0,
                    11.0,
                    &["Synthetic Quarterly Report"]
                ),
                text(
                    "F2",
                    11.0,
                    72.0,
                    700.0,
                    14.0,
                    &PROSE[(n - 1) * 2..(n - 1) * 2 + 2]
                ),
                text("F1", 9.0, 290.0, 30.0, 11.0, &[&format!("Page {n}")]),
            ))
        })
        .collect();
    synth::document(&pages, "", "")
}

#[test]
fn repeated_top_and_bottom_lines_become_header_and_footer_parts() {
    let (_, reflowed) = reflow_of(furniture_pdf());
    let document = &reflowed.document;
    let body = texts(document).join("\n");
    assert!(!body.contains("Synthetic Quarterly Report"), "{body}");
    assert!(!body.contains("Page "), "{body}");
    let definitions = document.definitions();
    let section = &definitions.sections[0];
    let header = definitions
        .headers
        .get(&section.headers[0].reference)
        .unwrap();
    let footer = definitions
        .footers
        .get(&section.footers[0].reference)
        .unwrap();
    let part_text = |blocks: &[BlockNode]| {
        blocks
            .iter()
            .filter_map(|b| match b {
                BlockNode::Paragraph(p) => Some(para_text(p)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        part_text(&header.blocks),
        vec!["Synthetic Quarterly Report"]
    );
    assert_eq!(
        part_text(&footer.blocks),
        vec!["Page {1}"],
        "the number is a PAGE field"
    );
    let BlockNode::Paragraph(p) = &footer.blocks[0] else {
        panic!()
    };
    assert!(
        p.inlines
            .iter()
            .any(|i| matches!(i, InlineNode::Field(f) if f.kind == FieldKind::Page))
    );
    assert!(reflowed.report.has(report::HEADER_FOOTER_INFERRED));
}

#[test]
fn a_one_page_document_has_no_repetition_to_infer_furniture_from() {
    let content = format!(
        "{}{}",
        text(
            "F1",
            9.0,
            72.0,
            750.0,
            11.0,
            &["Not a header, only a top line"]
        ),
        text("F2", 11.0, 72.0, 700.0, 14.0, &PROSE[..2]),
    );
    let (_, reflowed) = reflow_of(synth::document(&[Page::new(content)], "", ""));
    assert!(texts(&reflowed.document)[0].starts_with("Not a header"));
    assert!(reflowed.document.definitions().headers.is_empty());
}

// --- Footnotes ---------------------------------------------------------------

fn footnotes_pdf() -> Vec<u8> {
    let content = format!(
        "BT /F2 11 Tf 72 700 Td (The committee approved the plan) Tj /F2 7 Tf 3 Ts (1) Tj 0 Ts /F2 11 Tf ( and then reviewed) Tj ET\n\
         BT /F2 11 Tf 72 686 Td (the figures that the generator wrote) Tj /F2 7 Tf 3 Ts (2) Tj 0 Ts /F2 11 Tf (.) Tj ET\n\
         {}",
        text(
            "F2",
            8.0,
            72.0,
            90.0,
            10.0,
            &["1 The plan is synthetic.", "2 So is the committee."]
        ),
    );
    synth::document(&[Page::new(content)], "", "")
}

#[test]
fn superscripts_matched_to_bottom_blocks_become_footnotes() {
    let (_, reflowed) = reflow_of(footnotes_pdf());
    let document = &reflowed.document;
    let body = texts(document);
    assert_eq!(body.len(), 1, "{body:?}");
    assert_eq!(
        body[0],
        "The committee approved the plan[note] and then reviewed the figures that the generator wrote[note]."
    );
    let notes: Vec<String> = document
        .definitions()
        .footnotes
        .iter()
        .map(|(_, note)| match &note.blocks[0] {
            BlockNode::Paragraph(p) => para_text(p).trim().to_owned(),
            _ => String::new(),
        })
        .collect();
    assert_eq!(
        notes,
        vec!["The plan is synthetic.", "So is the committee."]
    );
    assert_eq!(
        reflowed
            .report
            .get(report::FOOTNOTE_INFERRED)
            .unwrap()
            .occurrences,
        2
    );
}

#[test]
fn a_numbered_line_at_the_bottom_with_no_reference_stays_body_text() {
    let content = format!(
        "{}{}",
        text("F2", 11.0, 72.0, 700.0, 14.0, &PROSE[..2]),
        text(
            "F2",
            8.0,
            72.0,
            90.0,
            10.0,
            &["1 Not a note: nothing refers to it."]
        ),
    );
    let (_, reflowed) = reflow_of(synth::document(&[Page::new(content)], "", ""));
    assert!(reflowed.document.definitions().footnotes.is_empty());
    assert!(
        texts(&reflowed.document)
            .iter()
            .any(|t| t.starts_with("1 Not a note"))
    );
}

// --- Columns -----------------------------------------------------------------

/// Monospaced columns of equal lines, so "ran to the column's end" is exact.
fn columns_pdf() -> Vec<u8> {
    let left = [
        "The committee met on a bright",
        "morning to review the figures",
        "that the synthetic corpus had",
        "produced over the last quarter",
    ];
    let right = [
        "and agreed that every number ",
        "was written here.",
        "A new paragraph starts here at",
        "the top of the second column.",
    ];
    let content = format!(
        "{}{}{}",
        text(
            "F3",
            18.0,
            72.0,
            730.0,
            22.0,
            &["A Two Column Synthetic Page"]
        ),
        text("F6", 9.0, 72.0, 700.0, 11.0, &left),
        text("F6", 9.0, 320.0, 700.0, 11.0, &right),
    );
    synth::document(&[Page::new(content)], "", "")
}

#[test]
fn column_bands_become_section_columns_and_read_column_by_column() {
    let (_, reflowed) = reflow_of(columns_pdf());
    let document = &reflowed.document;
    let sections = &document.definitions().sections;
    assert_eq!(sections.len(), 2, "the title's section, then the columns'");
    assert_eq!(sections[0].columns.count, 1);
    assert_eq!(sections[1].columns.count, 2);
    let body = texts(document);
    assert_eq!(body[0], "A Two Column Synthetic Page");
    // The left column's run-on sentence continues at the top of the right
    // column: one paragraph across the gutter.
    assert_eq!(
        body[1],
        "The committee met on a bright morning to review the figures that the synthetic corpus had produced over the last quarter and agreed that every number was written here."
    );
    assert!(body[2].starts_with("A new paragraph starts here"));
    assert!(reflowed.report.has(report::COLUMNS_INFERRED));
    assert!(reflowed.report.has(report::READING_ORDER_INFERRED));
}

// --- Tagged reading order ----------------------------------------------------

/// The content stream paints the bottom paragraph first and the top one
/// last; the structure tree reads the bottom one first. Geometry and paint
/// order agree with each other and disagree with the tree, so only the tree
/// can produce the asserted order. The H1 is set in the body font, so only
/// its role can make it a heading.
fn tagged_pdf() -> Vec<u8> {
    let mut pdf = synth::Pdf::default();
    let root = pdf.reserve();
    let content = format!(
        "/P <</MCID 0>> BDC\n{}EMC\n/H1 <</MCID 1>> BDC\n{}EMC\n/P <</MCID 2>> BDC\n{}EMC\n/Artifact BMC\n{}EMC\n",
        text(
            "F2",
            11.0,
            72.0,
            300.0,
            14.0,
            &["Read first: the tree puts this bottom paragraph first."]
        ),
        text(
            "F2",
            11.0,
            72.0,
            700.0,
            14.0,
            &["Tagged heading in the body font"]
        ),
        text(
            "F2",
            11.0,
            72.0,
            660.0,
            14.0,
            &["Read last: the top paragraph comes after it."]
        ),
        text("F1", 8.0, 290.0, 30.0, 10.0, &["1"]),
    );
    let page_ref = "{page0}";
    let bottom = pdf.add(format!(
        "<< /Type /StructElem /S /P /P {root} 0 R /Pg {page_ref} /K 0 >>"
    ));
    let heading = pdf.add(format!(
        "<< /Type /StructElem /S /Heading1 /P {root} 0 R /Pg {page_ref} /K 1 >>"
    ));
    let top = pdf.add(format!(
        "<< /Type /StructElem /S /P /P {root} 0 R /Pg {page_ref} /K [2] >>"
    ));
    pdf.set(
        root,
        format!("<< /Type /StructTreeRoot /RoleMap << /Heading1 /H1 >> /K [{bottom} 0 R {heading} 0 R {top} 0 R] >>"),
    );
    synth::document_in(
        &mut pdf,
        &[Page::new(content)],
        "",
        &format!("/MarkInfo << /Marked true >> /StructTreeRoot {root} 0 R"),
        |_| "/StructParents 0".to_owned(),
    )
}

#[test]
fn a_tagged_pdf_reflows_in_its_structure_tree_order() {
    let (imported, reflowed) = reflow_of(tagged_pdf());
    let tree = imported.source.structure.as_ref().expect("tagged");
    assert_eq!(
        tree.leaves
            .iter()
            .map(|l| (l.mcid, l.role().to_owned()))
            .collect::<Vec<_>>(),
        vec![
            (0, "P".to_owned()),
            (1, "H1".to_owned()),
            (2, "P".to_owned())
        ],
        "RoleMap applied"
    );
    let document = &reflowed.document;
    let paras: Vec<(Option<String>, String)> = paragraphs(document)
        .iter()
        .map(|p| (style_name(document, p), para_text(p)))
        .filter(|(_, t)| !t.is_empty())
        .collect();
    assert_eq!(
        paras,
        vec![
            (
                None,
                "Read first: the tree puts this bottom paragraph first.".to_owned()
            ),
            (
                Some("heading 1".to_owned()),
                "Tagged heading in the body font".to_owned()
            ),
            (
                None,
                "Read last: the top paragraph comes after it.".to_owned()
            ),
        ]
    );
    assert!(
        !reflowed.report.has(report::READING_ORDER_INFERRED),
        "the order was not inferred"
    );
    assert!(!reflowed.report.has(report::TAG_STRUCTURE_IGNORED));
    // The artifact page number in the bottom band is a footer, even on one page.
    assert!(reflowed.report.has(report::HEADER_FOOTER_INFERRED));
    assert_eq!(reflowed.document.definitions().footers.len(), 1);
}

#[test]
fn the_p0_tagged_fixture_reflows_heading_then_paragraph_without_its_artifact() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/pdf/synthetic-tagged.pdf"),
    )
    .unwrap();
    let (_, reflowed) = reflow_of(bytes);
    let body = texts(&reflowed.document);
    assert_eq!(body[0], "Tagged heading");
    assert!(body[1].starts_with("The quick brown fox"));
    assert_eq!(
        body.len(),
        2,
        "the artifact page number is not body text: {body:?}"
    );
}

// --- Links and comments ------------------------------------------------------

fn links_pdf() -> Vec<u8> {
    let first = Page {
        content: format!(
            "{}{}",
            text("F2", 11.0, 72.0, 700.0, 14.0, &["Visit the project site for details."]),
            text("F2", 11.0, 72.0, 660.0, 14.0, &["See the appendix on the next page."]),
        ),
        annots: vec![
            // "project site": x from about 108 to 167 at Times 11.
            "<< /Type /Annot /Subtype /Link /Rect [111 696 161.5 710] /A << /S /URI /URI (https://example.org/synthetic) >> >>".to_owned(),
            "<< /Type /Annot /Subtype /Link /Rect [72 656 300 670] /Dest [{page1} /XYZ 72 720 0] >>".to_owned(),
            "<< /Type /Annot /Subtype /Text /Rect [300 700 320 720] /T (Reviewer A) /M (D:20261029120000Z) /Contents (Check this figure.) >>".to_owned(),
            "<< /Type /Annot /Subtype /Highlight /Rect [72 690 200 710] >>".to_owned(),
            "<< /Type /Annot /Subtype /Link /Rect [72 600 200 620] /A << /S /URI /URI (javascript:alert\\(1\\)) >> >>".to_owned(),
        ],
        extra: String::new(),
    };
    let second = Page::new(format!(
        "{}{}",
        text("F3", 14.0, 72.0, 720.0, 18.0, &["Appendix"]),
        text(
            "F2",
            11.0,
            72.0,
            700.0,
            14.0,
            &["The appendix holds the synthetic figures."]
        ),
    ));
    synth::document(&[first, second], "", "")
}

fn hyperlinks(inlines: &[InlineNode]) -> Vec<(HyperlinkTarget, String)> {
    inlines
        .iter()
        .filter_map(|i| match i {
            InlineNode::Hyperlink(h) => {
                let mut t = String::new();
                inline_text(&h.inlines, &mut t);
                Some((h.target.clone(), t))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn links_become_hyperlinks_and_notes_become_comments_in_both_outputs() {
    let (imported, reflowed) = reflow_of(links_pdf());
    // Fixed layout: inside the frames.
    let frames: Vec<&Paragraph> = imported
        .document
        .body()
        .iter()
        .filter_map(|b| match b {
            BlockNode::Paragraph(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| match i {
            InlineNode::TextBox(t) => match &t.blocks[0] {
                BlockNode::Paragraph(p) => Some(p),
                _ => None,
            },
            _ => None,
        })
        .collect();
    for (label, paragraphs, definitions) in [
        ("fixed", frames, imported.document.definitions()),
        (
            "reflow",
            paragraphs(&reflowed.document),
            reflowed.document.definitions(),
        ),
    ] {
        let links: Vec<(HyperlinkTarget, String)> = paragraphs
            .iter()
            .flat_map(|p| hyperlinks(&p.inlines))
            .collect();
        assert!(
            links.iter().any(|(t, text)| matches!(t, HyperlinkTarget::External(e) if e.url == "https://example.org/synthetic")
                && text.trim() == "project site"),
            "{label}: {links:?}"
        );
        let internal = links
            .iter()
            .find_map(|(t, _)| match t {
                HyperlinkTarget::Internal(i) => Some(i.anchor.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{label}: an internal link: {links:?}"));
        assert!(
            definitions
                .bookmarks
                .iter()
                .any(|(_, b)| b.name == internal),
            "{label}: the bookmark the link names exists"
        );
        assert!(!links.iter().any(
            |(t, _)| matches!(t, HyperlinkTarget::External(e) if e.url.starts_with("javascript"))
        ));
        let comments: Vec<_> = definitions.comments.iter().map(|(_, c)| c).collect();
        assert_eq!(comments.len(), 1, "{label}");
        assert_eq!(comments[0].author.as_deref(), Some("Reviewer A"));
        assert_eq!(comments[0].date.as_deref(), Some("2026-10-29T12:00:00Z"));
        let BlockNode::Paragraph(body) = &comments[0].blocks[0] else {
            panic!()
        };
        assert_eq!(para_text(body), "Check this figure.");
    }
    // The bookmark is on page two's content, in both outputs.
    let fixed_bookmarked = imported
        .document
        .body()
        .iter()
        .filter_map(|b| match b {
            BlockNode::Paragraph(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.inlines.iter())
        .find_map(|i| match i {
            InlineNode::TextBox(t) => match &t.blocks[0] {
                BlockNode::Paragraph(p)
                    if p.inlines
                        .iter()
                        .any(|i| matches!(i, InlineNode::BookmarkStart(_))) =>
                {
                    Some(para_text(p))
                }
                _ => None,
            },
            _ => None,
        });
    assert_eq!(fixed_bookmarked.as_deref(), Some("Appendix"));
    let bookmarked = paragraphs(&reflowed.document)
        .into_iter()
        .find(|p| {
            p.inlines
                .iter()
                .any(|i| matches!(i, InlineNode::BookmarkStart(_)))
        })
        .map(para_text);
    assert_eq!(bookmarked.as_deref(), Some("Appendix"));
    for report in [&imported.report, &reflowed.report] {
        for feature in [
            report::HYPERLINK_INFERRED,
            report::BOOKMARK_INFERRED,
            report::COMMENT_INFERRED,
            report::ANNOTATION_OMITTED,
            report::LINK_UNRESOLVED,
        ] {
            assert!(report.has(feature), "{feature}: {:?}", report.findings);
        }
    }
}

// --- The loss report ---------------------------------------------------------

#[test]
fn the_loss_report_names_every_construct_reflow_could_not_keep() {
    let mut content = text("F2", 11.0, 72.0, 720.0, 14.0, &PROSE[..2]);
    // Vector art: a filled diamond (not a rule).
    content.push_str("0.5 g 300 500 m 340 540 l 380 500 l 340 460 l f 0 g\n");
    // Rotated text.
    content.push_str("BT /F1 10 Tf 0 1 -1 0 560 300 Tm (Sideways note) Tj ET\n");
    // An artifact in mid page (a watermark).
    content.push_str(&format!(
        "/Artifact BMC\n{}EMC\n",
        text("F1", 30.0, 150.0, 400.0, 30.0, &["DRAFT"])
    ));
    // Side-by-side blocks on shared baselines, half prose: below the table
    // threshold, so paragraphs and a finding.
    let a = [
        "Short",
        "Another long sentence with plenty of words in it",
        "Brief",
    ];
    let b = [
        "Value",
        "Yet another long sentence with plenty of words",
        "Note",
    ];
    content.push_str(&text("F2", 10.0, 72.0, 300.0, 13.0, &a));
    content.push_str(&text("F2", 10.0, 330.0, 300.0, 13.0, &b));
    let page = Page {
        content,
        annots: vec![
            "<< /Type /Annot /Subtype /Highlight /Rect [72 715 200 730] >>".to_owned(),
            "<< /Type /Annot /Subtype /Link /Rect [72 715 200 730] /A << /S /URI /URI (javascript:void\\(0\\)) >> >>".to_owned(),
        ],
        extra: String::new(),
    };
    let (_, reflowed) = reflow_of(synth::document(&[page], "", ""));
    let losses: Vec<&str> = reflowed
        .report
        .findings
        .iter()
        .filter(|f| f.outcome != PdfModelOutcome::Mapped)
        .map(|f| f.feature)
        .collect();
    assert_eq!(
        losses,
        vec![
            report::ANNOTATION_OMITTED,
            report::ARTIFACT_OMITTED,
            report::LINK_UNRESOLVED,
            report::TABLE_TOPOLOGY_DEGRADED,
            report::TEXT_ROTATED,
        ]
    );
    // The diamond is placed, not omitted.
    assert_eq!(
        reflowed
            .report
            .get(report::VECTOR_PLACED)
            .map(|f| f.occurrences),
        Some(1)
    );
    // Nothing visible was dropped silently: the rotated text and both
    // near-table columns are body text.
    let body = texts(&reflowed.document).join("\n");
    assert!(body.contains("Sideways note"), "{body}");
    assert!(
        body.contains("Another long sentence") && body.contains("Yet another long"),
        "{body}"
    );
    assert!(!body.contains("DRAFT"));
    for finding in &reflowed.report.findings {
        assert!(
            report::FEATURES.contains(&finding.feature),
            "{}",
            finding.feature
        );
    }
}

// --- Determinism and purity ---------------------------------------------------

#[test]
fn reflow_is_deterministic_and_leaves_the_fixed_document_alone() {
    let imported = import_pdf(headings_pdf(), PdfReadLimits::default()).unwrap();
    let before = imported.document.clone();
    let a = imported.reflow().unwrap();
    let b = imported.reflow().unwrap();
    assert_eq!(a.document, b.document);
    assert_eq!(imported.document, before);
}

// --- Complexity (SKILL §8: count, never time) --------------------------------

fn work_of(bytes: Vec<u8>) -> super::ReflowWork {
    reflow_of(bytes).1.work
}

fn ratio(a: u64, b: u64) -> f64 {
    b as f64 / a.max(1) as f64
}

/// Pages of a header, body, footnote and footer: furniture and footnote
/// work must grow linearly with the pages.
fn many_pages(n: usize) -> Vec<u8> {
    let pages: Vec<Page> = (1..=n)
        .map(|p| {
            Page::new(format!(
                "{}BT /F2 11 Tf 72 700 Td (A body line with a marker) Tj /F2 7 Tf 3 Ts (1) Tj 0 Ts ET\n{}{}{}",
                text("F1", 9.0, 72.0, 750.0, 11.0, &["Running header"]),
                text("F2", 11.0, 72.0, 680.0, 14.0, &PROSE[..4]),
                text("F2", 8.0, 72.0, 90.0, 10.0, &["1 A note on every page."]),
                text("F1", 9.0, 290.0, 30.0, 11.0, &[&format!("{p}")]),
            ))
        })
        .collect();
    synth::document(&pages, "", "")
}

#[test]
fn furniture_and_note_work_doubles_when_the_pages_double() {
    let (a, b) = (work_of(many_pages(20)), work_of(many_pages(40)));
    for (name, x, y) in [
        ("furniture", a.furniture, b.furniture),
        ("notes", a.notes, b.notes),
        ("paragraphs", a.paragraphs, b.paragraphs),
        ("order", a.order, b.order),
    ] {
        let r = ratio(x, y);
        assert!((1.0..2.3).contains(&r), "{name}: ratio {r:.2} ({x} -> {y})");
    }
}

/// A page of `n` small ruled grids stacked (so rules and intersections both
/// grow with `n`), and a page of `n` aligned whitespace rows.
fn many_rows(n: usize) -> Vec<u8> {
    let mut grids = String::new();
    let xs = [72.0, 200.0, 330.0];
    let top = 760.0;
    let pitch = 700.0 / n as f64;
    for g in 0..n {
        let (y0, y1) = (top - pitch * g as f64, top - pitch * (g as f64 + 0.6));
        grids.push_str(&line(xs[0], y0, xs[2], y0));
        grids.push_str(&line(xs[0], y1, xs[2], y1));
        for x in xs {
            grids.push_str(&line(x, y0, x, y1));
        }
        let y = y1 + pitch * 0.15;
        grids.push_str(&text("F2", 2.0, 76.0, y, 2.0, &["a"]));
        grids.push_str(&text("F2", 2.0, 204.0, y, 2.0, &["b"]));
    }
    let mut rows = String::new();
    for r in 0..n {
        let y = top - 2.2 * r as f64;
        rows.push_str(&text("F2", 2.0, 72.0, y, 2.0, &["x"]));
        rows.push_str(&text("F2", 2.0, 300.0, y, 2.0, &["y"]));
    }
    synth::document(&[Page::new(grids), Page::new(rows)], "", "")
}

#[test]
fn table_work_doubles_when_the_rows_double() {
    let (small, large) = (reflow_of(many_rows(25)), reflow_of(many_rows(50)));
    let tables = |r: &Reflowed| {
        r.document
            .body()
            .iter()
            .filter(|b| matches!(b, BlockNode::Table(_)))
            .count()
    };
    assert_eq!(
        (tables(&small.1), tables(&large.1)),
        (26, 51),
        "every grid and the whitespace table"
    );
    let r = ratio(small.1.work.tables, large.1.work.tables);
    assert!(
        (1.7..2.4).contains(&r),
        "tables: ratio {r:.2} ({} -> {})",
        small.1.work.tables,
        large.1.work.tables
    );
}

/// One page of `n` body lines in the upper half, each ending in a
/// superscript reference, and the `n` notes in the lower half.
fn many_notes(n: usize) -> Vec<u8> {
    let mut content = String::new();
    let leading = 360.0 / n as f64;
    for k in 1..=n {
        let y = 770.0 - leading * k as f64;
        content.push_str(&format!(
            "BT /F2 1.5 Tf 72 {y} Td (Body line with several words) Tj /F2 0.9 Tf 0.45 Ts ({k}) Tj 0 Ts ET\n"
        ));
    }
    let notes: Vec<String> = (1..=n).map(|k| format!("{k} Note text.")).collect();
    let refs: Vec<&str> = notes.iter().map(String::as_str).collect();
    content.push_str(&text("F2", 1.2, 72.0, 380.0, 320.0 / n as f64, &refs));
    synth::document(&[Page::new(content)], "", "")
}

#[test]
fn footnote_work_doubles_when_the_notes_on_a_page_double() {
    let (small, large) = (reflow_of(many_notes(100)), reflow_of(many_notes(200)));
    assert_eq!(small.1.document.definitions().footnotes.len(), 100);
    assert_eq!(large.1.document.definitions().footnotes.len(), 200);
    let r = ratio(small.1.work.notes, large.1.work.notes);
    assert!(
        (1.7..2.4).contains(&r),
        "notes: ratio {r:.2} ({} -> {})",
        small.1.work.notes,
        large.1.work.notes
    );
}

/// One page of `n` bullet items, each marker an indent tab from its text.
fn many_items(n: usize) -> Vec<u8> {
    let mut content = String::new();
    let leading = 700.0 / n as f64;
    for k in 0..n {
        let y = 760.0 - leading * k as f64;
        content.push_str(&format!("BT /F2 2 Tf 90 {y} Td (\\225) Tj ET\n"));
        content.push_str(&text("F2", 2.0, 96.0, y, 2.0, &["An item of the list"]));
    }
    synth::document(&[Page::new(content)], "", "")
}

#[test]
fn marker_and_paragraph_work_doubles_when_the_items_double() {
    let (small, large) = (reflow_of(many_items(150)), reflow_of(many_items(300)));
    let listed = |r: &Reflowed| {
        paragraphs(&r.document)
            .iter()
            .filter(|p| p.properties.numbering.is_some())
            .count()
    };
    assert_eq!((listed(&small.1), listed(&large.1)), (150, 300));
    let r = ratio(small.1.work.paragraphs, large.1.work.paragraphs);
    assert!(
        (1.7..2.4).contains(&r),
        "paragraphs: ratio {r:.2} ({} -> {})",
        small.1.work.paragraphs,
        large.1.work.paragraphs
    );
}

// --- Reply threads and FreeText ----------------------------------------------

fn threads_pdf() -> Vec<u8> {
    let page = Page {
        content: text("F2", 11.0, 72.0, 700.0, 14.0, &["A paragraph the reviewers discussed."]),
        annots: vec![
            "<< /Type /Annot /Subtype /Text /Rect [300 700 320 720] /T (Reviewer A) /M (D:20261029120000Z) /Contents (Is this right?) >>".to_owned(),
            // The reply carries no rectangle of its own: it threads onto, and
            // anchors where, the note it answers.
            "<< /Type /Annot /Subtype /Text /Rect [0 0 0 0] /IRT {annot0} /T (Author B) /M (D:20261029130000Z) /Contents (Yes, checked.) >>".to_owned(),
            "<< /Type /Annot /Subtype /FreeText /Rect [72 600 300 630] /DA (/Helv 14 Tf 1 0 0 rg) /T (Reviewer A) /Contents (Typed on the page) >>".to_owned(),
        ],
        extra: String::new(),
    };
    synth::document(&[page], "", "")
}

#[test]
fn replies_thread_onto_their_note_and_free_text_is_drawn() {
    use casual_doc_model::v1::Color;
    let (imported, reflowed) = reflow_of(threads_pdf());
    for (label, document) in [
        ("fixed", &imported.document),
        ("reflow", &reflowed.document),
    ] {
        let comments: Vec<_> = document
            .definitions()
            .comments
            .iter()
            .map(|(_, c)| c)
            .collect();
        assert_eq!(comments.len(), 3, "{label}");
        let root = comments
            .iter()
            .find(|c| c.author.as_deref() == Some("Reviewer A") && c.parent_para_id.is_none())
            .unwrap();
        let reply = comments
            .iter()
            .find(|c| c.author.as_deref() == Some("Author B"))
            .unwrap();
        assert!(root.para_id.is_some(), "{label}");
        assert_eq!(
            reply.parent_para_id, root.para_id,
            "{label}: the reply threads onto the note"
        );
        assert_eq!(
            comments
                .iter()
                .filter(|c| c.parent_para_id.is_some())
                .count(),
            1,
            "{label}: only the reply is a reply"
        );
    }
    // Fixed layout draws the FreeText's own text in its size and colour.
    let frame = imported
        .document
        .body()
        .iter()
        .filter_map(|b| match b {
            BlockNode::Paragraph(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.inlines.iter())
        .find_map(|i| match i {
            InlineNode::TextBox(t) => match &t.blocks[0] {
                BlockNode::Paragraph(p) if para_text(p) == "Typed on the page" => Some(p.clone()),
                _ => None,
            },
            _ => None,
        })
        .expect("the FreeText's text is a frame on the page");
    let InlineNode::Run(run) = frame
        .inlines
        .iter()
        .find(|i| matches!(i, InlineNode::Run(_)))
        .unwrap()
    else {
        unreachable!()
    };
    assert_eq!(run.properties.size_half_points, Some(28));
    assert!(matches!(run.properties.color, Some(Color::Rgb(c)) if (c.r, c.g, c.b) == (255, 0, 0)));
}

// --- Tagged tables and lists -------------------------------------------------

/// A tagged 2x2 table whose two rows are too few for the whitespace
/// detector, and a tagged list whose nested item sits at the same x as its
/// parent: only the tree can make the table a table and the nested item
/// level 2.
fn tagged_structure_pdf() -> Vec<u8> {
    let mut pdf = synth::Pdf::default();
    let root = pdf.reserve();
    let mut content = String::new();
    let mut mcid = 0;
    let mut mark = |role: &str, text_op: String| {
        let out = format!("/{role} <</MCID {mcid}>> BDC\n{text_op}EMC\n");
        mcid += 1;
        (mcid - 1, out)
    };
    let cell = |x: f64, y: f64, t: &str| text("F2", 10.0, x, y, 12.0, &[t]);
    let mut ids = Vec::new();
    for (x, y, t) in [
        (72.0, 700.0, "Name"),
        (250.0, 700.0, "Qty"),
        (72.0, 686.0, "Apples"),
        (250.0, 686.0, "3"),
    ] {
        let (id, op) = mark("TD", cell(x, y, t));
        content.push_str(&op);
        ids.push(id);
    }
    let items = [
        ("1.", "First item", 640.0),
        ("2.", "Second item", 626.0),
        ("a.", "Nested item", 612.0),
        ("3.", "Third item", 598.0),
    ];
    let mut list_ids = Vec::new();
    for (lbl, body, y) in items {
        let (l, op) = mark("Lbl", cell(72.0, y, lbl));
        content.push_str(&op);
        let (b, op) = mark("LBody", cell(90.0, y, body));
        content.push_str(&op);
        list_ids.push((l, b));
    }
    fn el(pdf: &mut synth::Pdf, root: u32, role: &str, kids: String) -> u32 {
        pdf.add(format!(
            "<< /Type /StructElem /S /{role} /P {root} 0 R /Pg {{page0}} /K {kids} >>"
        ))
    }
    fn li(pdf: &mut synth::Pdf, root: u32, (l, b): (i32, i32), extra: String) -> u32 {
        let lbl = el(pdf, root, "Lbl", l.to_string());
        let body = el(pdf, root, "LBody", b.to_string());
        el(pdf, root, "LI", format!("[{lbl} 0 R {body} 0 R {extra}]"))
    }
    let tds: Vec<u32> = ids
        .iter()
        .map(|m| el(&mut pdf, root, "TD", m.to_string()))
        .collect();
    let tr1 = el(
        &mut pdf,
        root,
        "TR",
        format!("[{} 0 R {} 0 R]", tds[0], tds[1]),
    );
    let tr2 = el(
        &mut pdf,
        root,
        "TR",
        format!("[{} 0 R {} 0 R]", tds[2], tds[3]),
    );
    let table = el(&mut pdf, root, "Table", format!("[{tr1} 0 R {tr2} 0 R]"));
    let li1 = li(&mut pdf, root, list_ids[0], String::new());
    let nested_li = li(&mut pdf, root, list_ids[2], String::new());
    let nested = el(&mut pdf, root, "L", format!("[{nested_li} 0 R]"));
    let li2 = li(&mut pdf, root, list_ids[1], format!("{nested} 0 R"));
    let li3 = li(&mut pdf, root, list_ids[3], String::new());
    let list = el(
        &mut pdf,
        root,
        "L",
        format!("[{li1} 0 R {li2} 0 R {li3} 0 R]"),
    );
    pdf.set(
        root,
        format!("<< /Type /StructTreeRoot /K [{table} 0 R {list} 0 R] >>"),
    );
    synth::document_in(
        &mut pdf,
        &[Page::new(content)],
        "",
        &format!("/MarkInfo << /Marked true >> /StructTreeRoot {root} 0 R"),
        |_| "/StructParents 0".to_owned(),
    )
}

#[test]
fn tagged_tables_and_nested_lists_follow_the_structure_tree() {
    let (_, reflowed) = reflow_of(tagged_structure_pdf());
    let document = &reflowed.document;
    assert_eq!(
        cell_texts(document, 0),
        vec![vec!["Name", "Qty"], vec!["Apples", "3"]]
    );
    let definitions = document.definitions();
    let listed: Vec<(String, u8, Option<NumberFormat>)> = paragraphs(document)
        .iter()
        .filter_map(|p| {
            let n = p.properties.numbering?;
            let instance = definitions.numbering.get(&n.instance)?;
            let abs = definitions.abstract_numbering.get(&instance.abstract_ref)?;
            Some((
                para_text(p),
                n.level,
                abs.levels[usize::from(n.level)].num_fmt.clone(),
            ))
        })
        .collect();
    assert_eq!(
        listed,
        vec![
            ("First item".to_owned(), 0, Some(NumberFormat::Decimal)),
            ("Second item".to_owned(), 0, Some(NumberFormat::Decimal)),
            ("Nested item".to_owned(), 1, Some(NumberFormat::LowerLetter)),
            ("Third item".to_owned(), 0, Some(NumberFormat::Decimal)),
        ]
    );
}

// --- Continued footnotes and symbol markers ----------------------------------

fn continued_notes_pdf() -> Vec<u8> {
    let first = format!(
        "BT /F2 11 Tf 72 700 Td (The committee approved the plan) Tj /F2 7 Tf 3 Ts (*) Tj 0 Ts /F2 11 Tf ( and the budget) Tj /F2 7 Tf 3 Ts (\\206) Tj 0 Ts /F2 11 Tf (.) Tj ET\n{}{}",
        text("F2", 11.0, 72.0, 660.0, 14.0, &PROSE),
        text(
            "F2",
            8.0,
            72.0,
            120.0,
            10.0,
            &["* A short note.", "\u{2020} The second note runs on and"],
        )
    );
    let first = first.replace('\u{2020}', "\\206");
    let second = format!(
        "BT /F2 11 Tf 72 700 Td (Another body line on the second page) Tj /F2 7 Tf 3 Ts (*) Tj 0 Ts ET\n{}{}{}",
        text("F2", 11.0, 72.0, 660.0, 14.0, &PROSE),
        text(
            "F2",
            8.0,
            72.0,
            150.0,
            10.0,
            &["continues here on page two."]
        ),
        text(
            "F2",
            8.0,
            72.0,
            110.0,
            10.0,
            &["* A note of the second page."]
        ),
    );
    synth::document(&[Page::new(first), Page::new(second)], "", "")
}

#[test]
fn a_footnote_continues_onto_the_next_page_and_symbol_markers_are_kept() {
    let (_, reflowed) = reflow_of(continued_notes_pdf());
    let document = &reflowed.document;
    let notes: Vec<String> = document
        .definitions()
        .footnotes
        .iter()
        .map(|(_, note)| match &note.blocks[0] {
            BlockNode::Paragraph(p) => para_text(p).trim().to_owned(),
            _ => String::new(),
        })
        .collect();
    assert_eq!(
        notes,
        vec![
            "A short note.",
            "The second note runs on and continues here on page two.",
            "A note of the second page.",
        ]
    );
    assert!(!texts(document).join("\n").contains("continues here"));
    let props = &document.definitions().sections[0].footnote_props;
    assert_eq!(
        props.number_format,
        Some(NumberFormat::Other("chicago".to_owned()))
    );
    assert_eq!(
        props.number_restart,
        Some(casual_doc_model::v1::NoteNumberRestart::EachPage)
    );
    assert!(!reflowed.report.has(report::FOOTNOTE_MARKER_RENUMBERED));
}

// --- First-page and even/odd headers ------------------------------------------

/// Five pages: a title page with no running head, then a head that
/// alternates between even pages (left) and odd pages (right); a footer on
/// every page.
fn variant_headers_pdf() -> Vec<u8> {
    let pages: Vec<Page> = (0..5)
        .map(|i| {
            let head = match i {
                0 => String::new(),
                i if i % 2 == 1 => text("F1", 9.0, 72.0, 750.0, 11.0, &["Even running head"]),
                _ => text("F1", 9.0, 450.0, 750.0, 11.0, &["Odd running head"]),
            };
            Page::new(format!(
                "{head}{}{}",
                text(
                    "F2",
                    11.0,
                    72.0,
                    700.0,
                    14.0,
                    &PROSE[(i % 4) * 2..(i % 4) * 2 + 2]
                ),
                text("F1", 9.0, 290.0, 30.0, 11.0, &[&format!("Page {}", i + 1)]),
            ))
        })
        .collect();
    synth::document(&pages, "", "")
}

#[test]
fn first_page_and_even_odd_headers_become_their_own_parts() {
    use casual_doc_model::v1::HeaderFooterKind;
    let (_, reflowed) = reflow_of(variant_headers_pdf());
    let definitions = reflowed.document.definitions();
    let section = &definitions.sections[0];
    let part_text = |blocks: &[BlockNode]| {
        blocks
            .iter()
            .filter_map(|b| match b {
                BlockNode::Paragraph(p) => Some(para_text(p)),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("|")
    };
    let headers: Vec<(HeaderFooterKind, String)> = section
        .headers
        .iter()
        .map(|r| {
            (
                r.kind,
                part_text(&definitions.headers.get(&r.reference).unwrap().blocks),
            )
        })
        .collect();
    assert_eq!(
        headers,
        vec![
            (HeaderFooterKind::Default, "Odd running head".to_owned()),
            (HeaderFooterKind::Even, "Even running head".to_owned()),
            (HeaderFooterKind::First, String::new()),
        ]
    );
    let footers: Vec<(HeaderFooterKind, String)> = section
        .footers
        .iter()
        .map(|r| {
            (
                r.kind,
                part_text(&definitions.footers.get(&r.reference).unwrap().blocks),
            )
        })
        .collect();
    assert_eq!(
        footers,
        vec![
            (HeaderFooterKind::Default, "Page {1}".to_owned()),
            (HeaderFooterKind::Even, "Page {1}".to_owned()),
            (HeaderFooterKind::First, "Page {1}".to_owned()),
        ]
    );
    assert_eq!(section.title_page, Some(true));
    assert!(definitions.settings.even_and_odd_headers);
    assert!(
        !texts(&reflowed.document)
            .join("\n")
            .contains("running head")
    );
}

// --- Page breaks and justification --------------------------------------------

fn breaks_pdf() -> Vec<u8> {
    let first = format!(
        "{}{}",
        text("F3", 16.0, 72.0, 720.0, 20.0, &["Chapter One"]),
        text(
            "F2",
            11.0,
            72.0,
            690.0,
            14.0,
            &["A short chapter that ends near the top of its page."]
        ),
    );
    let justified = [
        "Every line of this paragraph is exactly as long as the others are,",
        "so its right edge is straight; the typesetter set it justified and",
        "only the last line, which ends the paragraph, stops short.",
    ];
    let second = format!(
        "{}{}{}{}{}",
        text("F3", 16.0, 72.0, 720.0, 20.0, &["Chapter Two"]),
        text("F6", 10.0, 72.0, 690.0, 12.0, &justified),
        // As wide as the column, but ragged: its lines end at different x.
        text(
            "F6",
            10.0,
            72.0,
            630.0,
            12.0,
            &[
                "A ragged paragraph in the same face reaches the right edge once",
                "and then stops short of it on the lines that",
                "follow, the way unjustified text does.",
            ],
        ),
        text("F2", 11.0, 72.0, 600.0, 14.0, &PROSE[..4]),
        text(
            "F2",
            11.0,
            72.0,
            80.0,
            14.0,
            &["The last line of the body, low on the page."]
        ),
    );
    synth::document(&[Page::new(first), Page::new(second)], "", "")
}

#[test]
fn a_page_that_ends_early_keeps_its_break_and_justified_text_stays_justified() {
    use casual_doc_model::v1::Alignment;
    let (_, reflowed) = reflow_of(breaks_pdf());
    let paras: Vec<(String, Option<bool>, Option<Alignment>)> = paragraphs(&reflowed.document)
        .iter()
        .map(|p| {
            (
                para_text(p),
                p.properties.page_break_before,
                p.properties.alignment,
            )
        })
        .filter(|(t, ..)| !t.is_empty())
        .collect();
    let find = |prefix: &str| {
        paras
            .iter()
            .find(|p| p.0.starts_with(prefix))
            .cloned()
            .unwrap()
    };
    assert_eq!(find("Chapter Two").1, Some(true), "{paras:?}");
    assert_eq!(find("Chapter One").1, None);
    assert_eq!(find("The quick brown").1, None);
    assert_eq!(find("Every line").2, Some(Alignment::Justify));
    // The ragged paragraph is the justified one's twin: the same face and
    // column, three lines, its first reaching the right edge. Only its line
    // ends tell them apart.
    assert_eq!(find("A ragged paragraph").2, None, "{paras:?}");
    assert_eq!(
        find("The quick brown").2,
        None,
        "ragged-right prose is not justified"
    );
    // Exactly one paragraph is justified: every other multi-line paragraph
    // on these pages is ragged.
    let justified: Vec<&String> = paras
        .iter()
        .filter(|p| p.2 == Some(Alignment::Justify))
        .map(|p| &p.0)
        .collect();
    assert_eq!(justified.len(), 1, "{justified:?}");
    assert_eq!(
        reflowed
            .report
            .get(report::JUSTIFICATION_INFERRED)
            .map(|f| f.occurrences),
        Some(1)
    );
    assert!(reflowed.report.has(report::PAGE_BREAK_INFERRED));
}

fn line_ending_at(x1: f64, top: f64) -> super::ILine {
    super::ILine {
        page: 0,
        chars: Vec::new(),
        x0: 72.0,
        x1,
        top,
        bottom: top + 10.0,
        baseline: top + 8.0,
        size: 10.0,
        mcid: None,
        border_below: None,
        shade: None,
    }
}

#[test]
fn only_lines_ending_together_at_the_column_edge_are_justified() {
    let justified = |ends: &[f64]| {
        let lines: Vec<super::ILine> = ends
            .iter()
            .enumerate()
            .map(|(i, &x1)| line_ending_at(x1, 100.0 + 12.0 * i as f64))
            .collect();
        let refs: Vec<&super::ILine> = lines.iter().collect();
        super::emit::justified(&refs, 72.0, 540.0)
    };
    assert!(justified(&[540.0, 539.6, 540.0, 400.0]));
    // Ragged right: the body lines end at different places.
    assert!(!justified(&[540.0, 510.0, 400.0]), "ragged");
    assert!(
        !justified(&[540.0, 539.5, 540.0, 520.0, 300.0]),
        "one line short"
    );
    // Straight, but well short of the column: a narrow block, not justified.
    assert!(!justified(&[400.0, 400.0, 300.0]), "short of the edge");
    // Every line reaches the edge, the last too: not a paragraph's end.
    assert!(!justified(&[540.0, 540.0, 540.0]), "no short last line");
    // Two lines are too few to tell.
    assert!(!justified(&[540.0, 300.0]), "two lines");
}

// --- Floating pictures and vector art -------------------------------------

/// `pages` with a 4x4 grey picture available to every page as `/Im1`.
fn with_picture(pages: &[Page]) -> Vec<u8> {
    let mut pdf = synth::Pdf::default();
    let image = pdf.stream(
        "/Type /XObject /Subtype /Image /Width 4 /Height 4 /ColorSpace /DeviceGray /BitsPerComponent 8",
        &[128u8; 16],
    );
    synth::document_in(
        &mut pdf,
        pages,
        &format!("/XObject << /Im1 {image} 0 R >>"),
        "",
        |_| String::new(),
    )
}

/// The picture drawn at (`x`, `y`) (PDF space, lower left), `w` by `h`.
fn picture(x: f64, y: f64, w: f64, h: f64) -> String {
    format!("q {w} 0 0 {h} {x} {y} cm /Im1 Do Q\n")
}

/// One body paragraph's text, its floats' `(wrap, behind)`, and whether it
/// holds an inline picture.
type Placed = (String, Vec<(WrapMode, bool)>, bool);

fn graphics(document: &Document) -> Vec<Placed> {
    paragraphs(document)
        .into_iter()
        .map(|p| {
            let floats = p
                .inlines
                .iter()
                .filter_map(|i| match i {
                    InlineNode::AnchoredDrawing(d) => Some((d.anchor.wrap, d.anchor.behind_doc)),
                    InlineNode::Group(g) => g.anchor.as_ref().map(|a| (a.wrap, a.behind_doc)),
                    _ => None,
                })
                .collect();
            let inline = p
                .inlines
                .iter()
                .any(|i| matches!(i, InlineNode::Drawing(_)));
            (para_text(p), floats, inline)
        })
        .collect()
}

fn floats_pdf() -> Vec<u8> {
    // Page 1: a picture at the column's left, the prose set to its right
    // (18 pt away), and a full-width paragraph under both.
    let first = format!(
        "{}{}{}",
        picture(72.0, 640.0, 100.0, 80.0),
        text("F2", 11.0, 190.0, 710.0, 14.0, &PROSE[..4]),
        text("F2", 11.0, 72.0, 600.0, 14.0, &PROSE[4..8]),
    );
    // Page 2: a picture as wide as the text, between two paragraphs.
    let second = format!(
        "{}{}{}",
        text("F2", 11.0, 72.0, 720.0, 14.0, &PROSE[..4]),
        picture(72.0, 500.0, 460.0, 120.0),
        text("F2", 11.0, 72.0, 460.0, 14.0, &PROSE[4..8]),
    );
    with_picture(&[Page::new(first), Page::new(second)])
}

#[test]
fn a_picture_beside_text_floats_on_it_and_a_column_wide_one_stays_inline() {
    let (_, reflowed) = reflow_of(floats_pdf());
    let found = graphics(&reflowed.document);
    let with_floats: Vec<&Placed> = found.iter().filter(|p| !p.1.is_empty()).collect();
    assert_eq!(with_floats.len(), 1, "{found:?}");
    let (text, wraps, _) = with_floats[0];
    assert!(text.starts_with("The quick brown"), "{found:?}");
    assert_eq!(wraps, &vec![(WrapMode::Square, false)]);
    // The floated paragraph is not indented by the picture's width: the
    // wrap moves its text.
    let floated = paragraphs(&reflowed.document)
        .into_iter()
        .find(|p| para_text(p).starts_with("The quick brown"))
        .unwrap();
    assert_eq!(floated.properties.indentation, None);
    // Its anchor: the column's left edge and the paragraph's top.
    let anchor = floated
        .inlines
        .iter()
        .find_map(|i| match i {
            InlineNode::AnchoredDrawing(d) => Some(d.anchor.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(anchor.horizontal.relative_from, HorizontalAnchor::Column);
    assert_eq!(anchor.vertical.relative_from, VerticalAnchor::Paragraph);
    // The column-wide picture keeps its own line, inline.
    assert_eq!(found.iter().filter(|p| p.2).count(), 1, "{found:?}");
    assert_eq!(
        reflowed
            .report
            .get(report::FLOAT_INFERRED)
            .map(|f| f.occurrences),
        Some(1)
    );
}

fn vectors_pdf() -> Vec<u8> {
    let content = format!(
        "{}{}{}{}",
        // A pale panel under the first paragraph.
        "0.9 g 66 650 300 80 re f 0 g\n",
        text("F2", 11.0, 72.0, 720.0, 14.0, &PROSE[..4]),
        // A diamond on its own, between the paragraphs.
        "0.5 g 300 400 m 340 440 l 380 400 l 340 360 l f 0 g\n",
        text("F2", 11.0, 72.0, 300.0, 14.0, &PROSE[4..8]),
    );
    synth::document(&[Page::new(content)], "", "")
}

#[test]
fn vector_art_becomes_shape_groups_behind_its_text_or_on_its_own_line() {
    let (_, reflowed) = reflow_of(vectors_pdf());
    let found = graphics(&reflowed.document);
    let panel = found
        .iter()
        .find(|p| p.0.starts_with("The quick brown"))
        .unwrap();
    assert_eq!(panel.1, vec![(WrapMode::None, true)], "{found:?}");
    let own_line: Vec<&Placed> = found
        .iter()
        .filter(|p| p.0.is_empty() && !p.1.is_empty())
        .collect();
    assert_eq!(own_line.len(), 1, "{found:?}");
    assert_eq!(own_line[0].1, vec![(WrapMode::TopAndBottom, false)]);
    // The shapes are the PDF's paths: one custom-geometry shape each.
    let shapes: usize = paragraphs(&reflowed.document)
        .iter()
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| match i {
            InlineNode::Group(g) => Some(
                g.children
                    .iter()
                    .filter(|c| matches!(c, GroupChild::Shape(s) if s.path.is_some()))
                    .count(),
            ),
            _ => None,
        })
        .sum();
    assert_eq!(shapes, 2);
    assert_eq!(
        reflowed
            .report
            .get(report::VECTOR_PLACED)
            .map(|f| f.occurrences),
        Some(2)
    );
    assert!(!reflowed.report.has(report::VECTOR_OMITTED));
}

#[test]
fn a_fill_masking_the_picture_under_it_is_not_placed() {
    let content = format!(
        "{}{}{}",
        text("F2", 11.0, 72.0, 720.0, 14.0, &PROSE[..4]),
        picture(72.0, 400.0, 300.0, 150.0),
        // White boxes inside the picture, painted straight after it.
        "1 g 100 450 120 20 re f 100 420 80 20 re f 0 g\n",
    );
    let (_, reflowed) = reflow_of(with_picture(&[Page::new(content)]));
    let found = graphics(&reflowed.document);
    assert!(found.iter().all(|p| p.1.is_empty()), "{found:?}");
    assert_eq!(found.iter().filter(|p| p.2).count(), 1, "the picture stays");
    assert_eq!(
        reflowed
            .report
            .get(report::VECTOR_OMITTED)
            .map(|f| f.occurrences),
        Some(1)
    );
    assert!(!reflowed.report.has(report::VECTOR_PLACED));
}

fn diamond_pages(n: usize) -> Vec<u8> {
    let pages: Vec<Page> = (0..n)
        .map(|_| {
            Page::new(format!(
                "{}0.5 g 300 400 m 340 440 l 380 400 l 340 360 l f 0 g\n{}",
                text("F2", 11.0, 72.0, 720.0, 14.0, &PROSE[..4]),
                text("F2", 11.0, 72.0, 300.0, 14.0, &PROSE[4..8]),
            ))
        })
        .collect();
    synth::document(&pages, "", "")
}

#[test]
fn graphic_placement_work_doubles_when_the_pages_double() {
    let (a, b) = (work_of(diamond_pages(10)), work_of(diamond_pages(20)));
    let r = ratio(a.graphics, b.graphics);
    assert!(
        (1.7..2.3).contains(&r),
        "graphics: ratio {r:.2} ({} -> {})",
        a.graphics,
        b.graphics
    );
}

// --- Tables ruled with filled rectangles, and tables across a page break ----

fn tables_in(document: &Document) -> Vec<&casual_doc_model::v1::Table> {
    document
        .body()
        .iter()
        .filter_map(|b| match b {
            BlockNode::Table(t) => Some(&**t),
            _ => None,
        })
        .collect()
}

/// A filled rectangle (y up), the way Word, Excel and LibreOffice draw a
/// table's borders and its cell shading.
fn rect(x: f64, y: f64, w: f64, h: f64) -> String {
    format!("{x} {y} {w} {h} re f\n")
}

/// Two tables whose rules are thin filled rectangles in grey:
///
/// - Word's way: every border its own `re f`, each cell drawing its own four
///   (so a shared border is drawn twice, a fraction of a point apart), with a
///   shaded header row and a header cell whose text runs across the column
///   border it spans;
/// - LibreOffice's and Excel's way: every rule one subpath of ONE filled
///   path, and a cell whose text wraps onto a second line.
fn rectangle_ruled_pdf() -> Vec<u8> {
    let mut content = text("F2", 11.0, 72.0, 740.0, 14.0, &["Rectangle-ruled tables."]);
    // Word: header row shading first (paint order), then the borders.
    let (xs, ys) = ([72.0, 200.0, 330.0, 460.0], [700.0, 680.0, 660.0, 640.0]);
    content.push_str("0.85 g\n");
    content.push_str(&rect(xs[0], ys[1], xs[3] - xs[0], ys[0] - ys[1]));
    content.push_str("0.5 g\n");
    for r in 0..3 {
        for c in 0..3 {
            // The header row's first cell spans two columns: no border at
            // xs[1] in that row.
            if r == 0 && c == 1 {
                continue;
            }
            let x0 = xs[c];
            let x1 = if r == 0 && c == 0 { xs[2] } else { xs[c + 1] };
            let (top, bottom) = (ys[r], ys[r + 1]);
            for (x, y, w, h) in [
                (x0, top - 0.25, x1 - x0, 0.25),
                (x0, bottom, x1 - x0, 0.25),
                (x0, bottom, 0.25, top - bottom),
                (x1 - 0.25, bottom, 0.25, top - bottom),
            ] {
                content.push_str(&rect(x, y, w, h));
            }
        }
    }
    content.push_str("0 g\n");
    content.push_str(&text(
        "F2",
        10.0,
        xs[0] + 4.0,
        ys[1] + 6.0,
        12.0,
        &["A header that spans two columns of the grid"],
    ));
    content.push_str(&text(
        "F2",
        10.0,
        xs[2] + 4.0,
        ys[1] + 6.0,
        12.0,
        &["Total"],
    ));
    for (r, y) in ys[1..].windows(2).enumerate() {
        for (c, x) in xs.windows(2).enumerate() {
            content.push_str(&text(
                "F2",
                10.0,
                x[0] + 4.0,
                y[1] + 6.0,
                12.0,
                &[&format!("W{}{}", r + 1, c + 1)],
            ));
        }
    }
    // LibreOffice / Excel: one path, many rectangles, then one fill.
    let (xs, ys) = ([72.0, 250.0, 460.0], [560.0, 520.0, 500.0]);
    content.push_str("0.5 g\n");
    for y in ys {
        content.push_str(&format!("{} {y} {} 0.5 re\n", xs[0], xs[2] - xs[0]));
    }
    for x in xs {
        content.push_str(&format!("{x} {} 0.5 {} re\n", ys[2], ys[0] - ys[2]));
    }
    content.push_str("f\n0 g\n");
    content.push_str(&text(
        "F2",
        10.0,
        xs[0] + 4.0,
        ys[0] - 14.0,
        12.0,
        &["Wrapped cell text that", "continues on a second line"],
    ));
    content.push_str(&text("F2", 10.0, xs[1] + 4.0, ys[0] - 14.0, 12.0, &["L12"]));
    content.push_str(&text("F2", 10.0, xs[0] + 4.0, ys[2] + 6.0, 12.0, &["L21"]));
    content.push_str(&text("F2", 10.0, xs[1] + 4.0, ys[2] + 6.0, 12.0, &["L22"]));
    synth::document(&[Page::new(content)], "", "")
}

/// Rules drawn as thin filled rectangles count as rules, one `re f` each or
/// many in one path; cell text flows in its cell (a wrapped line joins its
/// paragraph, a line the grid split at a spanned border is one line again);
/// the header row's fill is its cells' shading, not a drawing; the rules'
/// grey is the table's border colour. Mutations, each driven red: require
/// `rectangles` to find one subpath (the second table is not found); drop
/// `rejoin` (the spanning header reads as two paragraphs); skip `shade`
/// (`shading` is `None`).
#[test]
fn tables_ruled_with_filled_rectangles_reflow_as_real_tables() {
    let (_, reflowed) = reflow_of(rectangle_ruled_pdf());
    let document = &reflowed.document;
    let tables = tables_in(document);
    assert_eq!(tables.len(), 2, "{:?}", reflowed.report.findings);
    assert_eq!(
        cell_texts(document, 0),
        vec![
            vec!["A header that spans two columns of the grid", "Total"],
            vec!["W11", "W12", "W13"],
            vec!["W21", "W22", "W23"],
        ]
    );
    assert_eq!(tables[0].rows[0].cells[0].properties.grid_span, Some(2));
    assert_eq!(
        cell_texts(document, 1),
        vec![
            vec!["Wrapped cell text that continues on a second line", "L12"],
            vec!["L21", "L22"],
        ]
    );
    // The header row's shading is on its cells, light grey; the body's none.
    for cell in &tables[0].rows[0].cells {
        let fill = cell.properties.shading.fill.expect("a shaded header cell");
        assert!(
            fill.r == fill.g && fill.g == fill.b && (200..=230).contains(&fill.r),
            "{fill:?}"
        );
    }
    assert!(tables[0].rows[1].cells[0].properties.shading.fill.is_none());
    // The rules are grey, hairline-thin.
    let top = tables[0].properties.borders.top.as_ref().expect("ruled");
    let color = top.color.expect("the rules' colour");
    assert!(
        color.r == color.g && (120..=135).contains(&color.r),
        "{color:?}"
    );
    assert_eq!(top.size_eighth_points, Some(2));
    // The fills became shading and borders, not shapes.
    assert!(
        !reflowed.report.has(report::VECTOR_PLACED),
        "{:?}",
        reflowed.report.findings
    );
    let body = texts(document).join("\n");
    assert!(!body.contains("W22") && !body.contains("L22"), "{body}");
}

/// A ruled table of `rows` data rows under a header row, from `top` (y up)
/// down, its rules thin filled rectangles.
fn ruled_rows(top: f64, first: usize, rows: usize) -> String {
    let xs = [72.0, 250.0, 400.0];
    let h = 20.0;
    let mut out = String::from("0 g\n");
    let bottom = top - h * (rows + 1) as f64;
    for r in 0..=rows + 1 {
        out.push_str(&rect(xs[0], top - h * r as f64, xs[2] - xs[0], 0.5));
    }
    for x in xs {
        out.push_str(&rect(x, bottom, 0.5, top - bottom));
    }
    out.push_str(&text(
        "F3",
        10.0,
        xs[0] + 4.0,
        top - h + 6.0,
        12.0,
        &["Item"],
    ));
    out.push_str(&text(
        "F3",
        10.0,
        xs[1] + 4.0,
        top - h + 6.0,
        12.0,
        &["Quantity"],
    ));
    for r in 0..rows {
        let y = top - h * (r + 2) as f64 + 6.0;
        let n = first + r;
        out.push_str(&text(
            "F2",
            10.0,
            xs[0] + 4.0,
            y,
            12.0,
            &[&format!("Item {n}")],
        ));
        out.push_str(&text(
            "F2",
            10.0,
            xs[1] + 4.0,
            y,
            12.0,
            &[&format!("{}", n * 3)],
        ));
    }
    out
}

/// A table the page break cut in two, its header repeated on the next page,
/// as Word and LibreOffice write a table with a repeating header row.
fn continued_table_pdf() -> Vec<u8> {
    let first = format!(
        "{}{}",
        text(
            "F2",
            11.0,
            72.0,
            740.0,
            14.0,
            &["Inventory, continued overleaf."]
        ),
        ruled_rows(680.0, 1, 25),
    );
    let second = format!(
        "{}{}",
        ruled_rows(680.0, 26, 4),
        text("F2", 11.0, 72.0, 560.0, 14.0, &["After the table."]),
    );
    synth::document(&[Page::new(first), Page::new(second)], "", "")
}

/// One table in the flowing document, with every row once and the header
/// row repeating on each page (`w:tblHeader`). Mutation: return 0 from
/// `continue_across_pages` without joining and the body holds two tables.
#[test]
fn a_table_cut_by_a_page_break_is_one_table_with_a_repeating_header() {
    let (_, reflowed) = reflow_of(continued_table_pdf());
    let document = &reflowed.document;
    let tables = tables_in(document);
    assert_eq!(tables.len(), 1, "{:?}", reflowed.report.findings);
    let rows = cell_texts(document, 0);
    assert_eq!(
        rows.len(),
        1 + 29,
        "the header once, then 29 rows: {rows:?}"
    );
    assert_eq!(rows[0], vec!["Item", "Quantity"]);
    assert_eq!(rows[1], vec!["Item 1", "3"]);
    assert_eq!(rows[25], vec!["Item 25", "75"]);
    assert_eq!(rows[26], vec!["Item 26", "78"], "page 2 follows on");
    assert_eq!(rows[29], vec!["Item 29", "87"]);
    assert!(
        tables[0].rows[0].properties.header,
        "the header row repeats"
    );
    assert!(!tables[0].rows[1].properties.header);
    assert_eq!(
        reflowed
            .report
            .get(report::TABLE_CONTINUED)
            .map(|f| f.occurrences),
        Some(1)
    );
    assert!(texts(document).iter().any(|t| t == "After the table."));
}

/// The corpus: a table our own writer printed (each cell's borders thin
/// filled rectangles, drawn twice, with a two-column and a two-row merge)
/// reflows as one table of real cells, not frames or paragraphs.
#[test]
fn the_writer_corpus_table_with_merges_reflows_as_a_real_table() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/pdf/writer-real-producer-table-merges.pdf"),
    )
    .expect("fixture");
    let (_, reflowed) = reflow_of(bytes);
    let document = &reflowed.document;
    let tables = tables_in(document);
    assert_eq!(tables.len(), 1, "{:?}", reflowed.report.findings);
    let rows = cell_texts(document, 0);
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert_eq!(rows[0], vec!["Spans two columns", "C"], "{rows:?}");
    assert_eq!(tables[0].rows[0].cells[0].properties.grid_span, Some(2));
    assert_eq!(rows[1], vec!["Spans two rows", "B2", "C2"], "{rows:?}");
    assert_eq!(rows[2][1..], ["B3", "C3"], "{rows:?}");
    assert!(
        tables[0].rows[2].cells[0]
            .properties
            .vertical_merge
            .is_some(),
        "the two-row merge"
    );
    let body = texts(document).join("\n");
    assert!(body.contains("After the table."), "{body}");
    assert!(!body.contains("B2"), "{body}");
}

// --- Found by the ground truth (`tools/opendoc-reflow-score`, docs/182 §18.10) --

/// A borderless table the way Word and LibreOffice print one: every cell its
/// own line, rows 22 pt apart at 9 pt (farther than a paragraph's leading, so
/// every cell is its own block), a header row left-aligned over a column
/// whose body is centred.
fn spaced_table_pdf() -> Vec<u8> {
    let mut content = text("F2", 11.0, 72.0, 720.0, 14.0, &["A borderless table."]);
    let header = ["Feature", "Input", "Status"];
    let rows = [
        ["Paragraph styles", "Heading 1", "PASS"],
        ["Cell shading", "Blue", "PASS"],
        ["Text wrapping", "Long cell", "REVIEW"],
        ["Merged cells", "Two columns", "PASS"],
    ];
    for (c, cell) in header.iter().enumerate() {
        content.push_str(&text(
            "F3",
            9.0,
            [84.0, 200.0, 400.0][c],
            680.0,
            11.0,
            &[cell],
        ));
    }
    for (r, row) in rows.iter().enumerate() {
        let y = 680.0 - 22.0 * (r + 1) as f64;
        for (c, cell) in row.iter().enumerate() {
            // The status column is centred on x = 420.
            let x = if c == 2 {
                420.0 - 2.5 * cell.len() as f64
            } else {
                [84.0, 200.0][c]
            };
            content.push_str(&text("F2", 9.0, x, y, 11.0, &[cell]));
        }
    }
    synth::document(&[Page::new(content)], "", "")
}

/// Mutations, each driven red: drop the row linking in
/// `whitespace_candidates` (every row is its own one-row candidate, no
/// table); give `whitespace_table` a column per element (twelve columns).
#[test]
fn a_borderless_table_whose_cells_are_each_a_block_is_one_table() {
    let (_, reflowed) = reflow_of(spaced_table_pdf());
    let document = &reflowed.document;
    assert_eq!(tables_in(document).len(), 1, "{:?}", texts(document));
    let rows = cell_texts(document, 0);
    assert_eq!(rows[0], vec!["Feature", "Input", "Status"]);
    assert_eq!(rows[3], vec!["Text wrapping", "Long cell", "REVIEW"]);
    assert_eq!(rows.len(), 5);
}

/// Lines 24 pt apart at 10 pt, double spaced (the block grouping splits them)
/// on a page whose text is narrower than another page's: one paragraph, not
/// one per line. Mutations: measure every page against the document-wide
/// right edge in `emit::column` (the lines look short and stay apart); drop
/// the word-fit test in `text::continues` (the same).
#[test]
fn widely_leaded_lines_on_a_narrow_page_join_into_one_paragraph() {
    let lines = [
        "Every line of this paragraph is set with a generous",
        "leading, so the grouping puts each one in a block of",
        "its own; the reflow must still join them as a single",
        "paragraph because each one ran to the column's edge.",
    ];
    let mut first = String::new();
    for (i, l) in lines.iter().enumerate() {
        first.push_str(&text("F1", 10.0, 72.0, 700.0 - 24.0 * i as f64, 24.0, &[l]));
    }
    let second = text(
        "F1",
        10.0,
        72.0,
        700.0,
        14.0,
        &[
            "A wide page: this line runs much further to the right than any line of the first page does.",
        ],
    );
    let (_, reflowed) = reflow_of(synth::document(
        &[Page::new(first), Page::new(second)],
        "",
        "",
    ));
    let body = texts(&reflowed.document);
    assert!(
        body.iter()
            .any(|t| t.starts_with("Every line") && t.ends_with("column's edge.")),
        "{body:?}"
    );
}

/// A hanging indent and a bullet whose wrapped line hangs under its text
/// stay one paragraph each; code lines in a monospaced face stay one
/// paragraph per line. Mutations: drop the `under` inset in
/// `text::continues` (the wrapped lines split off); drop the monospaced
/// short-line break from `text::segment` (the code joins into one paragraph).
#[test]
fn hanging_lines_join_and_code_lines_do_not() {
    let mut content = String::new();
    content.push_str(&text(
        "F1",
        10.0,
        72.0,
        700.0,
        14.0,
        &["Hanging indent: the first line begins at the left edge while the lines"],
    ));
    content.push_str(&text(
        "F1",
        10.0,
        108.0,
        682.0,
        14.0,
        &["that follow begin half an inch further in, as glossaries set them."],
    ));
    content.push_str(&text("F1", 10.0, 72.0, 640.0, 14.0, &["Code follows."]));
    for (i, l) in [
        "pub fn render(document: &Document) {",
        "    let layout = document.layout();",
        "}",
    ]
    .iter()
    .enumerate()
    {
        content.push_str(&text("F6", 10.0, 72.0, 610.0 - 14.0 * i as f64, 14.0, &[l]));
    }
    let (_, reflowed) = reflow_of(synth::document(&[Page::new(content)], "", ""));
    let body = texts(&reflowed.document);
    assert!(
        body.iter()
            .any(|t| t.starts_with("Hanging indent") && t.ends_with("glossaries set them.")),
        "{body:?}"
    );
    assert!(
        body.iter()
            .any(|t| t == "pub fn render(document: &Document) {"),
        "{body:?}"
    );
    assert!(
        body.iter()
            .any(|t| t.trim() == "let layout = document.layout();"),
        "{body:?}"
    );
}

/// A title used once, two heading levels used more than once, and a one-off
/// larger subtitle: Title, Heading 1, Heading 2, and the subtitle stays body
/// text, with no level skipped. Mutations: drop `HEADING_MIN_PARAGRAPHS`
/// (the subtitle takes a level and pushes the rest down); drop the title
/// (the document's levels start at the title).
#[test]
fn a_title_and_dense_heading_levels() {
    let first = format!(
        "{}{}{}{}{}{}",
        text("F3", 26.0, 72.0, 720.0, 30.0, &["Annual Synthetic Review"]),
        text(
            "F1",
            14.0,
            72.0,
            690.0,
            18.0,
            &["A subtitle set once, larger than the body"]
        ),
        text("F3", 16.0, 72.0, 650.0, 20.0, &["Background"]),
        text("F3", 13.0, 72.0, 620.0, 16.0, &["Scope"]),
        text("F2", 11.0, 72.0, 600.0, 14.0, &PROSE[0..2]),
        text("F3", 13.0, 72.0, 560.0, 16.0, &["Method"]),
    );
    let second = format!(
        "{}{}{}",
        text("F2", 11.0, 72.0, 720.0, 14.0, &PROSE[2..4]),
        text("F3", 16.0, 72.0, 680.0, 20.0, &["Results"]),
        text("F2", 11.0, 72.0, 660.0, 14.0, &PROSE[4..8]),
    );
    let (_, reflowed) = reflow_of(synth::document(
        &[Page::new(first), Page::new(second)],
        "",
        "",
    ));
    let document = &reflowed.document;
    let named: Vec<(Option<String>, String)> = paragraphs(document)
        .into_iter()
        .map(|p| (style_name(document, p), para_text(p)))
        .collect();
    let style = |text: &str| {
        named
            .iter()
            .find(|(_, t)| t == text)
            .map(|(s, _)| s.clone())
            .unwrap_or_else(|| panic!("{text}: {named:?}"))
    };
    assert_eq!(style("Annual Synthetic Review").as_deref(), Some("Title"));
    assert_eq!(style("Background").as_deref(), Some("heading 1"));
    assert_eq!(style("Results").as_deref(), Some("heading 1"));
    assert_eq!(style("Scope").as_deref(), Some("heading 2"));
    assert_eq!(style("A subtitle set once, larger than the body"), None);
}

/// Two rows of short cells side by side are not a page set in columns.
/// Mutation: drop `MIN_COLUMN_WIDTH` from `order::xy_cut_pp` (a three-column
/// section).
#[test]
fn short_side_by_side_cells_are_not_a_column_section() {
    let mut content = text("F2", 11.0, 72.0, 720.0, 14.0, &PROSE[0..2]);
    for (r, row) in [["A1", "B1", "C1"], ["A2", "B2", "C2"]].iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            content.push_str(&text(
                "F2",
                10.0,
                [84.0, 240.0, 396.0][c],
                660.0 - 22.0 * r as f64,
                12.0,
                &[cell],
            ));
        }
    }
    content.push_str(&text("F2", 11.0, 72.0, 580.0, 14.0, &PROSE[2..4]));
    let (_, reflowed) = reflow_of(synth::document(&[Page::new(content)], "", ""));
    assert!(
        !reflowed.report.has(report::COLUMNS_INFERRED),
        "{:?}",
        reflowed.report.findings
    );
    assert!(
        reflowed
            .document
            .definitions()
            .sections
            .iter()
            .all(|s| s.columns.count <= 1)
    );
}

/// A borderless table cut by a page break whose next page does not repeat
/// the header (our writer's, and many producers'), with its centred column's
/// text at another offset: one table. Mutation: compare whitespace columns by
/// width in `continues` (two tables).
#[test]
fn a_borderless_table_cut_by_a_page_break_is_one_table() {
    let rows = |first: usize, n: usize, top: f64, header: bool| {
        let mut out = String::new();
        let mut y = top;
        if header {
            for (c, cell) in ["ID", "Feature", "Result"].iter().enumerate() {
                out.push_str(&text("F3", 9.0, [84.0, 140.0, 400.0][c], y, 11.0, &[cell]));
            }
            y -= 22.0;
        }
        for k in 0..n {
            let id = format!("R-{:02}", first + k);
            out.push_str(&text("F2", 9.0, 92.0, y, 11.0, &[&id]));
            out.push_str(&text(
                "F2",
                9.0,
                140.0,
                y,
                11.0,
                &[&format!("Feature {}", first + k)],
            ));
            out.push_str(&text("F2", 9.0, 412.0, y, 11.0, &["ok"]));
            y -= 22.0;
        }
        out
    };
    let first = format!(
        "{}{}",
        text("F2", 11.0, 72.0, 720.0, 14.0, &["A long borderless table."]),
        rows(1, 20, 680.0, true)
    );
    let second = format!(
        "{}{}",
        rows(21, 5, 700.0, false),
        text("F2", 11.0, 72.0, 560.0, 14.0, &["After the table."])
    );
    let (_, reflowed) = reflow_of(synth::document(
        &[Page::new(first), Page::new(second)],
        "",
        "",
    ));
    let document = &reflowed.document;
    assert_eq!(
        tables_in(document).len(),
        1,
        "{:?}",
        reflowed.report.findings
    );
    assert_eq!(cell_texts(document, 0).len(), 1 + 25);
}

/// One visual line written as several marked-content sequences, the way
/// Chromium writes every link, citation and styled run (Wikipedia's PDFs):
/// one paragraph with its spaces, the raised citation on the line, and no
/// table made of the pieces. Mutations: drop `same_line` from the
/// continuation in `emit` (each piece is a paragraph); drop the gutter test
/// in `whitespace_candidates` (with the second line below, the pieces form a
/// table).
#[test]
fn a_line_written_in_pieces_is_one_paragraph() {
    // Courier: every character is 0.6 em, so the pieces abut exactly.
    let piece = |mcid: usize, x: f64, y: f64, size: f64, s: &str| {
        format!(
            "/Span <</MCID {mcid}>> BDC BT /F6 {size} Tf {x} {y} Td ({}) Tj ET EMC\n",
            synth::literal(s)
        )
    };
    let mut content = String::new();
    let mut mcid = 0;
    for (row, y) in [700.0, 688.0].into_iter().enumerate() {
        let mut x = 72.0;
        let parts: &[&str] = if row == 0 {
            &[
                "Portable Document Format",
                " (PDF), standardized",
                " as ISO 32000",
            ]
        } else {
            &["is a file format", " developed by Adobe", " in 1992."]
        };
        for part in parts {
            content.push_str(&piece(mcid, x, y, 10.0, part));
            mcid += 1;
            x += 6.0 * part.chars().count() as f64;
        }
        if row == 0 {
            content.push_str(&piece(mcid, x, y + 3.0, 7.0, "[2]"));
            mcid += 1;
        }
    }
    let (_, reflowed) = reflow_of(synth::document(&[Page::new(content)], "", ""));
    let body = texts(&reflowed.document);
    assert!(
        body.iter()
            .any(|t| t.starts_with("Portable Document Format (PDF), standardized as ISO 32000[2]")),
        "{body:?}"
    );
    assert!(tables_in(&reflowed.document).is_empty(), "{body:?}");
}

/// An indent past the model's 22-inch domain is clamped, not a validation
/// failure that loses the whole flowing document (`arxiv-2210.11416` in the
/// corpus). Mutation: return `twips(points)` unclamped.
#[test]
fn an_indent_past_the_models_domain_is_clamped() {
    assert_eq!(super::emit::indent_twips(10_000.0), 31_680);
    assert_eq!(super::emit::indent_twips(-10_000.0), -31_680);
    assert_eq!(super::emit::indent_twips(36.0), 720);
}

/// A whitespace table whose measured column comes out negative (text that
/// overhangs the next column's start) still validates: widths are clamped
/// into the model's domain (the corpus sweep's `table.grid.column.width`
/// failure, which lost a whole flowing document). Mutation: drop the clamp
/// in `Emitter::table`.
#[test]
fn a_table_with_an_overhanging_column_still_validates() {
    use super::tables::{ICell, ITable};
    let table = ITable {
        rows: vec![vec![
            ICell {
                span: 1,
                x0: 100.0,
                x1: 90.0,
                ..ICell::default()
            },
            ICell {
                span: 1,
                x0: 90.0,
                x1: 3000.0,
                ..ICell::default()
            },
        ]],
        columns: vec![-10.0, 2910.0],
        ..ITable::default()
    };
    let grid: Vec<i32> = table
        .columns
        .iter()
        .map(|w| super::emit::grid_width(*w))
        .collect();
    assert_eq!(grid, vec![1, 31_680]);
    assert_eq!(
        super::emit::cell_width(table.rows[0][0].x1 - table.rows[0][0].x0),
        0
    );
}

/// A two-row borderless table under a shaded header band that spans its
/// columns, the way Word and LibreOffice print a merged header row
/// (`sample.docx` OD-TBL-002): one table of three rows, the header one cell
/// spanning every column, centred, and the rows' cells side by side, not
/// a staircase of indented paragraphs. Mutations, each driven red: no
/// header band found (`shaded_header` answering `None`: two rows are no
/// table); the header keeping its words' extent (it lands in one column of
/// the first row); a spanning single-line cell getting no row of its own in
/// `whitespace_table`; `centred_in` answering `false`.
#[test]
fn a_shaded_header_band_makes_two_aligned_rows_a_table() {
    let mut content = text("F2", 11.0, 72.0, 720.0, 14.0, &["Merged cells."]);
    content.push_str("0.91 0.93 0.96 rg\n");
    content.push_str(&rect(72.0, 672.0, 420.0, 20.0));
    content.push_str("0 g\n");
    content.push_str(&text(
        "F2",
        9.0,
        // Centred on the band: 132 pt of words around x = 282.
        216.0,
        678.0,
        11.0,
        &["Merged header across three columns"],
    ));
    let rows = [["A1", "B1", "C1"], ["Merged pair", "", "Standalone"]];
    for (r, row) in rows.iter().enumerate() {
        let y = 678.0 - 24.0 * (r + 1) as f64;
        for (c, cell) in row.iter().enumerate() {
            if !cell.is_empty() {
                content.push_str(&text("F2", 9.0, [84.0, 230.0, 380.0][c], y, 11.0, &[cell]));
            }
        }
    }
    // The next paragraph one row pitch below the last row: the row's own
    // height already holds that space.
    content.push_str(&text(
        "F2",
        9.0,
        72.0,
        678.0 - 24.0 * 3.0,
        11.0,
        &["After the table."],
    ));
    let (_, reflowed) = reflow_of(synth::document(&[Page::new(content)], "", ""));
    let document = &reflowed.document;
    let tables = tables_in(document);
    assert_eq!(tables.len(), 1, "{:?}", texts(document));
    let rows = cell_texts(document, 0);
    assert_eq!(rows.len(), 3, "{rows:?}");
    let after = paragraphs(document)
        .into_iter()
        .find(|p| para_text(p).starts_with("After"))
        .expect("the paragraph after the table");
    let before = after
        .properties
        .spacing
        .as_ref()
        .and_then(|s| s.before_twips)
        .unwrap_or(0);
    assert!(
        before < 100,
        "the last row's slack is not counted again above the next paragraph: {before} twips"
    );
    assert_eq!(
        rows[0],
        vec!["Merged header across three columns"],
        "{rows:?}"
    );
    assert_eq!(rows[1], vec!["A1", "B1", "C1"], "{rows:?}");
    let header = &tables[0].rows[0].cells[0];
    assert_eq!(header.properties.grid_span, Some(3));
    let BlockNode::Paragraph(paragraph) = &header.blocks[0] else {
        panic!("a paragraph");
    };
    assert_eq!(
        paragraph.properties.alignment,
        Some(casual_doc_model::v1::Alignment::Center),
        "the header is centred across the columns it spans"
    );
}

/// A line set with letter spacing (`2 Tc`: a subtitle's expanded tracking)
/// reflows with that spacing on its runs, and a line set solid with none.
/// Mutations, each driven red: `track` never setting a run's tracking; the
/// run properties ignoring it; the agreement test dropped (a solid line's
/// kerning noise reads as tracking, here forced by a kerned TJ).
#[test]
fn letter_spacing_reflows_as_character_spacing() {
    let mut content = text(
        "F2",
        10.0,
        72.0,
        720.0,
        14.0,
        &["An ordinary paragraph set solid, with nothing between its letters."],
    );
    content.push_str("BT /F2 14 Tf 2 Tc 72 680 Td (EXPANDED SUBTITLE) Tj 0 Tc ET\n");
    // Kerned, not tracked: uneven adjustments between a few pairs.
    content.push_str("BT /F2 10 Tf 72 650 Td [(A) -20 (V) -90 (A) -40 (T) -80 (W) -50 (a) -25 (y) -70 (s)] TJ ET\n");
    let (_, reflowed) = reflow_of(synth::document(&[Page::new(content)], "", ""));
    let spacing_of = |start: &str| -> Vec<Option<i32>> {
        paragraphs(&reflowed.document)
            .into_iter()
            .find(|p| para_text(p).starts_with(start))
            .unwrap_or_else(|| panic!("{start}: {:?}", texts(&reflowed.document)))
            .inlines
            .iter()
            .filter_map(|i| match i {
                InlineNode::Run(run) => Some(run.properties.character_spacing_twips),
                _ => None,
            })
            .collect()
    };
    let expanded = spacing_of("EXPANDED");
    assert!(
        expanded.iter().all(|s| *s == Some(40)),
        "2 pt of tracking is 40 twips on every run: {expanded:?}"
    );
    assert!(spacing_of("An ordinary").iter().all(Option::is_none));
    assert!(
        spacing_of("AVA").iter().all(Option::is_none),
        "kerning is not tracking"
    );
}

/// Text wrapped around a picture on its right: the lines beside it stop
/// short of the column at the picture's edge, and the sentence carries on
/// below the picture across the column. One paragraph, floated on by the
/// picture, not a paragraph ending at the picture's edge. Mutation, driven
/// red: `wrap_right` answering the column's edge.
#[test]
fn text_wrapped_around_a_picture_stays_one_paragraph() {
    let beside = [
        "Text that wraps around a picture runs to the picture",
        "and not to the column, so every line beside it stops",
        "at the same place, a little short of the picture edge",
        "where the picture begins, and the sentence carries on",
    ];
    let below = [
        "below the picture across the full width of the column where the text now runs",
        "to the right margin and the paragraph ends here at last.",
    ];
    let content = format!(
        "{}{}{}",
        picture(312.0, 655.0, 100.0, 65.0),
        text("F1", 10.0, 72.0, 710.0, 14.0, &beside),
        text("F1", 10.0, 72.0, 630.0, 14.0, &below),
    );
    let (_, reflowed) = reflow_of(with_picture(&[Page::new(content)]));
    let found = graphics(&reflowed.document);
    let whole = found
        .iter()
        .find(|(text, _, _)| text.starts_with("Text that wraps"))
        .unwrap_or_else(|| panic!("{found:?}"));
    assert!(
        whole.0.ends_with("ends here at last."),
        "the lines below the picture continue the paragraph: {found:?}"
    );
    assert_eq!(whole.1, vec![(WrapMode::Square, false)], "{found:?}");
}
