//! What a worker could hand the main thread instead of the open itself.
//!
//! The committed artifact behind `docs/180` §3.2. A worker that imports and lays
//! a document out only helps the editor if the main thread can then ADOPT the
//! result for less than it would cost to compute it, because the editor's
//! editing path holds the document synchronously on the main thread (ADR-073).
//! Without `SharedArrayBuffer` the only thing that crosses is bytes, so the
//! question is how the cost of decoding the model and the laid-out pages from
//! bytes compares with importing and paginating:
//!
//! ```text
//! cargo run --release --example open-handover-timing -p casual-doc-wasm -- <file>
//! ```
//!
//! Native, like `open-phase-timing`: the browser multiplies every row by a
//! similar factor, and it is the ratio between the rows that decides the design.
#![allow(clippy::print_stdout, clippy::print_stderr)]

#[cfg(target_arch = "wasm32")]
fn main() {
    println!("open-handover-timing is native-only");
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use casual_doc_io::{DetectionRequest, FormatSelection, builtin_registry};
    use casual_doc_layout::document_layout::paginate_document;
    use casual_doc_layout::page::PaginatedLayout;
    use casual_doc_layout::shape::ParleyShaper;
    use casual_doc_model::v1::Document;
    use std::time::Instant;

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: open-handover-timing <document>");
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

    let started = Instant::now();
    let imported = match builtin_registry().import(
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
    let import = started.elapsed().as_secs_f64();
    let document = imported.document;
    println!(
        "import (unzip, parse, normalize)  {:>8.3} s   {} top-level blocks",
        import,
        document.body().len()
    );

    let shaper = ParleyShaper::new();
    let started = Instant::now();
    let layout = paginate_document(&document, &shaper);
    let paginate = started.elapsed().as_secs_f64();
    println!(
        "paginate (shape + place)          {:>8.3} s   {} pages",
        paginate,
        layout.page_count()
    );

    let model = serde_json::to_vec(&document).unwrap_or_default();
    let pages = serde_json::to_vec(&layout).unwrap_or_default();
    let started = Instant::now();
    let decoded: Result<Document, _> = serde_json::from_slice(&model);
    let model_decode = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let decoded_pages: Result<PaginatedLayout, _> = serde_json::from_slice(&pages);
    let pages_decode = started.elapsed().as_secs_f64();
    println!(
        "decode model (serde_json)         {:>8.3} s   {} bytes{}",
        model_decode,
        model.len(),
        match &decoded {
            Ok(back) if *back == document => String::new(),
            Ok(_) => "   NOT EQUAL".to_owned(),
            Err(error) => format!("   FAILED: {error}"),
        }
    );
    println!(
        "decode pages (serde_json)         {:>8.3} s   {} bytes{}",
        pages_decode,
        pages.len(),
        if decoded_pages.as_ref().is_ok_and(|p| *p == layout) {
            ""
        } else {
            "   NOT EQUAL"
        }
    );
    println!(
        "open on the main thread today     {:>8.3} s",
        import + paginate
    );
    println!(
        "adopt a worker's result           {:>8.3} s   ({:.1}x)",
        model_decode + pages_decode,
        (import + paginate) / (model_decode + pages_decode).max(f64::EPSILON)
    );
}
