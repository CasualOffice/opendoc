//! OCR import (`docs/182` §20): classification, native-first merge, budgets,
//! cancel, and — when the host's models are present — real recognition of a
//! synthetic scan.
//!
//! Most tests use a **scripted provider** that returns fixed words at fixed
//! page positions, so the merge rules are tested without a model, a download
//! or the network. The model tests read the published `ocrs` weights from
//! `OPENDOC_OCR_MODELS` (a directory holding `text-detection.rten` and
//! `text-recognition.rten`) and the heron layout model from
//! `OPENDOC_LAYOUT_MODEL` (a `model.onnx` path); without them they print why
//! and pass. `crates/casual-doc-ocr/fetch-test-models.sh` downloads and
//! verifies both. CI does not set either variable, so CI never needs the
//! network for this file.

#![allow(clippy::print_stderr)]

use std::io::Write as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use casual_doc_model::v1::{BlockNode, Document, InlineNode};
use casual_doc_ocr::{
    CancelToken, HeuristicLayout, Interrupt, LayoutProvider, OcrBox, OcrError, OcrLimits, OcrLine,
    OcrPage, OcrProvider, OcrWord, OcrsProvider, PaddleOcr, ProviderManifest, Raster,
    RecognitionPack, RegionKind, Stop,
};
use casual_doc_pdf_import::report;
use casual_doc_pdf_import::{
    ImportJob, OcrOptions, PageClass, PdfImport, PdfImportError, PdfReadLimits, Progress,
    import_pdf, import_pdf_with_ocr,
};

// ---------------------------------------------------------------- fixtures

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/pdf")
            .join(name),
    )
    .expect(name)
}

/// Renders page `index` of `pdf` at `dpi` to 8-bit grey with hayro.
fn render_grey(pdf: &[u8], index: usize, dpi: f32) -> (u32, u32, Vec<u8>) {
    let pdf = hayro::hayro_syntax::Pdf::new(pdf.to_vec()).expect("hayro opens it");
    let pages = pdf.pages();
    let page = pages.get(index).expect("page");
    let white = hayro::vello_cpu::color::AlphaColor::from_rgba8(255, 255, 255, 255);
    let pixmap = hayro::render(
        page,
        &hayro::RenderCache::new(),
        &hayro::hayro_interpret::InterpreterSettings::default(),
        &hayro::RenderSettings::default(),
        &hayro::PixmapSettings {
            x_scale: dpi / 72.0,
            y_scale: dpi / 72.0,
            bg_color: white,
        },
    );
    let grey = pixmap
        .data()
        .iter()
        .map(|p| {
            let (r, g, b) = (u32::from(p.r), u32::from(p.g), u32::from(p.b));
            ((r * 299 + g * 587 + b * 114) / 1000) as u8
        })
        .collect();
    (u32::from(pixmap.width()), u32::from(pixmap.height()), grey)
}

/// A minimal PDF writer for test files: objects, then a correct xref.
fn write_pdf(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        writeln!(out, "{} 0 obj", i + 1).unwrap();
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    write!(out, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).unwrap();
    for offset in offsets {
        writeln!(out, "{offset:010} 00000 n ").unwrap();
    }
    write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    )
    .unwrap();
    out
}

fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
    let mut out = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
    out.extend_from_slice(data);
    out.extend_from_slice(b"\nendstream");
    out
}

/// An image-only PDF: each page is one full-page grey picture, optionally
/// with `overlay` content (native text) drawn over it with Helvetica as F1.
fn scan_pdf(pages: &[(u32, u32, Vec<u8>)], overlay: &str) -> Vec<u8> {
    let count = pages.len();
    // 1 catalog, 2 pages, 3 font, then per page: page, image, content.
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {count} >>",
            (0..count)
                .map(|i| format!("{} 0 R", 4 + i * 3))
                .collect::<Vec<_>>()
                .join(" ")
        )
        .into_bytes(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    ];
    for (i, (w, h, grey)) in pages.iter().enumerate() {
        let page_id = 4 + i * 3;
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(6));
        encoder.write_all(grey).unwrap();
        let packed = encoder.finish().unwrap();
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Im0 {} 0 R >> /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                page_id + 1,
                page_id + 2
            )
            .into_bytes(),
        );
        objects.push(stream(
            &format!(
                "/Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode"
            ),
            &packed,
        ));
        let content = format!("q 612 0 0 792 0 0 cm /Im0 Do Q\n{overlay}");
        objects.push(stream("", content.as_bytes()));
    }
    write_pdf(&objects)
}

/// A blank page as a "scan" (white pixels), for the scripted provider.
fn blank_scan(pages: usize, overlay: &str) -> Vec<u8> {
    let (w, h) = (306u32, 396u32);
    let grey = vec![255u8; (w * h) as usize];
    scan_pdf(&vec![(w, h, grey); pages], overlay)
}

/// Every text-box run's text on every page, frames separated by newlines.
fn text_of(document: &Document) -> String {
    let mut out = String::new();
    for block in document.body() {
        let BlockNode::Paragraph(page) = block else {
            continue;
        };
        for inline in &page.inlines {
            let InlineNode::TextBox(text_box) = inline else {
                continue;
            };
            for block in &text_box.blocks {
                let BlockNode::Paragraph(p) = block else {
                    continue;
                };
                for inline in &p.inlines {
                    match inline {
                        InlineNode::Run(run) => out.push_str(&run.text),
                        InlineNode::Break(_) => out.push('\n'),
                        _ => {}
                    }
                }
            }
            out.push('\n');
        }
    }
    out
}

fn pictures(document: &Document) -> usize {
    document
        .body()
        .iter()
        .filter_map(|b| match b {
            BlockNode::Paragraph(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.inlines.iter())
        .filter(|i| matches!(i, InlineNode::AnchoredDrawing(_)))
        .count()
}

// ------------------------------------------------------- scripted provider

/// Returns fixed words given in PAGE POINTS, mapped into the raster it is
/// handed: by default a full-page raster of a 612-point-wide page; with
/// `region`, a raster of that region (origin, pixels per point) — the
/// importer's own mapping, so the words land back where they were given.
/// Records how many rasters it saw.
struct Scripted {
    words: Vec<(&'static str, [f32; 4], f32)>,
    region: Option<((f32, f32), f32)>,
    calls: AtomicU32,
    /// Cancels this token on the first call (a host pressing Cancel while a
    /// page is being read).
    cancel_on_call: Option<CancelToken>,
    /// Reports every line turned by this many degrees about its own centre
    /// (the given box is then its upright frame), as a provider does for a
    /// skewed or upside-down line.
    turn: Option<f32>,
}

impl Scripted {
    fn new(words: Vec<(&'static str, [f32; 4], f32)>) -> Self {
        Self {
            words,
            region: None,
            calls: AtomicU32::new(0),
            cancel_on_call: None,
            turn: None,
        }
    }
}

impl OcrProvider for Scripted {
    fn manifest(&self) -> ProviderManifest {
        ProviderManifest {
            provider: "scripted",
            version: "test",
            models: Vec::new(),
            decoding: "fixed",
            determinism: casual_doc_ocr::Determinism::Exact,
        }
    }

    fn recognize(
        &self,
        raster: &Raster,
        _limits: &OcrLimits,
        interrupt: &dyn Interrupt,
    ) -> Result<OcrPage, OcrError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(token) = &self.cancel_on_call {
            token.cancel();
        }
        if interrupt.poll() == Some(Stop::Cancelled) {
            return Err(OcrError::Cancelled);
        }
        let ((ox, oy), s) = self
            .region
            .unwrap_or(((0.0, 0.0), raster.width() as f32 / 612.0));
        let lines = self
            .words
            .iter()
            .map(|(text, [x0, y0, x1, y1], confidence)| {
                let bounds = OcrBox {
                    x0: (x0 - ox) * s,
                    y0: (y0 - oy) * s,
                    x1: (x1 - ox) * s,
                    y1: (y1 - oy) * s,
                };
                let mut line = OcrLine::upright(
                    vec![OcrWord {
                        text: (*text).to_owned(),
                        bounds,
                        confidence: *confidence,
                    }],
                    bounds,
                );
                if let Some(turn) = self.turn {
                    line.rotation_degrees = turn;
                    line.pivot = [(bounds.x0 + bounds.x1) / 2.0, (bounds.y0 + bounds.y1) / 2.0];
                }
                line
            })
            .collect();
        Ok(OcrPage {
            lines,
            ..OcrPage::default()
        })
    }
}

fn with(provider: Arc<dyn OcrProvider>) -> OcrOptions {
    OcrOptions {
        provider: Some(provider),
        dpi: 72.0 * 1.5,
        ..OcrOptions::default()
    }
}

// ------------------------------------------------------------ no provider

#[test]
fn with_no_model_a_scan_reports_ocr_unavailable_and_still_imports() {
    let imported = import_pdf(blank_scan(2, ""), PdfReadLimits::default()).expect("imports");
    let finding = imported
        .report
        .get(report::OCR_UNAVAILABLE)
        .expect("OCR unavailable is reported, not skipped silently");
    assert_eq!(finding.occurrences, 2, "one region per scanned page");
    assert_eq!(finding.first_page, Some(0));
    assert_eq!(
        imported.ocr.classes,
        vec![Some(PageClass::Raster), Some(PageClass::Raster)]
    );
    assert_eq!(imported.ocr.provider, None);
    assert_eq!(
        imported.ocr.pixels, 0,
        "nothing is rasterised without a model"
    );
    assert_eq!(pictures(&imported.document), 2, "the scans stay pictures");
    assert!(text_of(&imported.document).is_empty());
}

// The editor starts loading its recognition models the moment a step reads a
// scan (`docs/182` §17a.5), so the count must move page by page, before
// `finish`. Mutation: count only `PageClass::Native` and every assertion on a
// scan reads 0.
#[test]
fn scanned_pages_counts_scans_as_each_step_reads_them() {
    let mut job = ImportJob::begin(blank_scan(2, ""), PdfReadLimits::default()).unwrap();
    assert_eq!(job.scanned_pages(), 0, "nothing is read before a step");
    assert!(matches!(job.step(1), Progress::Pending { .. }));
    assert_eq!(
        job.scanned_pages(),
        1,
        "the first scan is known after its step"
    );
    assert_eq!(job.step(1), Progress::Done);
    assert_eq!(job.scanned_pages(), 2);
    let mut native =
        ImportJob::begin(fixture("synthetic-prose.pdf"), PdfReadLimits::default()).unwrap();
    assert_eq!(native.step(usize::MAX), Progress::Done);
    assert_eq!(native.scanned_pages(), 0, "a born-digital PDF has none");
}

#[test]
fn a_born_digital_page_is_native_and_never_rasterised() {
    let scripted = Arc::new(Scripted::new(vec![(
        "ghost",
        [72.0, 72.0, 120.0, 84.0],
        0.99,
    )]));
    let imported = import_pdf_with_ocr(
        fixture("synthetic-prose.pdf"),
        PdfReadLimits::default(),
        with(scripted.clone()),
    )
    .expect("imports");
    assert_eq!(
        imported.ocr.classes,
        vec![Some(PageClass::Native), Some(PageClass::Native)]
    );
    assert_eq!(scripted.calls.load(Ordering::SeqCst), 0, "PDFR-I2");
    assert_eq!(imported.ocr.pixels, 0);
    let plain = import_pdf(fixture("synthetic-prose.pdf"), PdfReadLimits::default()).unwrap();
    assert_eq!(
        imported.document, plain.document,
        "a provider changes nothing on a native page"
    );
    assert!(!imported.report.has(report::OCR_UNAVAILABLE));
}

// ------------------------------------------------------- native-first merge

#[test]
fn native_text_is_never_overwritten_by_ocr() {
    // A scan with live Helvetica "Hello" over it at (72, 700) in PDF space,
    // i.e. top 92 in page space. The provider claims that place reads
    // "Mango" (a disagreement) and "Hallo" in an overlapping box (one slip,
    // not a disagreement), and that a third place reads "World".
    let overlay = "BT /F1 12 Tf 72 700 Td (Hello) Tj ET\n";
    let scripted = Arc::new(Scripted::new(vec![
        ("Mango", [72.0, 82.0, 101.0, 95.0], 0.99),
        ("Hallo", [73.0, 82.0, 100.0, 95.0], 0.99),
        ("World", [72.0, 300.0, 110.0, 313.0], 0.99),
    ]));
    let imported = import_pdf_with_ocr(
        blank_scan(1, overlay),
        PdfReadLimits::default(),
        with(scripted.clone()),
    )
    .expect("imports");
    assert_eq!(imported.ocr.classes, vec![Some(PageClass::Hybrid)]);
    assert_eq!(scripted.calls.load(Ordering::SeqCst), 1);
    let text = text_of(&imported.document);
    assert!(text.contains("Hello"), "native text kept: {text:?}");
    assert!(
        !text.contains("Mango") && !text.contains("Hallo"),
        "OCR must not overwrite native text: {text:?}"
    );
    assert!(text.contains("World"), "raster-only text is read: {text:?}");
    assert_eq!(
        imported
            .report
            .get(report::NATIVE_OCR_CONFLICT)
            .map(|f| f.occurrences),
        Some(1),
        "the disagreement is reported: {:?}",
        imported.report.findings
    );
    assert_eq!(imported.ocr.words_shadowed, 2);
    assert_eq!(imported.ocr.words_kept, 1);
    assert!(imported.report.has(report::OCR_APPLIED));
}

#[test]
fn ocr_replaces_glyphs_whose_unicode_is_unrecoverable_and_nothing_else() {
    // synthetic-type3.pdf: Helvetica text at the top, and "ABABAB" drawn
    // with a Type3 font that has no ToUnicode at baseline 600 (page-space
    // baseline 192, 24 pt). The scripted reader "recovers" it as "ABABAB",
    // and claims to read the Helvetica line as something else.
    // The region the importer renders: the run's box padded by 0.3 em, at
    // 40 px per em (24 pt -> 5/3 px per point).
    let mut scripted = Scripted::new(vec![
        ("ABABAB", [72.0, 172.0, 216.0, 197.0], 0.95),
        ("Nonsense", [72.0, 62.0, 200.0, 75.0], 0.95),
    ]);
    scripted.region = Some(((72.0 - 7.2, 172.8 - 7.2), 40.0 / 24.0));
    let scripted = Arc::new(scripted);
    let plain = import_pdf(fixture("synthetic-type3.pdf"), PdfReadLimits::default()).unwrap();
    let plain_text = text_of(&plain.document);
    // Without OCR the glyphs with no Unicode are reported, never U+FFFD in
    // the text (`docs/182` §18.8).
    assert!(!plain_text.contains('\u{fffd}'), "{plain_text:?}");
    assert!(!plain_text.contains("ABABAB"), "{plain_text:?}");
    assert!(plain.report.has(report::NATIVE_TEXT_INVALID));
    let imported = import_pdf_with_ocr(
        fixture("synthetic-type3.pdf"),
        PdfReadLimits::default(),
        with(scripted),
    )
    .expect("imports");
    assert_eq!(imported.ocr.classes, vec![Some(PageClass::Hybrid)]);
    let text = text_of(&imported.document);
    assert!(text.contains("ABABAB"), "{text:?}");
    assert!(
        !text.contains('\u{fffd}'),
        "the unmapped glyphs are replaced: {text:?}"
    );
    assert!(text.contains("Type3 glyphs without ToUnicode"), "{text:?}");
    assert!(!text.contains("Nonsense"), "{text:?}");
    assert_eq!(
        imported
            .report
            .get(report::NATIVE_TEXT_INVALID)
            .map(|f| f.occurrences),
        plain
            .report
            .get(report::NATIVE_TEXT_INVALID)
            .map(|f| f.occurrences),
        "the producer's defect is still reported once per glyph"
    );
}

#[test]
fn low_confidence_words_are_kept_and_reported_and_the_floor_drops_them() {
    let scripted = Arc::new(Scripted::new(vec![
        ("sure", [72.0, 100.0, 110.0, 113.0], 0.97),
        ("unsure", [72.0, 200.0, 120.0, 213.0], 0.5),
        ("noise", [72.0, 300.0, 120.0, 313.0], 0.1),
    ]));
    let imported = import_pdf_with_ocr(blank_scan(1, ""), PdfReadLimits::default(), with(scripted))
        .expect("imports");
    let text = text_of(&imported.document);
    assert!(text.contains("sure") && text.contains("unsure"), "{text:?}");
    assert!(
        !text.contains("noise"),
        "under the floor: dropped: {text:?}"
    );
    assert_eq!(
        imported
            .report
            .get(report::OCR_LOW_CONFIDENCE)
            .map(|f| f.occurrences),
        Some(2),
        "{:?}",
        imported.report.findings
    );
}

#[test]
fn optical_frames_sit_over_the_picture_and_hidden_placement_hides_them() {
    let scripted: Arc<dyn OcrProvider> = Arc::new(Scripted::new(vec![(
        "Above",
        [72.0, 100.0, 120.0, 113.0],
        0.99,
    )]));
    let visible = import_pdf_with_ocr(
        blank_scan(1, ""),
        PdfReadLimits::default(),
        with(scripted.clone()),
    )
    .unwrap();
    let BlockNode::Paragraph(page) = &visible.document.body()[0] else {
        panic!("a page paragraph");
    };
    let z = |want_text: bool| {
        page.inlines
            .iter()
            .find_map(|i| match i {
                InlineNode::TextBox(t) if want_text => t.relative_height,
                InlineNode::AnchoredDrawing(d) if !want_text => d.relative_height,
                _ => None,
            })
            .expect("present")
    };
    assert!(z(true) > z(false), "the frame is above the picture");
    let hidden = import_pdf_with_ocr(
        blank_scan(1, ""),
        PdfReadLimits::default(),
        OcrOptions {
            placement: casual_doc_pdf_import::OcrTextPlacement::Hidden,
            ..with(scripted)
        },
    )
    .unwrap();
    let BlockNode::Paragraph(page) = &hidden.document.body()[0] else {
        panic!("a page paragraph");
    };
    let hidden_runs = page
        .inlines
        .iter()
        .filter_map(|i| match i {
            InlineNode::TextBox(t) => Some(t),
            _ => None,
        })
        .flat_map(|t| t.blocks.iter())
        .filter_map(|b| match b {
            BlockNode::Paragraph(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.inlines.iter())
        .filter(|i| matches!(i, InlineNode::Run(r) if r.properties.hidden == Some(true)))
        .count();
    assert_eq!(hidden_runs, 1, "the optical run is hidden");
}

/// Each turned frame's text and rotation (60,000ths of a degree).
fn turned_texts(document: &Document) -> Vec<(String, i32)> {
    let mut out = Vec::new();
    for block in document.body() {
        let BlockNode::Paragraph(page) = block else {
            continue;
        };
        for inline in &page.inlines {
            let InlineNode::Group(group) = inline else {
                continue;
            };
            for child in &group.children {
                let casual_doc_model::v1::GroupChild::TextBox(text_box) = child else {
                    continue;
                };
                let Some(rotation) = text_box.rotation else {
                    continue;
                };
                let mut text = String::new();
                for block in &text_box.blocks {
                    if let BlockNode::Paragraph(p) = block {
                        for inline in &p.inlines {
                            if let InlineNode::Run(run) = inline {
                                text.push_str(&run.text);
                            }
                        }
                    }
                }
                out.push((text, rotation));
            }
        }
    }
    out
}

#[test]
fn a_line_read_turned_becomes_a_frame_turned_by_the_same_angle() {
    for (turn, want) in [(180.0f32, 180 * 60_000), (3.0, 3 * 60_000)] {
        let scripted = Arc::new(Scripted {
            turn: Some(turn),
            ..Scripted::new(vec![("Turned", [200.0, 300.0, 300.0, 320.0], 0.99)])
        });
        let imported =
            import_pdf_with_ocr(blank_scan(1, ""), PdfReadLimits::default(), with(scripted))
                .unwrap();
        assert_eq!(
            turned_texts(&imported.document),
            vec![("Turned".to_owned(), want)],
            "turn {turn}"
        );
        assert_eq!(imported.ocr.lines_turned, 1);
        // The turned line is not also an upright frame.
        assert!(!text_of(&imported.document).contains("Turned"));
    }
}

#[test]
fn an_upside_down_pages_lines_group_in_reading_order() {
    // Upside down, the first line read sits lower on the page than the
    // second. Turned back about the page centre, it is the top line, so the
    // two join one frame, first line first.
    let scripted = Arc::new(Scripted {
        turn: Some(180.0),
        ..Scripted::new(vec![
            ("Second", [300.0, 480.0, 360.0, 492.0], 0.99),
            ("First", [300.0, 495.0, 360.0, 507.0], 0.99),
        ])
    });
    let imported =
        import_pdf_with_ocr(blank_scan(1, ""), PdfReadLimits::default(), with(scripted)).unwrap();
    let frames = turned_texts(&imported.document);
    assert_eq!(frames.len(), 1, "one frame: {frames:?}");
    assert!(frames[0].0.starts_with("First"), "{frames:?}");
}

/// The cover shapes on each page: their fill colours.
fn covers(document: &Document) -> Vec<[u8; 3]> {
    let mut out = Vec::new();
    for block in document.body() {
        let BlockNode::Paragraph(page) = block else {
            continue;
        };
        for inline in &page.inlines {
            let InlineNode::Group(group) = inline else {
                continue;
            };
            for child in &group.children {
                if let casual_doc_model::v1::GroupChild::Shape(shape) = child
                    && let Some(casual_doc_model::v1::Fill::Solid(c)) = &shape.fill
                {
                    out.push([c.r, c.g, c.b]);
                }
            }
        }
    }
    out
}

/// The z-order (relative height) of the first inline matching `pick`.
fn z_of(document: &Document, pick: impl Fn(&InlineNode) -> Option<u32>) -> u32 {
    let BlockNode::Paragraph(page) = &document.body()[0] else {
        panic!("a page paragraph");
    };
    page.inlines.iter().find_map(pick).expect("present")
}

#[test]
fn visible_optical_text_covers_the_scan_in_the_papers_colour_and_keeps_the_picture() {
    // Off-white paper (230), so a cover in plain white would be visible, and
    // a native stamp painted after the scan, far from the optical line, which
    // the cover must stay under.
    let (w, h) = (306u32, 396u32);
    let paper = scan_pdf(
        &[(w, h, vec![230u8; (w * h) as usize])],
        "BT /F1 10 Tf 400 72 Td (Stamp) Tj ET",
    );
    let scripted: Arc<dyn OcrProvider> = Arc::new(Scripted::new(vec![(
        "Covered",
        [72.0, 100.0, 160.0, 113.0],
        0.99,
    )]));
    let covered = import_pdf_with_ocr(
        paper.clone(),
        PdfReadLimits::default(),
        with(scripted.clone()),
    )
    .unwrap();
    assert_eq!(covers(&covered.document), vec![[230, 230, 230]]);
    assert_eq!(covered.ocr.lines_covered, 1);
    let finding = covered
        .report
        .findings
        .iter()
        .find(|f| f.feature == report::OCR_SCAN_COVERED)
        .expect("the cover is reported");
    assert_eq!(finding.occurrences, 1);
    // Picture, then cover, then text.
    let picture = z_of(&covered.document, |i| match i {
        InlineNode::AnchoredDrawing(d) => d.relative_height,
        _ => None,
    });
    let cover = z_of(&covered.document, |i| match i {
        InlineNode::Group(g) => g.relative_height,
        _ => None,
    });
    let frame = |needle: &'static str| {
        move |i: &InlineNode| match i {
            InlineNode::TextBox(t) if format!("{:?}", t.blocks).contains(needle) => {
                t.relative_height
            }
            _ => None,
        }
    };
    let text = z_of(&covered.document, frame("Covered"));
    let stamp = z_of(&covered.document, frame("Stamp"));
    assert!(
        picture < cover && cover < text && cover < stamp,
        "picture {picture}, cover {cover}, optical text {text}, stamp {stamp}"
    );

    // Showing the scan through, or hiding the text, adds no cover; the
    // picture's bytes are the same in every case (the cover never edits it).
    for options in [
        OcrOptions {
            scan_behind_text: casual_doc_pdf_import::ScanBehindText::ShowThrough,
            ..with(scripted.clone())
        },
        OcrOptions {
            placement: casual_doc_pdf_import::OcrTextPlacement::Hidden,
            ..with(scripted.clone())
        },
    ] {
        let plain = import_pdf_with_ocr(paper.clone(), PdfReadLimits::default(), options).unwrap();
        assert!(covers(&plain.document).is_empty());
        assert!(
            plain
                .report
                .findings
                .iter()
                .all(|f| f.feature != report::OCR_SCAN_COVERED)
        );
        assert_eq!(
            plain.resources, covered.resources,
            "the picture is untouched"
        );
    }
}

// ----------------------------------------------------- bounds and cancel

#[test]
fn the_page_budget_stops_ocr_and_reports_the_rest() {
    let scripted = Arc::new(Scripted::new(vec![("x", [72.0, 100.0, 80.0, 113.0], 0.99)]));
    let imported = import_pdf_with_ocr(
        blank_scan(3, ""),
        PdfReadLimits::default(),
        OcrOptions {
            max_pages: 1,
            ..with(scripted.clone())
        },
    )
    .expect("a budget never fails the import");
    assert_eq!(scripted.calls.load(Ordering::SeqCst), 1);
    let limit = imported
        .report
        .get(report::OCR_LIMIT_REACHED)
        .expect("reported");
    assert_eq!((limit.occurrences, limit.first_page), (2, Some(1)));
}

#[test]
fn the_pixel_budgets_stop_ocr_before_rendering() {
    let scripted = Arc::new(Scripted::new(vec![]));
    // Import-wide pixels: a 612 x 792 page at 1.5 px/pt is 1,090,368 px.
    let imported = import_pdf_with_ocr(
        blank_scan(2, ""),
        PdfReadLimits::default(),
        OcrOptions {
            max_total_pixels: 1_500_000,
            ..with(scripted.clone())
        },
    )
    .unwrap();
    assert_eq!(scripted.calls.load(Ordering::SeqCst), 1);
    assert!(imported.ocr.pixels <= 1_500_000, "{}", imported.ocr.pixels);
    assert!(imported.report.has(report::OCR_LIMIT_REACHED));
    // Per raster: a bound that would force the page under 100 dpi refuses it.
    let scripted = Arc::new(Scripted::new(vec![]));
    let imported = import_pdf_with_ocr(
        blank_scan(1, ""),
        PdfReadLimits::default(),
        OcrOptions {
            limits: OcrLimits {
                max_pixels: 100_000,
                ..OcrLimits::default()
            },
            ..with(scripted.clone())
        },
    )
    .unwrap();
    assert_eq!(scripted.calls.load(Ordering::SeqCst), 0);
    assert_eq!(imported.ocr.pixels, 0);
    assert!(imported.report.has(report::OCR_LIMIT_REACHED));
}

/// Out of time after `after` polls.
struct OutOfTimeAfter {
    polls: AtomicU32,
    after: u32,
}

impl Interrupt for OutOfTimeAfter {
    fn poll(&self) -> Option<Stop> {
        (self.polls.fetch_add(1, Ordering::SeqCst) >= self.after).then_some(Stop::OutOfTime)
    }
}

#[test]
fn a_spent_time_budget_stops_ocr_and_the_import_still_succeeds() {
    let scripted = Arc::new(Scripted::new(vec![("x", [72.0, 100.0, 80.0, 113.0], 0.99)]));
    let imported = import_pdf_with_ocr(
        blank_scan(3, ""),
        PdfReadLimits::default(),
        OcrOptions {
            interrupt: Arc::new(OutOfTimeAfter {
                polls: AtomicU32::new(0),
                after: 0,
            }),
            ..with(scripted.clone())
        },
    )
    .expect("time is a budget, not a failure");
    assert_eq!(scripted.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        imported
            .report
            .get(report::OCR_LIMIT_REACHED)
            .map(|f| f.occurrences),
        Some(3)
    );
    assert_eq!(pictures(&imported.document), 3);
}

#[test]
fn cancel_during_ocr_abandons_the_import_and_returns_nothing() {
    let token = CancelToken::new();
    let mut scripted = Scripted::new(vec![("x", [72.0, 100.0, 80.0, 113.0], 0.99)]);
    scripted.cancel_on_call = Some(token.clone());
    let scripted = Arc::new(scripted);
    let mut job = ImportJob::begin_with_ocr(
        blank_scan(3, ""),
        PdfReadLimits::default(),
        OcrOptions {
            interrupt: Arc::new(token),
            ..with(scripted.clone())
        },
    )
    .unwrap();
    assert_eq!(
        job.step(1),
        Progress::Cancelled,
        "cancel lands inside the page"
    );
    assert_eq!(job.step(1), Progress::Cancelled);
    assert_eq!(
        scripted.calls.load(Ordering::SeqCst),
        1,
        "no further page is read"
    );
    assert_eq!(job.finish().unwrap_err(), PdfImportError::Cancelled);
}

// ---------------------------------------------------------------- layout

#[test]
fn the_heuristic_layout_marks_the_prose_title_and_its_paragraphs() {
    let imported = import_pdf_with_ocr(
        fixture("synthetic-prose.pdf"),
        PdfReadLimits::default(),
        OcrOptions {
            layout: Some(Arc::new(HeuristicLayout)),
            ..OcrOptions::default()
        },
    )
    .unwrap();
    assert_eq!(imported.layout.len(), 2, "one entry per page");
    let kinds: Vec<RegionKind> = imported.layout[0].regions.iter().map(|r| r.kind).collect();
    assert_eq!(
        kinds,
        [RegionKind::Title, RegionKind::Text, RegionKind::Text],
        "{:?}",
        imported.layout[0]
    );
    assert_eq!(
        imported.ocr.layout.as_ref().map(|m| m.provider),
        Some("heuristic-layout")
    );
    // Without a layout provider, no layout stage runs.
    let plain = import_pdf(fixture("synthetic-prose.pdf"), PdfReadLimits::default()).unwrap();
    assert!(plain.layout.is_empty());
}

// ------------------------------------------------------- the real models

/// The `ocrs` weights this file is pinned to (SHA-256): the ones `ocrs-cli`
/// 0.13 downloads by default (`ocrs-models.s3-accelerate.amazonaws.com`,
/// `text-detection.onnx` / `text-recognition.onnx`). The older checkpoints
/// on `huggingface.co/robertknight/ocrs` load but read this page as noise
/// with `ocrs` 0.13 (measured, `docs/182` §20.4). Licence: see §20.1.
const DETECTION_SHA256: &str = "a917b23dbd9524b465df7e922641b3ff2981623df4ded5a0234004ef2fee7cfe";
const RECOGNITION_SHA256: &str = "86c145c2edb96c8caed5b1ebb8f44d706408922211451c309c625157dd6061c5";
/// `docling-project/docling-layout-heron-onnx` `model.onnx` (Apache-2.0).
const HERON_SHA256: &str = "59c81a3a2923042d85034ffc487f8f47e4854117e879aef89b2b9f728fb4922a";

fn ocrs_provider() -> Option<Arc<OcrsProvider>> {
    let Some(dir) = std::env::var_os("OPENDOC_OCR_MODELS") else {
        eprintln!(
            "skipped: OPENDOC_OCR_MODELS is not set (run crates/casual-doc-ocr/fetch-test-models.sh)"
        );
        return None;
    };
    let dir = std::path::PathBuf::from(dir);
    let provider = OcrsProvider::from_model_files(
        &dir.join("text-detection.rten"),
        &dir.join("text-recognition.rten"),
    )
    .expect("the models in OPENDOC_OCR_MODELS load");
    let manifest = provider.manifest();
    assert_eq!(
        manifest.models,
        vec![
            ("detection", DETECTION_SHA256.to_owned()),
            ("recognition", RECOGNITION_SHA256.to_owned())
        ],
        "the pinned published weights"
    );
    Some(Arc::new(provider))
}

/// Character similarity, 1 - Levenshtein / max length, on whitespace-free
/// text (line and frame boundaries are layout, not reading).
fn similarity(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().filter(|c| !c.is_whitespace()).collect();
    let b: Vec<char> = b.chars().filter(|c| !c.is_whitespace()).collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous + usize::from(ca != cb);
            previous = row[j + 1];
            row[j + 1] = substitution.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    1.0 - row[b.len()] as f64 / a.len().max(b.len()).max(1) as f64
}

/// The stated threshold: the OCR text of the scanned prose page matches the
/// native text of the same page at least this well, as character similarity
/// in reading order (1 - Levenshtein / length, whitespace ignored). The
/// measured value is in `docs/182` §20.4; the margin absorbs a model update,
/// not a defect.
const SCAN_SIMILARITY: f64 = 0.95;

fn page_text(imported: &PdfImport, page: usize) -> String {
    let BlockNode::Paragraph(p) = &imported.document.body()[page] else {
        panic!("page paragraph")
    };
    let mut out = String::new();
    for inline in &p.inlines {
        if let InlineNode::TextBox(t) = inline {
            for block in &t.blocks {
                if let BlockNode::Paragraph(q) = block {
                    for i in &q.inlines {
                        match i {
                            InlineNode::Run(r) => out.push_str(&r.text),
                            InlineNode::Break(_) => out.push('\n'),
                            _ => {}
                        }
                    }
                }
            }
            out.push('\n');
        }
    }
    out
}

/// PP-OCRv5, the default provider: `OPENDOC_PADDLE_MODELS` holds the
/// `paddle2onnx` conversions of PaddlePaddle's own weights (`det.onnx`,
/// `rec.onnx` + `rec.dict.txt` for the multilingual Chinese/English/Japanese
/// pack, `latin_rec.onnx` + `latin_rec.dict.txt` for the Latin pack), made
/// by `fetch-test-models.sh`. The conversion is not byte-reproducible across
/// `paddle2onnx` versions, so the source weights are pinned there, not here.
fn paddle_provider(packs: &[&str]) -> Option<Arc<PaddleOcr>> {
    let Some(dir) = std::env::var_os("OPENDOC_PADDLE_MODELS") else {
        eprintln!(
            "skipped: OPENDOC_PADDLE_MODELS is not set (run crates/casual-doc-ocr/fetch-test-models.sh)"
        );
        return None;
    };
    let dir = std::path::PathBuf::from(dir);
    let read = |name: &str| std::fs::read(dir.join(name)).expect(name);
    let packs = packs
        .iter()
        .map(|pack| RecognitionPack {
            name: (*pack).to_owned(),
            model: read(&format!("{pack}.onnx")),
            dictionary: String::from_utf8(read(&format!("{pack}.dict.txt"))).unwrap(),
        })
        .collect();
    Some(Arc::new(
        PaddleOcr::from_model_bytes(read("det.onnx"), packs).expect("the PP-OCRv5 models load"),
    ))
}

/// Every provider whose models are present: PP-OCRv5 with the multilingual
/// pack, PP-OCRv5 with two packs, and ocrs.
fn model_providers() -> Vec<(&'static str, Arc<dyn OcrProvider>)> {
    let mut out: Vec<(&'static str, Arc<dyn OcrProvider>)> = Vec::new();
    if let Some(p) = paddle_provider(&["rec"]) {
        out.push(("pp-ocrv5", p));
    }
    if let Some(p) = paddle_provider(&["rec", "latin_rec"]) {
        out.push(("pp-ocrv5 two packs", p));
    }
    if let Some(p) = ocrs_provider() {
        out.push(("ocrs", p));
    }
    out
}

#[test]
fn a_scanned_page_imports_with_its_text_when_a_model_is_supplied() {
    let source = fixture("synthetic-prose.pdf");
    let native = import_pdf(source.clone(), PdfReadLimits::default()).unwrap();
    let expected = page_text(&native, 0);
    let scan = scan_pdf(&[render_grey(&source, 0, 200.0)], "");
    for (name, provider) in model_providers() {
        let imported =
            import_pdf_with_ocr(scan.clone(), PdfReadLimits::default(), with_model(provider))
                .expect("imports");
        assert_eq!(imported.ocr.classes, vec![Some(PageClass::Raster)]);
        let got = page_text(&imported, 0);
        let score = similarity(&expected, &got);
        eprintln!("{name}: scan similarity {score:.4}\ngot:\n{got}");
        assert!(
            score >= SCAN_SIMILARITY,
            "{name}: OCR text similarity {score:.4} < {SCAN_SIMILARITY}\nexpected:\n{expected}\ngot:\n{got}"
        );
        assert!(imported.report.has(report::OCR_APPLIED));
        assert_eq!(
            pictures(&imported.document),
            1,
            "the scan is kept under its text"
        );
        // The frames: the heading and the two paragraphs, as natively, at
        // about the native sizes (18 pt and 11 pt).
        let (native_sizes, ocr_sizes) = (frame_sizes(&native, 0), frame_sizes(&imported, 0));
        eprintln!("{name}: native frame sizes {native_sizes:?}, OCR {ocr_sizes:?}");
        assert_eq!(ocr_sizes.len(), native_sizes.len(), "{name}: {got}");
        for (n, o) in native_sizes.iter().zip(&ocr_sizes) {
            let ratio = f64::from(*o) / f64::from(*n);
            assert!(
                (0.8..=1.25).contains(&ratio),
                "{name}: size {o} vs native {n}"
            );
        }
        assert!(imported.ocr.provider.is_some(), "the manifest is recorded");
    }
}

/// The first run's size (half points) of each frame on `page`.
fn frame_sizes(imported: &PdfImport, page: usize) -> Vec<u32> {
    let BlockNode::Paragraph(p) = &imported.document.body()[page] else {
        panic!("page paragraph")
    };
    p.inlines
        .iter()
        .filter_map(|i| match i {
            InlineNode::TextBox(t) => t.blocks.iter().find_map(|b| match b {
                BlockNode::Paragraph(q) => q.inlines.iter().find_map(|i| match i {
                    InlineNode::Run(r) => r.properties.size_half_points,
                    _ => None,
                }),
                _ => None,
            }),
            _ => None,
        })
        .collect()
}

fn with_model(provider: Arc<dyn OcrProvider>) -> OcrOptions {
    OcrOptions {
        provider: Some(provider),
        ..OcrOptions::default()
    }
}

#[test]
fn the_same_model_and_input_give_the_same_document() {
    let source = fixture("synthetic-prose.pdf");
    let scan = scan_pdf(&[render_grey(&source, 0, 150.0)], "");
    for (name, provider) in model_providers() {
        let a = import_pdf_with_ocr(
            scan.clone(),
            PdfReadLimits::default(),
            with_model(provider.clone()),
        )
        .unwrap();
        let b = import_pdf_with_ocr(scan.clone(), PdfReadLimits::default(), with_model(provider))
            .unwrap();
        assert_eq!(a.document, b.document, "{name}: PDFR-I8 on this target");
        assert_eq!(a.report, b.report, "{name}");
        assert_eq!(a.ocr, b.ocr, "{name}");
    }
}

#[test]
fn the_model_providers_honour_their_pixel_line_and_cancel_bounds() {
    let (w, h, grey) = render_grey(&fixture("synthetic-prose.pdf"), 0, 150.0);
    let rgb: Vec<u8> = grey.iter().flat_map(|&g| [g, g, g]).collect();
    let raster = Raster::from_rgb(w, h, rgb).unwrap();
    for (name, provider) in model_providers() {
        // Pixels: refused before any model runs.
        let tight = OcrLimits {
            max_pixels: u64::from(w * h) - 1,
            ..OcrLimits::default()
        };
        assert!(
            matches!(
                provider.recognize(&raster, &tight, &casual_doc_ocr::Never),
                Err(OcrError::Limit {
                    limit: "max_pixels",
                    ..
                })
            ),
            "{name}"
        );
        // Lines: two read, the rest counted.
        let two = OcrLimits {
            max_lines: 2,
            ..OcrLimits::default()
        };
        let page = provider
            .recognize(&raster, &two, &casual_doc_ocr::Never)
            .unwrap();
        assert_eq!(page.lines.len(), 2, "{name}");
        assert!(page.lines_skipped >= 7, "{name}: {}", page.lines_skipped);
        // Cancel after detection and one line.
        struct CancelAfter(AtomicU32);
        impl Interrupt for CancelAfter {
            fn poll(&self) -> Option<Stop> {
                (self.0.fetch_add(1, Ordering::SeqCst) >= 2).then_some(Stop::Cancelled)
            }
        }
        let cancel = CancelAfter(AtomicU32::new(0));
        assert_eq!(
            provider.recognize(&raster, &OcrLimits::default(), &cancel),
            Err(OcrError::Cancelled),
            "{name}"
        );
        assert_eq!(
            cancel.0.load(Ordering::SeqCst),
            3,
            "{name}: stopped at the second line"
        );
    }
}

#[test]
fn heron_finds_the_prose_title_and_paragraphs() {
    let Some(path) = std::env::var_os("OPENDOC_LAYOUT_MODEL") else {
        eprintln!(
            "skipped: OPENDOC_LAYOUT_MODEL is not set (run crates/casual-doc-ocr/fetch-test-models.sh)"
        );
        return;
    };
    let bytes = std::fs::read(path).expect("the layout model reads");
    let heron = casual_doc_ocr::HeronLayout::from_model_bytes(bytes).expect("heron loads");
    assert_eq!(
        heron.manifest().models,
        vec![("layout", HERON_SHA256.to_owned())],
        "the pinned published weights"
    );
    let heron: Arc<casual_doc_ocr::HeronLayout> = Arc::new(heron);
    let run = || {
        import_pdf_with_ocr(
            fixture("synthetic-prose.pdf"),
            PdfReadLimits::default(),
            OcrOptions {
                layout: Some(heron.clone()),
                ..OcrOptions::default()
            },
        )
        .unwrap()
    };
    let imported = run();
    let regions = &imported.layout[0].regions;
    eprintln!("heron page 1: {regions:#?}");
    // The heading sits at 72..~200 x 52..74 (page space); the paragraphs
    // below it at y 92..150 and 172..230.
    let covers = |kind: &[RegionKind], y: f32| {
        regions
            .iter()
            .any(|r| kind.contains(&r.kind) && r.bounds.y0 <= y && r.bounds.y1 >= y)
    };
    assert!(
        covers(&[RegionKind::Title, RegionKind::SectionHeader], 65.0),
        "{regions:#?}"
    );
    assert!(covers(&[RegionKind::Text], 120.0), "{regions:#?}");
    assert!(covers(&[RegionKind::Text], 200.0), "{regions:#?}");
    assert_eq!(run().layout, imported.layout, "PDFR-I8 on this target");
}

/// `(w, h, grey)` turned `degrees` clockwise about its centre on a canvas of
/// the same size, white where nothing maps (a scan fed in skewed or upside
/// down). Bilinear. O(pixels).
fn turn_grey((w, h, grey): &(u32, u32, Vec<u8>), degrees: f32) -> (u32, u32, Vec<u8>) {
    let (w, h) = (*w as usize, *h as usize);
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let (sin, cos) = degrees.to_radians().sin_cos();
    let at = |x: i64, y: i64| -> f32 {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
            255.0
        } else {
            f32::from(grey[y as usize * w + x as usize])
        }
    };
    let mut out = vec![255u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            // The source point the clockwise turn brought here.
            let sx = cx + dx * cos + dy * sin - 0.5;
            let sy = cy - dx * sin + dy * cos - 0.5;
            let (x0, y0) = (sx.floor(), sy.floor());
            let (tx, ty) = (sx - x0, sy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let top = at(x0, y0) * (1.0 - tx) + at(x0 + 1, y0) * tx;
            let bottom = at(x0, y0 + 1) * (1.0 - tx) + at(x0 + 1, y0 + 1) * tx;
            out[y * w + x] = (top * (1.0 - ty) + bottom * ty).round() as u8;
        }
    }
    (w as u32, h as u32, out)
}

/// Every frame's text on `page`, upright and turned, in paint order, and
/// each turned frame's rotation.
fn all_frames(imported: &PdfImport, page: usize) -> (String, Vec<i32>) {
    let BlockNode::Paragraph(p) = &imported.document.body()[page] else {
        panic!("page paragraph")
    };
    let mut out = String::new();
    let mut turns = Vec::new();
    let push = |blocks: &[BlockNode], out: &mut String| {
        for block in blocks {
            if let BlockNode::Paragraph(q) = block {
                for i in &q.inlines {
                    match i {
                        InlineNode::Run(r) => out.push_str(&r.text),
                        InlineNode::Break(_) => out.push('\n'),
                        _ => {}
                    }
                }
            }
        }
        out.push('\n');
    };
    for inline in &p.inlines {
        match inline {
            InlineNode::TextBox(t) => push(&t.blocks, &mut out),
            InlineNode::Group(g) => {
                for child in &g.children {
                    if let casual_doc_model::v1::GroupChild::TextBox(t) = child {
                        turns.push(t.rotation.unwrap_or(0));
                        push(&t.blocks, &mut out);
                    }
                }
            }
            _ => {}
        }
    }
    (out, turns)
}

/// PP-OCRv5 with the multilingual pack and, when present, the text-line
/// orientation classifier (`textline_ori.onnx`, `fetch-test-models.sh`).
fn paddle_with_orientation() -> Option<Arc<PaddleOcr>> {
    let dir = std::path::PathBuf::from(std::env::var_os("OPENDOC_PADDLE_MODELS")?);
    let path = dir.join("textline_ori.onnx");
    if !path.is_file() {
        eprintln!("skipped: {} is absent", path.display());
        return None;
    }
    let provider = paddle_provider(&["rec"])?;
    let provider = Arc::try_unwrap(provider).expect("sole owner");
    Some(Arc::new(
        provider
            .with_orientation(std::fs::read(path).unwrap())
            .expect("the orientation model loads"),
    ))
}

#[test]
fn an_upside_down_scan_reads_the_right_way_up_with_the_orientation_model() {
    let Some(provider) = paddle_with_orientation() else {
        return;
    };
    let source = fixture("synthetic-prose.pdf");
    let native = import_pdf(source.clone(), PdfReadLimits::default()).unwrap();
    let expected = page_text(&native, 0);
    let scan = scan_pdf(&[turn_grey(&render_grey(&source, 0, 200.0), 180.0)], "");
    let imported =
        import_pdf_with_ocr(scan, PdfReadLimits::default(), with_model(provider)).unwrap();
    let (got, turns) = all_frames(&imported, 0);
    let score = similarity(&expected, &got);
    eprintln!("upside down: similarity {score:.4}, turns {turns:?}\ngot:\n{got}");
    assert!(
        score >= SCAN_SIMILARITY,
        "upside down: similarity {score:.4} < {SCAN_SIMILARITY}\ngot:\n{got}"
    );
    assert!(
        !turns.is_empty() && turns.iter().all(|&t| t == 180 * 60_000),
        "every frame is turned half way, as the scan is: {turns:?}"
    );
}

#[test]
fn a_skewed_scan_reads_as_well_as_a_straight_one() {
    let Some(provider) = paddle_provider(&["rec"]) else {
        return;
    };
    let source = fixture("synthetic-prose.pdf");
    let native = import_pdf(source.clone(), PdfReadLimits::default()).unwrap();
    let expected = page_text(&native, 0);
    let scan = scan_pdf(&[turn_grey(&render_grey(&source, 0, 200.0), 3.0)], "");
    let imported =
        import_pdf_with_ocr(scan, PdfReadLimits::default(), with_model(provider)).unwrap();
    let (got, turns) = all_frames(&imported, 0);
    let score = similarity(&expected, &got);
    eprintln!("3 degrees: similarity {score:.4}, turns {turns:?}\ngot:\n{got}");
    assert!(
        score >= SCAN_SIMILARITY,
        "3 degrees: similarity {score:.4} < {SCAN_SIMILARITY}\ngot:\n{got}"
    );
    // The frames carry the skew (one shared angle, within a quarter degree
    // of the 3 the scan was turned by), so they sit on the scan's lines.
    assert!(!turns.is_empty(), "the frames are turned");
    for t in &turns {
        let degrees = f64::from(*t) / 60_000.0;
        assert!((degrees - 3.0).abs() < 0.25, "turned {degrees}");
    }
    assert!(imported.ocr.lines_turned > 0);
}

/// The host path end to end: the model directory `fetch-test-models.sh`
/// writes (the parent of `OPENDOC_PADDLE_MODELS`), read by
/// `OcrModels::from_dir`, built into options by `OcrOptions::from_models`,
/// and the import records every model's identity.
#[test]
fn a_model_directory_builds_the_providers_and_the_import_records_their_identity() {
    let Some(paddle) = std::env::var_os("OPENDOC_PADDLE_MODELS") else {
        eprintln!("skipped: OPENDOC_PADDLE_MODELS is not set");
        return;
    };
    let dir = std::path::PathBuf::from(paddle)
        .parent()
        .expect("paddle/ sits in the model directory")
        .to_path_buf();
    let models = casual_doc_pdf_import::OcrModels::from_dir(&dir).expect("the directory reads");
    let options = OcrOptions::from_models(&models).expect("the models load");
    let source = fixture("synthetic-prose.pdf");
    let native = import_pdf(source.clone(), PdfReadLimits::default()).unwrap();
    let scan = scan_pdf(&[render_grey(&source, 0, 200.0)], "");
    let imported = import_pdf_with_ocr(scan, PdfReadLimits::default(), options).unwrap();
    let score = similarity(&page_text(&native, 0), &all_frames(&imported, 0).0);
    assert!(score >= SCAN_SIMILARITY, "similarity {score:.4}");
    let manifest = imported.ocr.provider.expect("the provider is recorded");
    let roles: Vec<&str> = manifest.models.iter().map(|(role, _)| *role).collect();
    eprintln!("packs {:?}, roles {roles:?}", models.recognition_names());
    assert_eq!(roles[0], "detection");
    assert!(roles.contains(&"recognition") && roles.contains(&"dictionary"));
    assert_eq!(roles.contains(&"orientation"), models.has_orientation());
    assert!(manifest.models.iter().all(|(_, sha)| sha.len() == 64));
    assert_eq!(imported.ocr.layout.is_some(), models.has_layout());
}

// ------------------------------------------------------- flowing document

/// The flowing document of a recognised scan is its text: the scan (a
/// picture the size of each page, under the recognised lines) is neither an
/// inline picture pushing the text, a background behind text that no longer
/// lines up with it, nor a letterhead repeated in the header. The exact
/// layout keeps the scan.
#[test]
fn a_recognised_scan_reflows_as_its_text_and_leaves_the_scan_to_the_exact_layout() {
    let scripted: Arc<dyn OcrProvider> = Arc::new(Scripted::new(vec![
        ("Recognised", [72.0, 100.0, 160.0, 113.0], 0.99),
        ("words", [72.0, 120.0, 120.0, 133.0], 0.99),
    ]));
    let imported =
        import_pdf_with_ocr(blank_scan(2, ""), PdfReadLimits::default(), with(scripted)).unwrap();
    assert_eq!(
        pictures(&imported.document),
        2,
        "the exact layout keeps both scans"
    );
    let flowing = imported.reflow().expect("reflows");
    flowing.document.validate().expect("valid");
    let mut text = String::new();
    let mut graphics = 0;
    let parts = flowing
        .document
        .definitions()
        .headers
        .iter()
        .flat_map(|(_, h)| h.blocks.iter());
    for block in flowing.document.body().iter().chain(parts) {
        if let BlockNode::Paragraph(p) = block {
            for inline in &p.inlines {
                match inline {
                    InlineNode::Run(r) => text.push_str(&r.text),
                    InlineNode::Drawing(_)
                    | InlineNode::AnchoredDrawing(_)
                    | InlineNode::Group(_) => {
                        graphics += 1;
                    }
                    _ => {}
                }
            }
            text.push('\n');
        }
    }
    assert!(text.contains("Recognised"), "{text}");
    assert_eq!(
        graphics, 0,
        "no scan, background or cover in the flowing document"
    );
    assert!(flowing.report.has(report::SCAN_REPLACED_BY_TEXT));
}

/// A scan fed a degree and a half off square: the recogniser reads every
/// line turned by that much. The flowing document lays the lines level and
/// groups them as text, instead of reporting each as rotated text of its own
/// (404 lines on the owner's eight-page scan, which reflowed to 34 pages).
#[test]
fn a_skewed_scans_lines_reflow_as_level_text_not_rotated_text() {
    let mut scripted = Scripted::new(vec![
        (
            "The first line of a paragraph",
            [72.0, 100.0, 300.0, 113.0],
            0.99,
        ),
        (
            "runs on into the second line.",
            [72.0, 115.0, 296.0, 128.0],
            0.99,
        ),
    ]);
    scripted.turn = Some(1.5);
    let scripted: Arc<dyn OcrProvider> = Arc::new(scripted);
    let imported =
        import_pdf_with_ocr(blank_scan(1, ""), PdfReadLimits::default(), with(scripted)).unwrap();
    let flowing = imported.reflow().expect("reflows");
    flowing.document.validate().expect("valid");
    assert!(
        !flowing.report.has(report::TEXT_ROTATED),
        "a skewed line is not rotated text"
    );
    let paragraphs: Vec<String> = flowing
        .document
        .body()
        .iter()
        .filter_map(|b| match b {
            BlockNode::Paragraph(p) => {
                let text: String = p
                    .inlines
                    .iter()
                    .filter_map(|i| match i {
                        InlineNode::Run(r) => Some(r.text.as_str()),
                        _ => None,
                    })
                    .collect();
                (!text.trim().is_empty()).then_some(text)
            }
            _ => None,
        })
        .collect();
    assert_eq!(paragraphs.len(), 1, "{paragraphs:?}");
}
