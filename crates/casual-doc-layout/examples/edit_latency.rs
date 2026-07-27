//! Measures per-edit layout cost on a large body, isolating where the time goes.
//!
//! The WASM edit path calls [`paginate_document`] on every keystroke, which
//! re-shapes *every* paragraph via [`build_galley_for_blocks`]. This example times
//! that against the incremental [`build_galley_cached`] path (which re-shapes only
//! the edited paragraph) so we can see the ceiling a galley cache would buy.
//!
//! `cargo run --release --example edit_latency [paragraph_count]`
#![allow(clippy::print_stdout)] // a manual measurement, not library code
use std::time::Instant;

use casual_doc_layout::document_layout::{paginate_document, paginate_document_cached};
use casual_doc_layout::flow::{build_galley_cached, build_galley_for_blocks};
use casual_doc_layout::incremental::{DirtySet, GalleyCache};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, Paragraph, ParagraphProperties, Run,
    RunProperties,
};

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

/// A body of `n` paragraphs, each a full sentence — representative of a real
/// prose document (the fast path we care about: "para first").
fn big_doc(n: u64) -> Document {
    const SENTENCE: &str = "The quick brown fox jumps over the lazy dog while the \
        editor reflows this paragraph and every other one on the page.";
    let body = (0..n)
        .map(|i| {
            let id = i + 1;
            BlockNode::Paragraph(Paragraph {
                id: node(id),
                properties: ParagraphProperties::default(),
                inlines: vec![InlineNode::Run(Run {
                    id: node(id + 1_000_000),
                    properties: RunProperties::default(),
                    text: format!("Paragraph {id}. {SENTENCE}"),
                })],
            })
        })
        .collect();
    Document::new(node(9_000_000), body, Definitions::default()).unwrap()
}

fn median_ms(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    samples[samples.len() / 2]
}

fn main() {
    let n: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);
    let document = big_doc(n);
    let shaper = ParleyShaper::new();

    // Content width of a US-Letter page with 1" margins (12240 - 2*1440).
    const WIDTH: Twip = Twip(9_360);
    const ITERS: usize = 11;

    // Warm the OS font cache / shaper so the first sample isn't an outlier.
    let _ = paginate_document(&document, &shaper);

    // 1. What the app does on every edit today: full re-pagination.
    let full_pag: Vec<f64> = (0..ITERS)
        .map(|_| {
            let t = Instant::now();
            let _ = paginate_document(&document, &shaper);
            t.elapsed().as_secs_f64() * 1e3
        })
        .collect();

    // 2. The shaping portion alone (fresh galley of the whole body).
    let full_shape: Vec<f64> = (0..ITERS)
        .map(|_| {
            let t = Instant::now();
            let _ = build_galley_for_blocks(&document, &shaper, document.body(), WIDTH);
            t.elapsed().as_secs_f64() * 1e3
        })
        .collect();

    // 3. The incremental path: warm cache, then re-shape only one edited paragraph.
    let mut cache = GalleyCache::new();
    let _ = build_galley_cached(
        &document,
        &shaper,
        WIDTH,
        &mut cache,
        &DirtySet::everything(),
    );
    let one_dirty: DirtySet = [node(n / 2)].into_iter().collect();
    let cached: Vec<f64> = (0..ITERS)
        .map(|_| {
            let t = Instant::now();
            let _ = build_galley_cached(&document, &shaper, WIDTH, &mut cache, &one_dirty);
            t.elapsed().as_secs_f64() * 1e3
        })
        .collect();

    // 4. The real proposed edit path: full incremental pagination (cached galley +
    //    the cheap post-passes), re-shaping only the one edited paragraph.
    let mut cache4 = GalleyCache::new();
    let _ = paginate_document_cached(&document, &shaper, &mut cache4, &DirtySet::everything());
    let cached_pag: Vec<f64> = (0..ITERS)
        .map(|_| {
            let t = Instant::now();
            let _ = paginate_document_cached(&document, &shaper, &mut cache4, &one_dirty);
            t.elapsed().as_secs_f64() * 1e3
        })
        .collect();

    // Correctness: the cached path must produce the identical pagination.
    let mut cache_eq = GalleyCache::new();
    let _ = paginate_document_cached(&document, &shaper, &mut cache_eq, &DirtySet::everything());
    let a = paginate_document(&document, &shaper);
    let b = paginate_document_cached(&document, &shaper, &mut cache_eq, &DirtySet::new());
    assert_eq!(
        a.page_count(),
        b.page_count(),
        "cached pagination diverged from full pagination"
    );

    let pages = a.page_count();
    println!("document: {n} paragraphs, {pages} page(s)\n");
    println!(
        "1. full paginate_document (today's edit path)   : {:>7.2} ms",
        median_ms(full_pag)
    );
    println!(
        "2.   of which build_galley_for_blocks (shape)   : {:>7.2} ms",
        median_ms(full_shape)
    );
    println!(
        "3. build_galley_cached only, 1 paragraph dirty  : {:>7.2} ms  ({} re-shaped)",
        median_ms(cached),
        cache.shaped_last_build()
    );
    println!(
        "4. paginate_document_cached, 1 paragraph dirty  : {:>7.2} ms  <-- proposed edit path",
        median_ms(cached_pag)
    );
}
