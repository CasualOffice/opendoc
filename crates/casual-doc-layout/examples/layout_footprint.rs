//! Measures what a *laid-out* document costs in memory, per paragraph, in each
//! of the two galley tiers (`docs/113`).
//!
//! `model_footprint.rs` measures the document model. This measures the 12.8 KB
//! per paragraph on top of it that `docs/111` §2 attributed to the shaped
//! galley (4.7 KB) and the paginated layout (8.1 KB) — the half that windowed
//! layout exists to make non-resident.
//!
//! Three phases, each run in its own child process so the resident-set reading
//! is not confused by memory a previous phase freed but the allocator kept:
//!
//! - **`model`** — the document model alone, so the two galley phases can be
//!   read net of the model they both hold.
//! - **`full`** — the model, the whole shaped galley, and a full `paginate`.
//!   This is what opening a document costs today.
//! - **`measure`** — the model, the **measure tier** of the same galley, and
//!   `paginate_measures` with checkpoints. This is what the measure tier is
//!   asking the process to hold once windowing is wired up: enough to know the
//!   exact page count, every page boundary, and where to resume.
//!
//! The measure phase builds its tier in chunks, shaping a chunk and projecting
//! it before shaping the next, so the full galley is never resident. That is a
//! faithful stand-in **only for a body with no cross-chunk flow state** — no
//! list numbering, no section breaks — which is what the synthetic body here
//! is, and the phase asserts the chunked result equals the whole-body result
//! before measuring. The flow engine does not yet emit measures directly;
//! `docs/113` §6 step 4 records that.
//!
//! ```text
//! cargo run --release --example layout_footprint [paragraph_count]
//! cargo run --release --example layout_footprint [paragraph_count] model
//! cargo run --release --example layout_footprint [paragraph_count] full
//! cargo run --release --example layout_footprint [paragraph_count] measure
//! ```
#![allow(clippy::print_stdout)] // a manual measurement, not library code
use std::mem::size_of;

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::flow::build_galley_for_blocks;
use casual_doc_layout::measure::FragmentMeasure;
use casual_doc_layout::measure::LineMeasure;
use casual_doc_layout::measure::PageOutline;
use casual_doc_layout::measure::measure_galley;
use casual_doc_layout::page::Page;
use casual_doc_layout::page::PlacedFragment;
use casual_doc_layout::paginate::Checkpoint;
use casual_doc_layout::paginate::DEFAULT_CHECKPOINT_INTERVAL;
use casual_doc_layout::paginate::PageConfig;
use casual_doc_layout::paginate::paginate;
use casual_doc_layout::paginate::paginate_measures;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::Line;
use casual_doc_layout::units::Size;
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use casual_doc_model::v1::BlockNode;
use casual_doc_model::v1::Definitions;
use casual_doc_model::v1::Document;
use casual_doc_model::v1::InlineNode;
use casual_doc_model::v1::Paragraph;
use casual_doc_model::v1::ParagraphProperties;
use casual_doc_model::v1::Run;
use casual_doc_model::v1::RunProperties;
use casual_doc_model::v1::SectionId;

/// Paragraphs shaped before the shaped form is projected and dropped.
const CHUNK: usize = 4_096;

/// The owner's file, for the projection line.
const OWNER_PARAGRAPHS: u64 = 1_303_306;

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

/// Resident set size of this process in bytes, via `ps` (kibibytes on both
/// macOS and Linux). `None` when `ps` is unavailable.
fn resident_bytes() -> Option<u64> {
    let pid = std::process::id().to_string();
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    text.trim().parse::<u64>().ok().map(|kib| kib * 1024)
}

/// A body of `n` paragraphs, each 130-character prose in a single run — the
/// same synthetic shape `docs/111` §2 and `model_footprint.rs` measure.
fn big_document(n: u64) -> Document {
    const SENTENCE: &str = "The quick brown fox jumps over the lazy dog while the \
        editor reflows this paragraph and every other one on the page.";
    let body: Vec<BlockNode> = (0..n)
        .map(|i| {
            let id = i + 1;
            BlockNode::Paragraph(Paragraph {
                id: node(id),
                properties: ParagraphProperties::default(),
                inlines: vec![InlineNode::Run(Run {
                    id: node(id + 10_000_000),
                    properties: RunProperties::default(),
                    text: SENTENCE.to_owned(),
                })],
            })
        })
        .collect();
    Document::new(node(99_000_000), body, Definitions::default())
        .expect("the synthetic body is valid")
}

/// US-Letter with 1-inch margins — a 9360×12960-twip content area, the same
/// geometry `docs/111` §2 measured at.
fn letter_config() -> PageConfig {
    PageConfig {
        section: SectionId::new(node(98_000_000)),
        page_size: Size::new(Twip(12_240), Twip(15_840)),
        margin_top: Twip(1_440),
        margin_bottom: Twip(1_440),
        margin_start: Twip(1_440),
        margin_end: Twip(1_440),
        header_distance: Twip(720),
        footer_distance: Twip(720),
        header_height: Twip::ZERO,
        footer_height: Twip::ZERO,
    }
}

/// The measure tier of a document's body, shaped a chunk at a time so the full
/// galley is never resident.
fn chunked_measures(
    document: &Document,
    shaper: &ParleyShaper,
    width: Twip,
) -> Vec<FragmentMeasure> {
    let body = document.body();
    let mut measures = Vec::with_capacity(body.len());
    let mut start = 0;
    while start < body.len() {
        let end = (start + CHUNK).min(body.len());
        let chunk = build_galley_for_blocks(document, shaper, &body[start..end], width);
        measures.extend(chunk.iter().map(FragmentMeasure::of));
        start = end;
    }
    measures
}

/// The resident bytes a `Vec<Checkpoint>` holds, counting each one's heap.
fn checkpoint_bytes(checkpoints: &[Checkpoint]) -> usize {
    std::mem::size_of_val(checkpoints)
        + checkpoints
            .iter()
            .map(|c| c.table_headers.len() * size_of::<u32>())
            .sum::<usize>()
}

fn row(name: &str, bytes: usize) {
    println!("{name:<28} {bytes:>6}");
}

fn sizes() {
    println!("-- size_of, paint tier --");
    row("BlockFragment", size_of::<BlockFragment>());
    row("Line", size_of::<Line>());
    row("PlacedFragment", size_of::<PlacedFragment>());
    row("Page", size_of::<Page>());
    println!("\n-- size_of, measure tier --");
    row("FragmentMeasure", size_of::<FragmentMeasure>());
    row("LineMeasure", size_of::<LineMeasure>());
    row("PageOutline", size_of::<PageOutline>());
    row("Checkpoint", size_of::<Checkpoint>());
}

#[allow(clippy::cast_precision_loss)] // reporting, not arithmetic we branch on
fn report(phase: &str, n: u64, delta: u64, pages: usize, extra: &str) {
    let per = delta as f64 / n as f64;
    let projected = per * OWNER_PARAGRAPHS as f64 / 1_073_741_824.0;
    println!("\n-- {phase}: {n} paragraphs --");
    println!("pages             {pages:>12}");
    println!("resident          {delta:>12} B");
    println!("per paragraph     {per:>12.0} B");
    println!("1.3M projection   {projected:>12.2} GiB");
    if !extra.is_empty() {
        println!("{extra}");
    }
    // The one line a caller greps for.
    println!("RESULT {phase} n={n} pages={pages} bytes={delta} per_paragraph={per:.0}");
}

/// The `model` phase: the document model alone, so the two galley phases can
/// be read net of the model they both hold (`docs/111` §4 stage 1b measured
/// this at 1,004 B/paragraph with `model_footprint.rs`).
fn phase_model(n: u64) {
    let warm = big_document(1_000);
    drop(warm);
    let Some(before) = resident_bytes() else {
        println!("(no `ps`: resident measurement skipped)");
        return;
    };
    let document = big_document(n);
    let after = resident_bytes().expect("`ps` answered once already");
    report("model", n, after.saturating_sub(before), 0, "");
    assert_eq!(document.body().len(), usize::try_from(n).unwrap());
}

/// The `full` phase: model + whole shaped galley + full pagination.
fn phase_full(n: u64) {
    let shaper = ParleyShaper::new();
    let config = letter_config();
    let width = config.content_area().size.width;

    // Warm the shaper and the allocator so first-touch growth is not charged to
    // the measured document.
    let warm = big_document(1_000);
    let warm_galley = build_galley_for_blocks(&warm, &shaper, warm.body(), width);
    let warm_pages = paginate(&warm_galley, &config).pages.len();
    drop(warm_galley);
    drop(warm);

    let Some(before) = resident_bytes() else {
        println!("(no `ps`: resident measurement skipped)");
        return;
    };
    let document = big_document(n);
    let galley = build_galley_for_blocks(&document, &shaper, document.body(), width);
    let layout = paginate(&galley, &config);
    let after = resident_bytes().expect("`ps` answered once already");

    report(
        "full",
        n,
        after.saturating_sub(before),
        layout.pages.len(),
        &format!("(warm-up paginated {warm_pages} pages)"),
    );
    // Keep everything alive across the reading.
    assert_eq!(galley.len(), usize::try_from(n).unwrap());
    assert!(!layout.pages.is_empty());
}

/// The `measure` phase: model + measure tier + measure pagination.
fn phase_measure(n: u64) {
    let shaper = ParleyShaper::new();
    let config = letter_config();
    let width = config.content_area().size.width;

    // Warm up, and prove the chunked build is the same tier the whole-body
    // build produces — otherwise the number below would be measuring a
    // different galley from the one `full` measures.
    let warm = big_document(1_000);
    let whole = measure_galley(&build_galley_for_blocks(&warm, &shaper, warm.body(), width));
    let chunked = chunked_measures(&warm, &shaper, width);
    assert_eq!(
        whole, chunked,
        "chunked shaping must produce the same measure tier as a whole-body build"
    );
    drop(whole);
    drop(chunked);
    drop(warm);

    let Some(before) = resident_bytes() else {
        println!("(no `ps`: resident measurement skipped)");
        return;
    };
    let document = big_document(n);
    let measures = chunked_measures(&document, &shaper, width);
    let layout = paginate_measures(&measures, &config, DEFAULT_CHECKPOINT_INTERVAL);
    let after = resident_bytes().expect("`ps` answered once already");

    let checkpoints = checkpoint_bytes(&layout.checkpoints);
    report(
        "measure",
        n,
        after.saturating_sub(before),
        layout.pages.len(),
        &format!(
            "checkpoints       {:>12} ({checkpoints} B total, every \
             {DEFAULT_CHECKPOINT_INTERVAL} pages)",
            layout.checkpoints.len()
        ),
    );
    assert_eq!(measures.len(), usize::try_from(n).unwrap());
    assert!(!layout.pages.is_empty());
}

/// Runs this example again in a child process for one phase, echoing its
/// output, so each phase gets a clean resident-set baseline.
fn run_phase_in_child(n: u64, phase: &str) {
    let exe = std::env::current_exe().expect("the running example has a path");
    let output = std::process::Command::new(exe)
        .args([n.to_string(), phase.to_owned()])
        .output()
        .expect("re-running this example as a child process");
    print!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(output.status.success(), "the {phase} phase failed");
}

fn main() {
    let n: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);
    match std::env::args().nth(2).as_deref() {
        Some("model") => phase_model(n),
        Some("full") => phase_full(n),
        Some("measure") => phase_measure(n),
        _ => {
            sizes();
            run_phase_in_child(n, "model");
            run_phase_in_child(n, "full");
            run_phase_in_child(n, "measure");
        }
    }
}
