//! Renders one page of an arbitrary `.docx` to a PNG — a visual-fidelity probe
//! over the real sample corpus. `render_gallery <in.docx> <out.png> [page]`.
#![allow(clippy::print_stderr, clippy::print_stdout)] // a manual harness
use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::{document_page_config, paginate_document};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use casual_doc_render::{MapMediaSource, RegistryFontSource, Surface, render};

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .expect("usage: render_gallery <in.docx> <out.png> [page]");
    let out = args
        .next()
        .expect("usage: render_gallery <in.docx> <out.png> [page]");
    let page_idx: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);

    let bytes = std::fs::read(&input).expect("read docx");
    let limits = PackageLimits {
        max_input_bytes: 64 * 1024 * 1024,
        max_total_expanded_bytes: 256 * 1024 * 1024,
        max_single_expanded_bytes: 64 * 1024 * 1024,
        ..PackageLimits::default()
    };
    let mut package = match DocxPackage::open(&bytes, limits) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("OPEN FAILED {input}: {e:?}");
            return;
        }
    };
    let imported = match import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    ) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("IMPORT FAILED {input}: {e:?}");
            return;
        }
    };
    let document = imported.document;

    if std::env::var("PROBE").is_ok() {
        use casual_doc_model::v1::{BlockNode, LineRule};
        let (mut total, mut exact, mut auto, mut atleast, mut none, mut styled) =
            (0, 0, 0, 0, 0, 0);
        for b in document.body() {
            if let BlockNode::Paragraph(p) = b {
                total += 1;
                if p.properties.style_ref.is_some() {
                    styled += 1;
                }
                match p.properties.spacing.as_ref().and_then(|s| s.line_rule) {
                    Some(LineRule::Exact) => exact += 1,
                    Some(LineRule::AtLeast) => atleast += 1,
                    Some(LineRule::Auto) => auto += 1,
                    None => {
                        if p.properties
                            .spacing
                            .as_ref()
                            .and_then(|s| s.line_percent)
                            .is_some()
                        {
                            auto += 1;
                        } else {
                            none += 1;
                        }
                    }
                }
            }
        }
        eprintln!(
            "PROBE total={total} exact={exact} atleast={atleast} auto={auto} none={none} styled={styled}"
        );
        // Sample the shaped line heights of the first several paragraphs.
        use casual_doc_layout::block::BlockFragment;
        let shaper2 = ParleyShaper::new();
        let cw = casual_doc_layout::units::Twip::from_points(468);
        let galley = casual_doc_layout::flow::build_galley(&document, &shaper2, cw);
        // Correlate each body paragraph's imported line rule with its shaped
        // first-line height.
        let mut exact_heights: Vec<i32> = Vec::new();
        let mut other_heights: Vec<i32> = Vec::new();
        let mut gi = 0;
        for b in document.body() {
            if let BlockNode::Paragraph(p) = b {
                let is_exact = matches!(
                    p.properties.spacing.as_ref().and_then(|s| s.line_rule),
                    Some(LineRule::Exact)
                );
                // Find the matching fragment by id.
                while gi < galley.len() {
                    if let BlockFragment::Paragraph { id, lines, .. } = &galley[gi]
                        && *id == p.id
                    {
                        if let Some(l) = lines.lines.first() {
                            if is_exact {
                                exact_heights.push(l.height.raw());
                            } else {
                                other_heights.push(l.height.raw());
                            }
                        }
                        gi += 1;
                        break;
                    }
                    gi += 1;
                }
            }
        }
        let avg = |v: &[i32]| {
            if v.is_empty() {
                0
            } else {
                v.iter().sum::<i32>() / v.len() as i32
            }
        };
        eprintln!(
            "PROBE exact_lines n={} avg_h={} sample={:?}",
            exact_heights.len(),
            avg(&exact_heights),
            &exact_heights.iter().take(6).collect::<Vec<_>>()
        );
        eprintln!(
            "PROBE other_lines n={} avg_h={}",
            other_heights.len(),
            avg(&other_heights)
        );
        let total_h: i32 = galley.iter().map(|f| f.height().raw()).sum();
        eprintln!("PROBE galley_total_height_twips={total_h}");
    }

    let mut media = MapMediaSource::new();
    for (_id, reference) in document.definitions().media.iter() {
        if let Ok(part_bytes) = package.read_part(&reference.part_name) {
            media.insert(reference.part_name.clone(), part_bytes);
        }
    }

    let shaper = ParleyShaper::new();
    // One call: real per-section geometry, flowed headers/footers, anchored
    // drawings, and page-number fields.
    let pages = paginate_document(&document, &shaper);
    let config = document_page_config(&document);
    let page = match pages.pages.get(page_idx) {
        Some(p) => p,
        None => {
            eprintln!(
                "no page {page_idx} ({} pages) for {input}",
                pages.page_count()
            );
            return;
        }
    };

    let dpi = 96.0;
    let w = config.page_size.width.to_device_px(dpi).ceil() as u32;
    let h = config.page_size.height.to_device_px(dpi).ceil() as u32;
    let mut surface = Surface::new(w, h).unwrap();
    // Serve bundled + fallback (OS/system, with --features system-fonts) faces
    // from the shaper's registry, taken after pagination shaped every paragraph.
    let registry = shaper.registry();
    let fonts = RegistryFontSource::new(&registry);
    render(&compose_page(page), &mut surface, dpi, &fonts, &media);
    std::fs::write(&out, surface.encode_png().unwrap()).unwrap();
    let uncovered = registry.missing_coverage().len();
    println!(
        "OK {input}: {} pages -> {out} (page {page_idx}){}",
        pages.page_count(),
        if uncovered > 0 {
            format!(" [{uncovered} uncovered code points]")
        } else {
            String::new()
        },
    );
}
