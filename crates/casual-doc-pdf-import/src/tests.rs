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
                // Its Launch link is not navigable: not imported, reported.
                report::LINK_UNRESOLVED,
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

/// A PDF drawing "Habe" with a bare CFF program (`FontFile3` / `Type1C`) and
/// again with a Type1 program (`FontFile`), both Caladea's own outlines
/// (`fonts::test_support`); the PDF widths are the advances times
/// `widths_scale`.
fn wrapped_fonts_pdf(widths_scale: f64) -> Vec<u8> {
    use crate::fonts::test_support::{bare_cff, glyphs, type1};
    use crate::synth::{Page, Pdf, document_in, text};
    let chars = "Habe";
    let advances = glyphs(chars);
    let widths: Vec<String> = (32u8..=126)
        .map(|code| {
            let c = char::from(code);
            let w = advances
                .iter()
                .find(|g| g.1 == Some(c))
                .map_or(0.0, |g| f64::from(g.2) * widths_scale);
            format!("{}", w.round() as i64)
        })
        .collect();
    let widths = widths.join(" ");
    let mut pdf = Pdf::default();
    let cff = pdf.stream("/Subtype /Type1C", &bare_cff(chars));
    let t1 = type1(chars);
    let marker = b"currentfile eexec\n";
    let clear = t1.windows(marker.len()).position(|w| w == marker).unwrap() + marker.len();
    let zeros = t1.windows(64).position(|w| w == [b'0'; 64]).unwrap();
    let t1_file = pdf.stream(
        &format!(
            "/Length1 {clear} /Length2 {} /Length3 {}",
            zeros - clear,
            t1.len() - zeros
        ),
        &t1,
    );
    let mut fonts = String::from("/Font << ");
    for (name, key, file, entry) in [
        ("SynthCff", "C1", cff, "FontFile3"),
        ("SynthType1", "C2", t1_file, "FontFile"),
    ] {
        let descriptor = pdf.add(format!(
            "<< /Type /FontDescriptor /FontName /{name} /Flags 34 /FontBBox [-200 -250 1200 950] /ItalicAngle 0 /Ascent 900 /Descent -250 /CapHeight 650 /StemV 80 /{entry} {file} 0 R >>"
        ));
        let font = pdf.add(format!(
            "<< /Type /Font /Subtype /Type1 /BaseFont /{name} /FirstChar 32 /LastChar 126 /Widths [{widths}] /Encoding /WinAnsiEncoding /FontDescriptor {descriptor} 0 R >>"
        ));
        fonts.push_str(&format!("/{key} {font} 0 R "));
    }
    fonts.push_str(">>");
    let content = format!(
        "{}{}",
        text("C1", 24.0, 72.0, 700.0, 28.0, &["Habe"]),
        text("C2", 24.0, 72.0, 640.0, 28.0, &["Habe"]),
    );
    document_in(&mut pdf, &[Page::new(content)], &fonts, "", |_| {
        String::new()
    })
}

#[test]
fn bare_cff_and_type1_programs_are_made_opentype_and_register_for_layout() {
    use casual_doc_model::v1::FontProgramFormat;
    let imported = import_pdf(wrapped_fonts_pdf(1.0), PdfReadLimits::default()).unwrap();
    let programs = &imported.document.definitions().font_programs;
    assert_eq!(
        programs
            .iter()
            .map(|p| (p.family.as_str(), p.format))
            .collect::<Vec<_>>(),
        vec![
            ("SynthCff", FontProgramFormat::OpenTypeCff),
            ("SynthType1", FontProgramFormat::OpenTypeCff),
        ],
        "{:?}",
        imported.report.findings
    );
    assert_eq!(
        imported
            .report
            .get(report::FONT_EMBEDDED)
            .unwrap()
            .occurrences,
        2
    );
    assert!(!imported.report.has(report::FONT_SUBSTITUTED));
    for program in programs {
        assert!(imported.resources[&program.part_name].starts_with(b"OTTO"));
    }
    // The layout engine registers both (it reports a bare CFF as
    // `unsupported-format`, so registering is the proof of the wrap).
    let shaper = casual_doc_layout::shape::ParleyShaper::new();
    let outcome = casual_doc_layout::font_registry::register_embedded_fonts(
        &shaper,
        &imported.document,
        |part| imported.resources.get(part).map(Vec::as_slice),
    );
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    assert_eq!(outcome.faces, 2);
}

#[test]
fn a_wrapped_program_whose_advances_contradict_the_pdf_is_substituted() {
    // The PDF says every glyph is 40% wider than the program draws it: the
    // names cannot be trusted to name the drawn characters.
    let imported = import_pdf(wrapped_fonts_pdf(1.4), PdfReadLimits::default()).unwrap();
    assert!(imported.document.definitions().font_programs.is_empty());
    assert_eq!(
        imported
            .report
            .get(report::FONT_SUBSTITUTED)
            .unwrap()
            .occurrences,
        2
    );
}

#[test]
fn a_subset_without_a_usable_cmap_is_substituted_not_registered() {
    // Our own writer's subsets carry no Unicode cmap (P0): registering one
    // would draw .notdef where a substitute draws the letters.
    let imported = import("writer-real-producer-rich.pdf");
    assert!(imported.document.definitions().font_programs.is_empty());
    assert!(imported.report.has(report::FONT_SUBSTITUTED));
}

/// The turned frames on a page: `(text, rotation in 60000ths)` of every
/// `GroupTextBox` that carries a rotation.
fn turned_frames(paragraph: &casual_doc_model::v1::Paragraph) -> Vec<(String, i32)> {
    let mut out = Vec::new();
    for inline in &paragraph.inlines {
        let InlineNode::Group(group) = inline else {
            continue;
        };
        for child in &group.children {
            if let casual_doc_model::v1::GroupChild::TextBox(text_box) = child
                && let Some(rotation) = text_box.rotation
            {
                let text: String = text_box
                    .blocks
                    .iter()
                    .filter_map(|b| match b {
                        BlockNode::Paragraph(p) => Some(p),
                        _ => None,
                    })
                    .flat_map(|p| p.inlines.iter())
                    .filter_map(|inline| match inline {
                        InlineNode::Run(run) => Some(run.text.clone()),
                        _ => None,
                    })
                    .collect();
                out.push((text, rotation));
            }
        }
    }
    out
}

/// G-3 (`docs/182` §16b): rotated text is a frame turned by its angle — real,
/// editable text, not an upright stand-in — and a `/Rotate 90` page is a
/// landscape section whose text is turned with it.
#[test]
fn rotated_text_becomes_turned_frames_and_a_rotated_page_is_landscape() {
    let imported = import("synthetic-rotated-text.pdf");
    let page = pages(&imported.document)[0];
    let turned = turned_frames(page);
    let rotations: Vec<i32> = turned.iter().map(|(_, r)| *r).collect();
    assert_eq!(rotations, [-5_400_000, -1_800_000], "{turned:?}");
    assert!(turned.iter().all(|(text, _)| !text.trim().is_empty()));
    // Mapped, not a loss.
    let finding = imported
        .report
        .findings
        .iter()
        .find(|f| f.feature == report::TEXT_ROTATED)
        .expect("reported as the importer's choice");
    assert_eq!(finding.outcome, report::PdfModelOutcome::Mapped);

    let imported = import("synthetic-page-boxes.pdf");
    let sections = &imported.document.definitions().sections;
    let orientations: Vec<_> = sections.iter().map(|s| s.orientation).collect();
    assert_eq!(
        orientations,
        [None, Some(casual_doc_model::v1::PageOrientation::Landscape)],
        "the /Rotate 90 page is landscape"
    );
    let page = pages(&imported.document)[1];
    assert!(frames(page).is_empty(), "no upright stand-in remains");
    assert!(
        !turned_frames(page).is_empty(),
        "the rotated page's text is turned with it"
    );
}

/// On-curve points of `c`'s outline at a 1000-unit em, through skrifa.
fn on_curve_points(font: &[u8], c: char) -> Vec<(i32, i32)> {
    use skrifa::MetadataProvider;
    use skrifa::outline::{DrawSettings, OutlinePen};
    use skrifa::prelude::{LocationRef, Size};
    #[derive(Default)]
    struct Points(Vec<(i32, i32)>);
    impl OutlinePen for Points {
        fn move_to(&mut self, x: f32, y: f32) {
            self.0.push((x.round() as i32, y.round() as i32));
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.0.push((x.round() as i32, y.round() as i32));
        }
        fn quad_to(&mut self, _: f32, _: f32, x: f32, y: f32) {
            self.0.push((x.round() as i32, y.round() as i32));
        }
        fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, x: f32, y: f32) {
            self.0.push((x.round() as i32, y.round() as i32));
        }
        fn close(&mut self) {}
    }
    let face = skrifa::FontRef::new(font).expect("an sfnt");
    let gid = face.charmap().map(c).expect("mapped");
    let mut points = Points::default();
    face.outline_glyphs()
        .get(gid)
        .expect("an outline")
        .draw(
            DrawSettings::unhinted(Size::new(1000.0), LocationRef::default()),
            &mut points,
        )
        .expect("draws");
    points.0
}

/// A character, its CID and its advance in 1000 units.
type CjkGlyph = (char, u16, i64);

/// The CJK test subset (a CID-keyed CFF inside an OTF), and three of its
/// characters with their CIDs and advances in 1000 units.
fn cjk_subset() -> (Vec<u8>, Vec<u8>, Vec<CjkGlyph>) {
    use skrifa::MetadataProvider;
    use skrifa::raw::TableProvider;
    let otf = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/fonts/NotoSansCJK-TestSubset.otf"),
    )
    .expect("the CJK test subset");
    let face = skrifa::FontRef::new(&otf).unwrap();
    let cff = face
        .table_data(skrifa::raw::types::Tag::new(b"CFF "))
        .unwrap()
        .as_bytes()
        .to_vec();
    let program = skrifa::raw::ps::cff::CffFontRef::new_cff(&cff, 0, None).unwrap();
    assert!(program.is_cid());
    let charset = program.charset().unwrap();
    let upem = f64::from(face.head().unwrap().units_per_em());
    let metrics = face.glyph_metrics(
        skrifa::instance::Size::unscaled(),
        skrifa::instance::LocationRef::default(),
    );
    let chars: Vec<(char, u16, i64)> = face
        .charmap()
        .mappings()
        .filter(|(c, _)| *c > 0x3000)
        .take(3)
        .map(|(c, gid)| {
            let cid = charset.string_id(gid).unwrap().to_u16();
            let w = metrics.advance_width(gid).unwrap_or(0.0);
            (
                char::from_u32(c).unwrap(),
                cid,
                (f64::from(w) * 1000.0 / upem).round() as i64,
            )
        })
        .collect();
    assert_eq!(chars.len(), 3);
    (otf, cff, chars)
}

/// A Type0 font over a CID-keyed CFF (`FontFile3` / `CIDFontType0C`),
/// Identity-H, with a ToUnicode CMap, drawing the subset's three characters.
fn cid_cff_pdf() -> Vec<u8> {
    use crate::synth::{Page, Pdf, document_in};
    let (_, cff, chars) = cjk_subset();
    let mut pdf = Pdf::default();
    let file = pdf.stream("/Subtype /CIDFontType0C", &cff);
    let bfchar: String = chars
        .iter()
        .map(|(c, cid, _)| format!("<{cid:04X}> <{:04X}>\n", u32::from(*c)))
        .collect();
    let cmap = format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n{} beginbfchar\n{bfchar}endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n",
        chars.len()
    );
    let to_unicode = pdf.stream("", cmap.as_bytes());
    let descriptor = pdf.add(format!(
        "<< /Type /FontDescriptor /FontName /SynthCjk /Flags 4 /FontBBox [-1000 -300 2000 1100] /ItalicAngle 0 /Ascent 880 /Descent -120 /CapHeight 700 /StemV 80 /FontFile3 {file} 0 R >>"
    ));
    let widths: String = chars
        .iter()
        .map(|(_, cid, w)| format!("{cid} [{w}] "))
        .collect();
    let descendant = pdf.add(format!(
        "<< /Type /Font /Subtype /CIDFontType0 /BaseFont /SynthCjk /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /W [{widths}] >>"
    ));
    let font = pdf.add(format!(
        "<< /Type /Font /Subtype /Type0 /BaseFont /SynthCjk /Encoding /Identity-H /DescendantFonts [{descendant} 0 R] /ToUnicode {to_unicode} 0 R >>"
    ));
    let codes: String = chars
        .iter()
        .map(|(_, cid, _)| format!("{cid:04X}"))
        .collect();
    let content = format!("BT /K1 24 Tf 72 700 Td <{codes}> Tj ET\n");
    document_in(
        &mut pdf,
        &[Page::new(content)],
        &format!("/Font << /K1 {font} 0 R >>"),
        "",
        |_| String::new(),
    )
}

#[test]
fn a_cid_keyed_cff_is_wrapped_from_the_glyphs_the_page_drew() {
    use casual_doc_model::v1::FontProgramFormat;
    let (otf, _, chars) = cjk_subset();
    let imported = import_pdf(cid_cff_pdf(), PdfReadLimits::default()).unwrap();
    let programs = &imported.document.definitions().font_programs;
    assert_eq!(
        programs.iter().map(|p| p.format).collect::<Vec<_>>(),
        vec![FontProgramFormat::OpenTypeCff],
        "{:?}",
        imported.report.findings
    );
    assert!(!imported.report.has(report::FONT_SUBSTITUTED));
    let wrapped = &imported.resources[&programs[0].part_name];
    for (c, _, _) in &chars {
        assert_eq!(
            on_curve_points(wrapped, *c),
            on_curve_points(&otf, *c),
            "{c:?} draws with the producer's own outline"
        );
    }
    let shaper = casual_doc_layout::shape::ParleyShaper::new();
    let outcome = casual_doc_layout::font_registry::register_embedded_fonts(
        &shaper,
        &imported.document,
        |part| imported.resources.get(part).map(Vec::as_slice),
    );
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    assert_eq!(outcome.faces, 1);
}

// --- Special characters (`docs/182` §18.8) ----------------------------------

/// Every run's text in the body, tables and frames included, in order.
fn all_text(document: &Document) -> String {
    fn blocks(list: &[BlockNode], out: &mut String) {
        for block in list {
            match block {
                BlockNode::Paragraph(p) => {
                    for inline in &p.inlines {
                        match inline {
                            InlineNode::Run(run) => out.push_str(&run.text),
                            InlineNode::Break(_) => out.push('\n'),
                            InlineNode::TextBox(frame) => blocks(&frame.blocks, out),
                            InlineNode::Hyperlink(h) => {
                                for inline in &h.inlines {
                                    if let InlineNode::Run(run) = inline {
                                        out.push_str(&run.text);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    out.push('\n');
                }
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            blocks(&cell.blocks, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = String::new();
    blocks(document.body(), &mut out);
    out
}

/// A `ToUnicode` CMap mapping one-byte codes to UTF-16 hex strings.
fn to_unicode(pairs: &[(u8, &str)]) -> Vec<u8> {
    let mut body = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Synth def\n/CMapType 2 def\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n",
    );
    body.push_str(&format!("{} beginbfchar\n", pairs.len()));
    for (code, utf16) in pairs {
        body.push_str(&format!("<{code:02X}> <{utf16}>\n"));
    }
    body.push_str("endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    body.into_bytes()
}

/// One page drawn with fonts whose `ToUnicode` says what producers write:
/// Word's private-use Symbol bullet, a ligature, a replacement character, a
/// combining acute in its own glyph, a soft hyphen at a line end and a
/// no-break space.
fn special_characters_pdf() -> Vec<u8> {
    use crate::synth::{Page, Pdf, document_in};
    let mut pdf = Pdf::default();
    let symbol = pdf.stream("", &to_unicode(&[(0xB7, "F0B7")]));
    let text = pdf.stream(
        "",
        &to_unicode(&[(0x61, "FB01"), (0x62, "FFFD"), (0x63, "0301")]),
    );
    let spaces = pdf.stream("", &to_unicode(&[(0xAD, "00AD"), (0xA0, "00A0")]));
    let fonts = format!(
        "/Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >> /S1 << /Type /Font /Subtype /Type1 /BaseFont /Symbol /ToUnicode {symbol} 0 R >> /T1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding /ToUnicode {text} 0 R >> /T2 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding /ToUnicode {spaces} 0 R >> >>"
    );
    // "• fix olé" (the `b` has no Unicode), then "infor-" / "mation", then
    // "10 kg" with a no-break space, all at 12 pt. In `T1` the codes `a`,
    // `b` and `c` are the ligature, the replacement and the acute.
    let content = "BT /S1 12 Tf 72 700 Td (\\267) Tj ET\n\
        BT /T1 12 Tf 90 700 Td (ax olec b) Tj ET\n\
        BT /T2 12 Tf 72 660 Td 14 TL (infor\\255) Tj T* (mation follows here) Tj ET\n\
        BT /T2 12 Tf 72 600 Td (10\\240kg) Tj ET\n";
    document_in(&mut pdf, &[Page::new(content)], &fonts, "", |_| {
        String::new()
    })
}

/// The reader made the text fit for editing, and each family is reported.
/// Mutations, each driven red: return the text unchanged from
/// `unicode::normalize` (the bullet reads U+F0B7, the ligature U+FB01);
/// keep U+FFFD in `group.rs` (`\u{fffd}` is in the text); skip
/// `compose_run` (the acute stays a combining mark after `e`).
#[test]
fn special_characters_are_made_fit_for_editing_in_both_layouts() {
    let imported = import_pdf(special_characters_pdf(), PdfReadLimits::default()).unwrap();
    let fixed = all_text(&imported.document);
    let flowing = imported.flowing().unwrap();
    let flowing_text = all_text(&flowing.document);
    for (layout, text) in [("fixed", &fixed), ("flowing", &flowing_text)] {
        assert!(text.contains('\u{2022}'), "{layout}: the bullet: {text:?}");
        assert!(
            text.contains("fix"),
            "{layout}: the ligature's letters: {text:?}"
        );
        assert!(text.contains("ol\u{e9}"), "{layout}: composed: {text:?}");
        assert!(
            text.contains("10\u{a0}kg"),
            "{layout}: the no-break space: {text:?}"
        );
        for bad in ['\u{fffd}', '\u{f0b7}', '\u{fb01}', '\u{301}'] {
            assert!(!text.contains(bad), "{layout}: {bad:?} in {text:?}");
        }
    }
    // A soft hyphen the PDF drew stays visible on the fixed page and joins
    // the word in the flowing document.
    assert!(fixed.contains("infor-"), "{fixed:?}");
    assert!(!fixed.contains('\u{ad}'), "{fixed:?}");
    assert!(flowing_text.contains("information"), "{flowing_text:?}");
    for (report, layout) in [(&imported.report, "fixed"), (&flowing.report, "flowing")] {
        for feature in [
            report::SYMBOL_FONT_MAPPED,
            report::LIGATURE_DECOMPOSED,
            report::MARKS_COMPOSED,
            report::NATIVE_TEXT_INVALID,
        ] {
            assert!(
                report.has(feature),
                "{layout}: {feature}: {:?}",
                report.findings
            );
        }
    }
    assert!(flowing.report.has(report::LAYOUT_INFERRED));
    assert!(flowing.report.has(report::EXACT_SAVE_FIXED_LAYOUT_ONLY));
}

/// A font whose `ToUnicode` flattens its curly apostrophe to `'`: the glyph's
/// name in the embedded program (`quoteright`) restores it. Mutation: return
/// 0 from `QuoteNames::correct` without changing anything and the text reads
/// `H'e`.
#[test]
fn a_straight_quote_the_glyph_names_curly_is_imported_curly() {
    use crate::fonts::test_support::{bare_cff, glyphs};
    use crate::synth::{Page, Pdf, document_in};
    let chars = "H\u{2019}e";
    let advances = glyphs(chars);
    let width = |c: char| {
        advances
            .iter()
            .find(|g| g.1 == Some(c))
            .map_or(0, |g| i64::from(g.2))
    };
    // WinAnsi: 0x48 H, 0x65 e, 0x92 quoteright.
    let widths: Vec<String> = (32u32..=0x92)
        .map(|code| match code {
            0x48 => width('H'),
            0x65 => width('e'),
            0x92 => width('\u{2019}'),
            _ => 0,
        })
        .map(|w| w.to_string())
        .collect();
    let mut pdf = Pdf::default();
    let cff = pdf.stream("/Subtype /Type1C", &bare_cff(chars));
    let map = pdf.stream("", &to_unicode(&[(0x92, "0027")]));
    let descriptor = pdf.add(format!(
        "<< /Type /FontDescriptor /FontName /SynthQuote /Flags 34 /FontBBox [-200 -250 1200 950] /ItalicAngle 0 /Ascent 900 /Descent -250 /CapHeight 650 /StemV 80 /FontFile3 {cff} 0 R >>"
    ));
    let font = pdf.add(format!(
        "<< /Type /Font /Subtype /Type1 /BaseFont /SynthQuote /FirstChar 32 /LastChar 146 /Widths [{}] /Encoding /WinAnsiEncoding /FontDescriptor {descriptor} 0 R /ToUnicode {map} 0 R >>",
        widths.join(" ")
    ));
    let fonts = format!("/Font << /Q1 {font} 0 R >>");
    let bytes = document_in(
        &mut pdf,
        &[Page::new("BT /Q1 24 Tf 72 700 Td (H\\222e) Tj ET\n")],
        &fonts,
        "",
        |_| String::new(),
    );
    let imported = import_pdf(bytes, PdfReadLimits::default()).unwrap();
    let text = all_text(&imported.document);
    assert!(text.contains("H\u{2019}e"), "{text:?}");
    assert!(
        imported.report.has(report::QUOTE_FROM_GLYPH_NAME),
        "{:?}",
        imported.report.findings
    );
}
