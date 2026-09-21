//! Where an open spends its time: import, then the measure-tier pass.
//!
//! The committed artifact behind `docs/116` §3. That section says import costs
//! 0.83 s of the owner's 1,303,306-paragraph file and the measure pass costs
//! 7.78 s — 88% of the open — and the whole design decision about opening
//! lazily rests on that split, so the split has to be reproducible rather than
//! recalled:
//!
//! ```text
//! cargo run --release --example open-phase-timing -p casual-doc-wasm -- <file>
//! ```
//!
//! Native, deliberately. Through the browser every number is 4-6× larger
//! (`webapp/tests/e2e/viewer-ceiling-measurement.spec.mjs` measures that end,
//! including the main-thread block the tab actually feels), but the *ratio*
//! between the phases is a property of the work and not of the target, and it is
//! the ratio that decides what is worth making lazy.
//!
//! It takes a path so a large real document can be measured without the document
//! entering the repository.
#![allow(clippy::print_stdout, clippy::print_stderr)]

#[cfg(target_arch = "wasm32")]
fn main() {
    println!("open-phase-timing is native-only");
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use casual_doc_io::{
        DetectionRequest, FormatSelection, PlainTextLimits, builtin_registry_with_limits,
    };
    use casual_doc_layout::incremental::PageRange;
    use casual_doc_layout::shape::ParleyShaper;
    use casual_doc_layout::windowed::{WindowPolicy, measure_document, window_of};
    use casual_doc_ooxml::PackageLimits;
    use std::time::Instant;

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: open-phase-timing <document>");
        std::process::exit(2);
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("{path}: {error}");
            std::process::exit(1);
        }
    };
    println!("{path}: {} bytes", bytes.len());

    // The viewer's own limits, so this measures the path the editor takes.
    let registry = builtin_registry_with_limits(
        PackageLimits {
            max_input_bytes: 200 * 1024 * 1024,
            max_total_expanded_bytes: 512 * 1024 * 1024,
            max_single_expanded_bytes: 200 * 1024 * 1024,
            ..PackageLimits::default()
        },
        PlainTextLimits {
            max_input_bytes: 200 * 1024 * 1024,
            max_output_bytes: 200 * 1024 * 1024,
            max_paragraphs: 1_800_000,
            max_unicode_scalar_values: PlainTextLimits::HARD_MAX_UNICODE_SCALAR_VALUES,
            ..PlainTextLimits::default()
        },
    );

    let whole = Instant::now();
    let imported = match registry.import(
        DetectionRequest {
            bytes: &bytes,
            selection: FormatSelection::Auto,
            file_name_hint: None,
            mime_hint: None,
        },
        true,
    ) {
        Ok(imported) => imported,
        Err(error) => {
            eprintln!("import: {error}");
            std::process::exit(1);
        }
    };
    let import = whole.elapsed();
    println!(
        "import              {:>8.2} s   {} top-level blocks",
        import.as_secs_f64(),
        imported.document.body().len()
    );

    let started = Instant::now();
    let shaper = ParleyShaper::new();
    println!(
        "shaper              {:>8.2} s",
        started.elapsed().as_secs_f64()
    );

    let started = Instant::now();
    let measures = measure_document(&imported.document, &shaper);
    let measure = started.elapsed();
    match &measures {
        Ok(measures) => println!(
            "measure_document    {:>8.2} s   {} pages, {} checkpoints, {} B resident",
            measure.as_secs_f64(),
            measures.page_count(),
            measures.checkpoints.len(),
            measures.resident_bytes(),
        ),
        Err(reason) => println!(
            "measure_document    {:>8.2} s   not windowable: {reason:?} (the viewer lays this \
             document out whole)",
            measure.as_secs_f64(),
        ),
    }

    // One window, at three places, because "open lazily" is only worth building
    // if a window is cheap wherever the reader lands.
    if let Ok(measures) = &measures {
        let total = measures.page_count();
        for at in [0, total / 2, total.saturating_sub(2)] {
            let started = Instant::now();
            let window = window_of(
                &imported.document,
                &shaper,
                measures,
                PageRange::new(at, (at + 1).min(total)),
                WindowPolicy::default(),
            );
            println!(
                "window_of page {at:<8} {:>8.2} s   {} pages resident",
                started.elapsed().as_secs_f64(),
                window.viewport.pages.len(),
            );
        }
    }

    println!(
        "total               {:>8.2} s",
        whole.elapsed().as_secs_f64()
    );
}
