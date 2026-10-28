//! Reflow, per construct family, on rights-safe synthetic PDFs (`synth.rs`),
//! plus the tagged reading order, a loss-report fixture and the doubling
//! guards (SKILL §4, §8). Each guard's mutation and red output is recorded in
//! `docs/182` §18.

use casual_doc_model::v1::{
    BlockNode, Document, FieldKind, HyperlinkTarget, InlineNode, NumberFormat, Paragraph,
};

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
    assert_eq!(
        named[0],
        (Some("heading 1".into()), "Annual Synthetic Review".into())
    );
    assert_eq!(named[1], (Some("heading 2".into()), "Background".into()));
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
    assert_eq!(named[3], (Some("heading 2".into()), "Method".into()));
    assert_eq!(named[5], (Some("heading 2".into()), "Results".into()));
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
            report::VECTOR_OMITTED,
        ]
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
