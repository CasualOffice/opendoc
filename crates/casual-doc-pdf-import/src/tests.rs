use casual_doc_model::v1::{BlockNode, InlineNode, LineRule, TextBoxAutoFit};

use super::*;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/pdf")
            .join(name),
    )
    .expect(name)
}

fn import(name: &str) -> PdfImport {
    import_pdf(fixture(name), PdfReadLimits::default()).expect(name)
}

/// The page anchor paragraphs, one per page.
fn pages(document: &Document) -> Vec<&casual_doc_model::v1::Paragraph> {
    document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(p) => Some(p),
            _ => None,
        })
        .collect()
}

/// Text of every text box on a page, frame by frame.
fn frames(paragraph: &casual_doc_model::v1::Paragraph) -> Vec<String> {
    paragraph
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            InlineNode::TextBox(text_box) => Some(
                text_box
                    .blocks
                    .iter()
                    .filter_map(|b| match b {
                        BlockNode::Paragraph(p) => Some(p),
                        _ => None,
                    })
                    .flat_map(|p| p.inlines.iter())
                    .map(|inline| match inline {
                        InlineNode::Run(run) => run.text.clone(),
                        InlineNode::Break(_) => "\n".to_owned(),
                        _ => String::new(),
                    })
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

#[test]
fn prose_becomes_one_section_per_page_with_framed_paragraphs() {
    let imported = import("synthetic-prose.pdf");
    let document = &imported.document;
    assert_eq!(document.definitions().sections.len(), 2);
    let section = &document.definitions().sections[0];
    assert_eq!(section.page_size.width_twips, 612 * 20);
    assert_eq!(section.page_margins.top_twips, 0);
    let pages = pages(document);
    assert_eq!(pages.len(), 2, "one anchor paragraph per page");
    assert_eq!(
        pages[0].properties.section_break,
        Some(section.id),
        "page 1's paragraph ends page 1's section"
    );
    assert_eq!(pages[1].properties.section_break, None);
    let first = frames(pages[0]);
    // Heading, then the two four-line paragraphs as separate blocks.
    assert_eq!(first.len(), 3, "{first:#?}");
    assert_eq!(first[0], "Synthetic prose");
    assert_eq!(first[1].lines().count(), 4);
    assert!(first[1].starts_with("The quick brown fox jumps over the lazy dog"));
    // Exact line pitch from the 14 pt leading, and grow-on-overflow.
    let InlineNode::TextBox(text_box) = &pages[0].inlines[1] else {
        panic!("second inline is a frame");
    };
    assert_eq!(text_box.body_properties.auto_fit, TextBoxAutoFit::Shape);
    let BlockNode::Paragraph(paragraph) = &text_box.blocks[0] else {
        panic!()
    };
    let spacing = paragraph.properties.spacing.unwrap();
    assert_eq!(spacing.line_rule, Some(LineRule::Exact));
    assert_eq!(spacing.line_twips, Some(280));
    assert!(imported.report.has(report::PARAGRAPH_BOUNDARY_INFERRED));
    assert!(imported.report.has(report::FONT_SUBSTITUTED));
}

#[test]
fn z_order_follows_paint_order() {
    let imported = import("synthetic-images.pdf");
    let page = pages(&imported.document)[0];
    let heights: Vec<u32> = page
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            InlineNode::AnchoredDrawing(d) => d.relative_height,
            InlineNode::TextBox(t) => t.relative_height,
            InlineNode::Group(g) => g.relative_height,
            _ => None,
        })
        .collect();
    assert_eq!(heights.len(), 4, "three pictures then the caption");
    assert!(heights.windows(2).all(|w| w[0] < w[1]), "{heights:?}");
    let crops: Vec<bool> = page
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            InlineNode::AnchoredDrawing(d) => Some(d.crop.is_some()),
            _ => None,
        })
        .collect();
    assert_eq!(crops, vec![false, false, true], "the clip became a crop");
    assert_eq!(imported.resources.len(), 3);
}

#[test]
fn every_construct_file_imports_and_reports_its_losses() {
    let expect: [(&str, &[&str]); 10] = [
        ("synthetic-prose.pdf", &[report::FONT_SUBSTITUTED]),
        (
            "synthetic-two-column.pdf",
            &[report::READING_ORDER_INFERRED],
        ),
        ("synthetic-vector.pdf", &[]),
        ("synthetic-embedded-truetype.pdf", &[report::FONT_EMBEDDED]),
        ("synthetic-images.pdf", &[]),
        ("synthetic-rotated-text.pdf", &[report::TEXT_ROTATED]),
        ("synthetic-tagged.pdf", &[report::TAG_STRUCTURE_IGNORED]),
        (
            "synthetic-type3.pdf",
            &[
                report::NATIVE_TEXT_INVALID,
                report::TYPE3_GLYPHS_SUBSTITUTED,
            ],
        ),
        ("synthetic-page-boxes.pdf", &[]),
        (
            "synthetic-active-content.pdf",
            &[
                report::ACTIVE_CONTENT_BLOCKED,
                report::ATTACHMENT_OMITTED,
                report::FORM_OMITTED,
                report::ANNOTATION_OMITTED,
            ],
        ),
    ];
    for (name, features) in expect {
        let imported = import(name);
        imported.document.validate().expect(name);
        for feature in features {
            assert!(
                imported.report.has(feature),
                "{name} must report {feature}; got {:?}",
                imported.report.findings
            );
        }
        assert!(report::FEATURES.windows(2).all(|w| w[0] < w[1]));
        for finding in &imported.report.findings {
            assert!(
                report::FEATURES.contains(&finding.feature),
                "{}",
                finding.feature
            );
        }
    }
}

#[test]
fn every_writer_file_imports_with_its_text() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pdf");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.starts_with("writer-") {
            continue;
        }
        let imported = import(&name);
        let text: String = pages(&imported.document)
            .iter()
            .flat_map(|p| frames(p))
            .collect();
        assert!(
            text.chars().filter(|c| c.is_alphanumeric()).count() > 20,
            "{name}"
        );
    }
}

#[test]
fn encrypted_input_is_refused_with_its_finding_id() {
    let error = import_pdf(
        fixture("hostile/hostile-encrypted.pdf"),
        PdfReadLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.feature(), Some(report::ENCRYPTED_UNSUPPORTED));
    assert!(
        error
            .to_string()
            .contains("pdf.import.encrypted_unsupported")
    );
}

#[test]
fn the_job_steps_by_budget_and_cancel_produces_nothing() {
    let mut job =
        ImportJob::begin(fixture("synthetic-prose.pdf"), PdfReadLimits::default()).expect("admit");
    assert_eq!(job.step(1), Progress::Pending { page: 1, of: 2 });
    job.cancel();
    assert_eq!(job.step(1), Progress::Cancelled);
    assert_eq!(job.finish().unwrap_err(), PdfImportError::Cancelled);

    let mut job =
        ImportJob::begin(fixture("synthetic-prose.pdf"), PdfReadLimits::default()).expect("admit");
    assert_eq!(job.step(1), Progress::Pending { page: 1, of: 2 });
    assert_eq!(job.step(1), Progress::Done);
    let stepped = job.finish().expect("finish");
    let whole = import("synthetic-prose.pdf");
    assert_eq!(
        stepped.document, whole.document,
        "stepping does not change the result (and ids are deterministic)"
    );
}

#[test]
#[allow(clippy::field_reassign_with_default)] // non_exhaustive: no struct update
fn a_page_that_breaks_a_limit_fails_the_whole_job_atomically() {
    let mut limits = PdfReadLimits::default();
    limits.max_evidence_per_page = 100;
    // Page 1 of the prose file has more than 100 glyphs.
    let mut job = ImportJob::begin(fixture("synthetic-prose.pdf"), limits).expect("admit");
    assert_eq!(job.step(1), Progress::Failed);
    assert!(matches!(
        job.finish(),
        Err(PdfImportError::Read(PdfReadError::Limit {
            limit: "max_evidence_per_page",
            ..
        }))
    ));
}

/// A minimal PDF of `pages` pages, each with `lines` lines laid out in
/// narrow columns so no line joins another's block: the worst case for the
/// line-to-block search.
fn generated(pages: usize, lines: usize) -> Vec<u8> {
    let mut objects: Vec<String> = Vec::new();
    let mut kids = Vec::new();
    let page_ids: Vec<usize> = (0..pages).map(|i| 3 + i * 2).collect();
    objects.push("<< /Type /Catalog /Pages 2 0 R >>".to_owned());
    for id in &page_ids {
        kids.push(format!("{id} 0 R"));
    }
    objects.push(format!(
        "<< /Type /Pages /Kids [{}] /Count {pages} >>",
        kids.join(" ")
    ));
    for id in &page_ids {
        let mut content = String::new();
        for line in 0..lines {
            // 54 rows a column at a 14 pt pitch (8 pt type: a 6 pt gap, over
            // line_margin), columns 40 pt apart: every line is its own block.
            let x = 10 + (line / 54) * 40;
            let y = 780 - (line % 54) * 14;
            content.push_str(&format!("BT /F1 8 Tf {x} {y} Td (row {line}) Tj ET\n"));
        }
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents {} 0 R /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> >>",
            id + 1
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len() + 1
        ));
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", index + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// Doubling the lines on a page doubles the grouping work: the line-to-block
/// search is windowed, not against every block (SKILL §8: a counter, not a
/// clock).
#[test]
fn grouping_work_doubles_when_a_pages_lines_double() {
    let work = |lines| {
        import_pdf(generated(1, lines), PdfReadLimits::default())
            .expect("import")
            .comparisons
    };
    let (small, large) = (work(400), work(800));
    let ratio = large as f64 / small as f64;
    assert!(
        (1.5..2.6).contains(&ratio),
        "doubling the lines roughly doubled the work; ratio {ratio:.2} ({small} -> {large}). \
         A search against every block would be near 4."
    );
}

/// Doubling the pages doubles the work and the output.
#[test]
fn import_work_doubles_when_the_pages_double() {
    let run = |pages| import_pdf(generated(pages, 40), PdfReadLimits::default()).expect("import");
    let (small, large) = (run(20), run(40));
    let ratio = large.comparisons as f64 / small.comparisons as f64;
    assert!((1.8..2.2).contains(&ratio), "ratio {ratio:.2}");
    assert_eq!(
        large.document.definitions().sections.len(),
        2 * small.document.definitions().sections.len()
    );
}

#[test]
fn the_even_odd_rule_reaches_the_model() {
    use casual_doc_model::v1::{FillRule, GroupChild};
    let imported = import("synthetic-vector.pdf");
    let rules: Vec<FillRule> = pages(&imported.document)[0]
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            InlineNode::Group(group) => Some(group),
            _ => None,
        })
        .flat_map(|group| group.children.iter())
        .filter_map(|child| match child {
            GroupChild::Shape(shape) => shape.path.as_ref().map(|g| g.fill_rule),
            _ => None,
        })
        .collect();
    assert_eq!(
        rules.iter().filter(|r| **r == FillRule::EvenOdd).count(),
        1,
        "exactly the donut is even-odd: {rules:?}"
    );
}

#[test]
fn an_embedded_program_with_a_cmap_is_registered_and_its_bytes_carried() {
    use casual_doc_model::v1::FontProgramFormat;
    let imported = import("synthetic-embedded-truetype.pdf");
    let programs = &imported.document.definitions().font_programs;
    assert_eq!(programs.len(), 1);
    assert_eq!(programs[0].family, "Caladea");
    assert_eq!(programs[0].format, FontProgramFormat::TrueType);
    assert_eq!(programs[0].source_name.as_deref(), Some("Caladea-Regular"));
    let bytes = imported
        .resources
        .get(&programs[0].part_name)
        .expect("bytes");
    assert!(bytes.starts_with(b"\x00\x01\x00\x00"), "unobfuscated sfnt");
    assert!(!imported.report.has(report::FONT_SUBSTITUTED));
    // The runs name the registered family.
    let frames = frames(pages(&imported.document)[0]);
    assert_eq!(frames[0], "Embedded Caladea");
}

#[test]
fn a_subset_without_a_usable_cmap_is_substituted_not_registered() {
    // Our own writer's subsets carry no Unicode cmap (P0): registering one
    // would draw .notdef where a substitute draws the letters.
    let imported = import("writer-real-producer-rich.pdf");
    assert!(imported.document.definitions().font_programs.is_empty());
    assert!(imported.report.has(report::FONT_SUBSTITUTED));
}
