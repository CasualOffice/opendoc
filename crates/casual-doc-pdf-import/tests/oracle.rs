//! The P1 exit gate's pixel comparison (`docs/182` §10): open each corpus PDF,
//! import it, render every page of the MODEL with our renderer, and compare
//! against hayro's own render of the ORIGINAL page — an oracle we did not write.
//!
//! # The metric and its tolerance
//!
//! Both pages are rendered at 72 dpi (one pixel per point) over white. A pixel
//! is *ink* when its luma is below 160. Text is drawn with substitute faces
//! (`pdf.import.font_substituted`, `docs/182` G-1), so glyph shapes differ and
//! a per-pixel equality would measure the font, not the import. The metric is
//! therefore ink agreement within a small radius: **recall** is the share of the
//! oracle's ink pixels that have our ink within [`RADIUS`] pixels, **precision**
//! the share of ours that have the oracle's. The stated tolerance is
//! [`MIN_AGREEMENT`] for both, on every page. A frame placed a few points off,
//! a missing picture, a lost shape or a dropped line fails it.

use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_pdf_import::{PdfReadLimits, import_pdf};
use casual_doc_render::{MapMediaSource, RegistryFontSource, Surface, render};

/// Ink within this many pixels (points) counts as agreeing.
const RADIUS: usize = 2;
/// Recall and precision must both reach this on every page.
const MIN_AGREEMENT: f64 = 0.85;
/// Luma below this is ink.
const INK: u32 = 160;

struct Ink {
    width: usize,
    height: usize,
    mask: Vec<bool>,
}

impl Ink {
    fn from_rgba(width: usize, height: usize, rgba: &[u8], premultiplied: bool) -> Self {
        let mut mask = Vec::with_capacity(width * height);
        for px in rgba.chunks_exact(4) {
            let (mut r, mut g, mut b) = (u32::from(px[0]), u32::from(px[1]), u32::from(px[2]));
            let a = u32::from(px[3]);
            if premultiplied {
                // Composite over white.
                r += 255 - a;
                g += 255 - a;
                b += 255 - a;
            }
            let luma = (r * 299 + g * 587 + b * 114) / 1000;
            mask.push(luma < INK);
        }
        Self {
            width,
            height,
            mask,
        }
    }

    fn at(&self, x: usize, y: usize) -> bool {
        x < self.width && y < self.height && self.mask[y * self.width + x]
    }

    fn near(&self, x: usize, y: usize) -> bool {
        let (x0, y0) = (x.saturating_sub(RADIUS), y.saturating_sub(RADIUS));
        (y0..=y + RADIUS).any(|yy| (x0..=x + RADIUS).any(|xx| self.at(xx, yy)))
    }

    /// The share of `self`'s ink that `other` has nearby (1.0 for no ink).
    fn covered_by(&self, other: &Self) -> (f64, usize) {
        let (mut ink, mut hit) = (0usize, 0usize);
        for y in 0..self.height.min(other.height) {
            for x in 0..self.width.min(other.width) {
                if self.at(x, y) {
                    ink += 1;
                    if other.near(x, y) {
                        hit += 1;
                    }
                }
            }
        }
        if ink == 0 {
            (1.0, 0)
        } else {
            (hit as f64 / ink as f64, ink)
        }
    }
}

fn oracle_pages(bytes: &[u8]) -> Vec<Ink> {
    let pdf = hayro::hayro_syntax::Pdf::new(bytes.to_vec()).expect("hayro opens it");
    let settings = hayro::hayro_interpret::InterpreterSettings {
        render_annotations: false,
        ..Default::default()
    };
    let white = hayro::vello_cpu::color::AlphaColor::from_rgba8(255, 255, 255, 255);
    pdf.pages()
        .iter()
        .map(|page| {
            let pixmap = hayro::render(
                page,
                &hayro::RenderCache::new(),
                &settings,
                &hayro::RenderSettings::default(),
                &hayro::PixmapSettings {
                    x_scale: 1.0,
                    y_scale: 1.0,
                    bg_color: white,
                },
            );
            let rgba: Vec<u8> = pixmap
                .data()
                .iter()
                .flat_map(|p| [p.r, p.g, p.b, p.a])
                .collect();
            Ink::from_rgba(
                usize::from(pixmap.width()),
                usize::from(pixmap.height()),
                &rgba,
                true,
            )
        })
        .collect()
}

fn model_pages(bytes: &[u8]) -> Vec<Ink> {
    let imported = import_pdf(bytes.to_vec(), PdfReadLimits::default()).expect("import");
    let mut media = MapMediaSource::new();
    for (name, bytes) in &imported.resources {
        media.insert(name.clone(), bytes.clone());
    }
    let shaper = ParleyShaper::new();
    // A host registers the document's own font programs at open, as it does
    // `.odttf` faces (G-1).
    let outcome = casual_doc_layout::font_registry::register_embedded_fonts(
        &shaper,
        &imported.document,
        |part| imported.resources.get(part).map(Vec::as_slice),
    );
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    let pages = paginate_document(&imported.document, &shaper);
    let registry = shaper.registry();
    let fonts = RegistryFontSource::new(&registry);
    pages
        .pages
        .iter()
        .map(|page| {
            let width = page.page_size.width.to_device_px(72.0).ceil().max(1.0) as u32;
            let height = page.page_size.height.to_device_px(72.0).ceil().max(1.0) as u32;
            let mut surface = Surface::new(width, height).expect("surface");
            render(&compose_page(page), &mut surface, 72.0, &fonts, &media);
            Ink::from_rgba(width as usize, height as usize, surface.data(), false)
        })
        .collect()
}

/// `(recall, precision)` per page.
fn agreement(name: &str) -> Vec<(f64, f64)> {
    let bytes = fixture(name);
    let oracle = oracle_pages(&bytes);
    let ours = model_pages(&bytes);
    assert_eq!(
        oracle.len(),
        ours.len(),
        "{name}: one model page per PDF page"
    );
    oracle
        .iter()
        .zip(&ours)
        .map(|(theirs, ours)| (theirs.covered_by(ours).0, ours.covered_by(theirs).0))
        .collect()
}

/// Pages whose import reports a loss that is visible by construction, with the
/// finding that reports it: Type3 box glyphs become substitute text. Each is
/// asserted to be REPORTED, and the last test asserts the comparison SEES one.
///
/// Rotated text — a rotated run, and every line of a `/Rotate` page — was
/// exempt here until G-3 (`docs/182` §16b): it was placed upright and reported.
/// It is now a turned frame, and its pages are compared like any other.
const REPORTED_LOSSES: [(&str, usize, &str); 1] = [(
    "synthetic-type3.pdf",
    1,
    "pdf.import.type3_glyphs_substituted",
)];

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/pdf")
            .join(name),
    )
    .expect(name)
}

/// Every page of every corpus file matches the oracle, except the pages whose
/// visible loss the import reports.
#[test]
fn every_page_matches_hayros_render_of_the_original() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pdf");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".pdf"))
        .collect();
    names.sort();
    assert!(names.len() >= 18, "the corpus is present");
    let mut failures = Vec::new();
    let mut compared = 0;
    for name in &names {
        for (index, (recall, precision)) in agreement(name).into_iter().enumerate() {
            let page = index + 1;
            // `OPENDOC_ORACLE_SCORES=1` prints every page's score, so a change
            // that should not move evidence (the font/CMap trim, `docs/182`
            // §17a) can be compared before and after, not just against 0.85.
            // Opt-in diagnostics only, so stderr is the right channel.
            #[allow(clippy::print_stderr)]
            if std::env::var_os("OPENDOC_ORACLE_SCORES").is_some() {
                eprintln!("oracle {name} page {page}: recall {recall:.6} precision {precision:.6}");
            }
            if let Some((_, _, feature)) = REPORTED_LOSSES
                .iter()
                .find(|(file, at, _)| file == name && *at == page)
            {
                let imported = import_pdf(fixture(name), PdfReadLimits::default()).unwrap();
                assert!(
                    imported.report.has(feature),
                    "{name} page {page} is exempt only because it reports {feature}"
                );
                continue;
            }
            compared += 1;
            if recall < MIN_AGREEMENT || precision < MIN_AGREEMENT {
                failures.push(format!(
                    "{name} page {page}: recall {recall:.3} precision {precision:.3}"
                ));
            }
        }
    }
    assert!(compared >= 16, "{compared} pages compared");
    assert!(
        failures.is_empty(),
        "pages below {MIN_AGREEMENT} ink agreement within {RADIUS}px:\n{}",
        failures.join("\n")
    );
}

/// The comparison is able to fail: the Type3 page's substituted glyphs, the
/// one exemption left, disagree with the oracle's ink.
#[test]
fn the_comparison_sees_a_reported_visual_loss() {
    let (recall, precision) = agreement("synthetic-type3.pdf")[0];
    assert!(
        recall < MIN_AGREEMENT || precision < MIN_AGREEMENT,
        "type3 page recall {recall:.3} precision {precision:.3}"
    );
}

/// Rotated text matches the original (`docs/182` G-3): the rotated run page and
/// the `/Rotate 90` page, each once exempt, are within the tolerance on their
/// own, named here so a regression says which.
#[test]
fn rotated_text_pages_match_the_oracle() {
    for (name, index) in [
        ("synthetic-rotated-text.pdf", 0),
        ("synthetic-page-boxes.pdf", 1),
    ] {
        let (recall, precision) = agreement(name)[index];
        assert!(
            recall >= MIN_AGREEMENT && precision >= MIN_AGREEMENT,
            "{name} page {}: recall {recall:.3} precision {precision:.3}",
            index + 1
        );
    }
}
