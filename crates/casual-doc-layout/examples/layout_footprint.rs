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
//!   `paginate_measures` with checkpoints, built by shaping in chunks and
//!   projecting each chunk. This is the step-2 stand-in `docs/113` §6.1
//!   describes, kept so the two ways of getting the tier can be compared.
//! - **`stream`** — the same thing through the real seam: the flow engine
//!   emits the measure tier directly (`build_measures_for_blocks`, `docs/113`
//!   §6 step 4), so the shaped galley never exists at all. This is the phase
//!   whose PEAK is the number that decides whether a document opens.
//! - **`window`** — model + measure tier + one materialized window of pages,
//!   which is what a viewer holds while someone reads page N.
//!
//! **Peak, not only resident.** Resident set at the end of a phase says what
//! the process still holds; it says nothing about the high-water mark it had
//! to reach to get there, and the high-water mark is what fails an allocation.
//! Each phase runs in its own child process and the parent samples that
//! child's RSS every few milliseconds, reporting the maximum — so both numbers
//! come from the same run.
//!
//! ```text
//! cargo run --release --example layout_footprint [paragraph_count]
//! cargo run --release --example layout_footprint [paragraph_count] model
//! cargo run --release --example layout_footprint [paragraph_count] full
//! cargo run --release --example layout_footprint [paragraph_count] measure
//! cargo run --release --example layout_footprint [paragraph_count] stream
//! cargo run --release --example layout_footprint [paragraph_count] window
//! ```
#![allow(clippy::print_stdout)] // a manual measurement, not library code
use std::io::Read as _;
use std::mem::size_of;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::flow::build_galley_for_blocks;
use casual_doc_layout::flow::build_measures_for_blocks;
use casual_doc_layout::incremental::PageRange;
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
use casual_doc_layout::windowed::WindowPolicy;
use casual_doc_layout::windowed::measure_document;
use casual_doc_layout::windowed::window_of;
use casual_doc_model::NodeId;
use casual_doc_model::v1::BlockNode;
use casual_doc_model::v1::Definitions;
use casual_doc_model::v1::Document;
use casual_doc_model::v1::InlineNode;
use casual_doc_model::v1::PageMargins;
use casual_doc_model::v1::PageSize;
use casual_doc_model::v1::Paragraph;
use casual_doc_model::v1::ParagraphProperties;
use casual_doc_model::v1::Run;
use casual_doc_model::v1::RunProperties;
use casual_doc_model::v1::SectionBoundary;
use casual_doc_model::v1::SectionColumns;
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
    Document::new(node(99_000_000), big_body(n), Definitions::default())
        .expect("the synthetic body is valid")
}

/// The body [`big_document`] wraps, so a variant can reuse it without cloning
/// a million paragraphs to swap the definitions.
fn big_body(n: u64) -> Vec<BlockNode> {
    const SENTENCE: &str = "The quick brown fox jumps over the lazy dog while the \
        editor reflows this paragraph and every other one on the page.";
    (0..n)
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
        .collect()
}

/// [`big_document`] with an explicit US-Letter section, so the windowed
/// driver has the single section it needs. `big_document` declares none, which
/// the full driver falls back to US-Letter for — the same geometry, but
/// `measure_document` refuses a sectionless body rather than guessing.
fn windowable_document(n: u64) -> Document {
    let definitions = Definitions {
        sections: vec![letter_section()],
        ..Definitions::default()
    };
    Document::new(node(99_000_000), big_body(n), definitions).expect("the synthetic body is valid")
}

/// The section `letter_config` describes, in model form.
fn letter_section() -> SectionBoundary {
    SectionBoundary {
        id: SectionId::new(node(98_000_000)),
        page_size: PageSize {
            width_twips: 12_240,
            height_twips: 15_840,
        },
        page_margins: PageMargins {
            top_twips: 1_440,
            bottom_twips: 1_440,
            start_twips: 1_440,
            end_twips: 1_440,
            header_twips: None,
            footer_twips: None,
            gutter_twips: None,
        },
        columns: SectionColumns {
            count: 1,
            space_twips: None,
            separator: None,
            equal_width: None,
            columns: Vec::new(),
        },
        headers: Vec::new(),
        footers: Vec::new(),
        section_type: None,
        title_page: None,
        vertical_alignment: None,
        page_numbering: Default::default(),
        doc_grid: Default::default(),
        orientation: None,
        paper_source: Default::default(),
        page_borders: Default::default(),
        line_numbering: Default::default(),
        footnote_props: Default::default(),
        endnote_props: Default::default(),
        text_direction: None,
        bidi: false,
        section_change: None,
    }
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

/// The `stream` phase: model + measure tier emitted **directly by the flow
/// engine**, plus measure pagination.
///
/// The difference from `measure` is not the answer — the phase asserts the two
/// tiers are equal on a warm-up body — it is the peak. `measure` shapes 4,096
/// paragraphs at a time; `stream` shapes two.
fn phase_stream(n: u64) {
    let shaper = ParleyShaper::new();
    let config = letter_config();
    let width = config.content_area().size.width;

    let warm = big_document(1_000);
    let projected = measure_galley(&build_galley_for_blocks(&warm, &shaper, warm.body(), width));
    let (streamed, marks) = build_measures_for_blocks(&warm, &shaper, warm.body(), width);
    assert_eq!(
        projected, streamed,
        "streaming the measure tier must equal projecting the whole galley"
    );
    assert_eq!(marks.len(), warm.body().len());
    drop(projected);
    drop(streamed);
    drop(warm);

    let Some(before) = resident_bytes() else {
        println!("(no `ps`: resident measurement skipped)");
        return;
    };
    let started = Instant::now();
    let document = big_document(n);
    let (measures, _marks) = build_measures_for_blocks(&document, &shaper, document.body(), width);
    let layout = paginate_measures(&measures, &config, DEFAULT_CHECKPOINT_INTERVAL);
    let elapsed = started.elapsed();
    let after = resident_bytes().expect("`ps` answered once already");

    let checkpoints = checkpoint_bytes(&layout.checkpoints);
    report(
        "stream",
        n,
        after.saturating_sub(before),
        layout.pages.len(),
        &format!(
            "checkpoints       {:>12} ({checkpoints} B total, every \
             {DEFAULT_CHECKPOINT_INTERVAL} pages)\nelapsed           {:>12.2} s",
            layout.checkpoints.len(),
            elapsed.as_secs_f64(),
        ),
    );
    assert_eq!(measures.len(), usize::try_from(n).unwrap());
    assert!(!layout.pages.is_empty());
}

/// The `owner` phase: the shape of the file this whole line of work exists
/// for — 1,303,306 paragraphs, each the single line that file repeats — opened
/// end to end through the windowed driver, then scrolled to a page three
/// quarters of the way down.
///
/// The file itself is the owner's and is not in the repository, so the shape
/// is reproduced here rather than read: `file(1)` reports it as ASCII text
/// with CRLF terminators, `sort -u` over it yields exactly one distinct line,
/// and `wc -l` counts 1,303,305 of them — which is the paragraph list below,
/// minus the trailing one the plain-text adapter emits. A memory and timing
/// measurement of that shape is a measurement of that file; what it is not is
/// a measurement of the browser host, which has its own ceiling.
fn phase_owner(_n: u64) {
    let shaper = ParleyShaper::new();
    let warm = windowable_document(1_000);
    let _ = measure_document(&warm, &shaper);
    drop(warm);

    let Some(before) = resident_bytes() else {
        println!("(no `ps`: resident measurement skipped)");
        return;
    };
    let built = Instant::now();
    let document = owner_document();
    let model_time = built.elapsed();
    let after_model = resident_bytes().expect("`ps` answered once already");

    let opened = Instant::now();
    let measures = match measure_document(&document, &shaper) {
        Ok(measures) => measures,
        Err(refusal) => {
            println!("REFUSED owner reason={}", refusal.reason());
            return;
        }
    };
    let open_time = opened.elapsed();
    let after_open = resident_bytes().expect("`ps` answered once already");
    let total = measures.page_count();

    let visible = PageRange::new(total * 3 / 4, (total * 3 / 4 + 2).min(total));
    let scrolled = Instant::now();
    let window = window_of(
        &document,
        &shaper,
        &measures,
        visible,
        WindowPolicy::default(),
    );
    let scroll_time = scrolled.elapsed();
    let after_window = resident_bytes().expect("`ps` answered once already");

    let n = OWNER_PARAGRAPHS;
    report(
        "owner",
        n,
        after_window.saturating_sub(before),
        total,
        &format!(
            "model             {:>12} B in {:.2} s\n\
             + measure tier    {:>12} B in {:.2} s\n\
             + one window      {:>12} B in {:.3} s\n\
             page outlines     {:>12} B\n\
             checkpoints       {:>12}\n\
             window pages      {:>12}\n\
             window paint      {:>12} B\n\
             window reshaped   {:>12} fragments\n\
             window resumed at {:>12}",
            after_model.saturating_sub(before),
            model_time.as_secs_f64(),
            after_open.saturating_sub(after_model),
            open_time.as_secs_f64(),
            after_window.saturating_sub(after_open),
            scroll_time.as_secs_f64(),
            measures.resident_bytes(),
            measures.checkpoints.len(),
            window.viewport.pages.len(),
            window.bytes,
            window.shaped_fragments,
            window.resumed_at_page,
        ),
    );
    assert!(!window.viewport.pages.is_empty());
}

/// The `owner_full` phase: the **production** path — `paginate_document`, the
/// entry `casual-doc-wasm` calls on open — over the owner's paragraph shape,
/// at `n` paragraphs.
///
/// The comparison `owner` needs. It takes `n` rather than the owner's full
/// count because the full path at 1,303,306 paragraphs does not fit this
/// machine, which is the finding rather than a gap in it: run it at a size
/// that fits, and the per-paragraph cost is measured on the owner's own
/// paragraph shape instead of on a 130-character stand-in.
fn phase_owner_full(n: u64) {
    let shaper = ParleyShaper::new();
    let warm = owner_document_of(1_000);
    let _ = paginate_document(&warm, &shaper);
    drop(warm);

    let Some(before) = resident_bytes() else {
        println!("(no `ps`: resident measurement skipped)");
        return;
    };
    let started = Instant::now();
    let document = owner_document_of(n);
    let layout = paginate_document(&document, &shaper);
    let elapsed = started.elapsed();
    let after = resident_bytes().expect("`ps` answered once already");

    report(
        "owner_full",
        n,
        after.saturating_sub(before),
        layout.pages.len(),
        &format!("elapsed           {:>12.2} s", elapsed.as_secs_f64()),
    );
    assert!(!layout.pages.is_empty());
}

/// The owner's file as a model: one paragraph per line, one run per paragraph.
fn owner_document() -> Document {
    owner_document_of(OWNER_PARAGRAPHS)
}

/// [`owner_document`] truncated to `n` paragraphs.
fn owner_document_of(n: u64) -> Document {
    // The single distinct line the file repeats (`sort -u` yields exactly one).
    const LINE: &str = "examplefile.com - Sample Files";
    let body: Vec<BlockNode> = (0..n)
        .map(|i| {
            let id = i + 1;
            BlockNode::Paragraph(Paragraph {
                id: node(id),
                properties: ParagraphProperties::default(),
                inlines: vec![InlineNode::Run(Run {
                    id: node(id + 2_000_000_000),
                    properties: RunProperties::default(),
                    text: LINE.to_owned(),
                })],
            })
        })
        .collect();
    let definitions = Definitions {
        sections: vec![letter_section()],
        ..Definitions::default()
    };
    Document::new(node(1_900_000_000), body, definitions).expect("the owner shape is valid")
}

/// The `window` phase: what a viewer holds while someone reads one page —
/// the model, the measure tier, and the paint tier of one window.
fn phase_window(n: u64) {
    let shaper = ParleyShaper::new();
    let warm = big_document(1_000);
    let _ = measure_document(&warm, &shaper);
    drop(warm);

    let Some(before) = resident_bytes() else {
        println!("(no `ps`: resident measurement skipped)");
        return;
    };
    let document = windowable_document(n);
    let opened = Instant::now();
    let measures = measure_document(&document, &shaper).expect("the synthetic body is windowable");
    let open_time = opened.elapsed();
    let total = measures.page_count();

    // A window deep in the document, which is the case windowing exists for.
    let visible = PageRange::new(total * 3 / 4, (total * 3 / 4 + 2).min(total));
    let scrolled = Instant::now();
    let window = window_of(
        &document,
        &shaper,
        &measures,
        visible,
        WindowPolicy::default(),
    );
    let scroll_time = scrolled.elapsed();
    let after = resident_bytes().expect("`ps` answered once already");

    report(
        "window",
        n,
        after.saturating_sub(before),
        total,
        &format!(
            "open              {:>12.2} s\nmeasure tier      {:>12} B\n\
             window pages      {:>12}\nwindow paint      {:>12} B\n\
             window reshaped   {:>12} fragments\nwindow resumed at {:>12}\n\
             scroll            {:>12.3} s",
            open_time.as_secs_f64(),
            measures.resident_bytes(),
            window.viewport.pages.len(),
            window.bytes,
            window.shaped_fragments,
            window.resumed_at_page,
            scroll_time.as_secs_f64(),
        ),
    );
    assert!(!window.viewport.pages.is_empty());
}

/// Runs this example again in a child process for one phase, echoing its
/// output and sampling the child's resident set so the reported **peak** is
/// the high-water mark the phase actually reached.
///
/// Sampled rather than read from `getrusage`, because the workspace forbids
/// `unsafe` and has no `libc` dependency; at a 4 ms period over a phase that
/// runs for seconds the sample is tight, and it is reported as a sample rather
/// than as an exact maximum.
fn run_phase_in_child(n: u64, phase: &str) {
    let exe = std::env::current_exe().expect("the running example has a path");
    let mut child = std::process::Command::new(exe)
        .args([n.to_string(), phase.to_owned()])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("re-running this example as a child process");
    let pid = child.id();
    let peak = Arc::new(AtomicU64::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let sampler = {
        let peak = Arc::clone(&peak);
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                if let Some(rss) = process_resident_bytes(pid) {
                    peak.fetch_max(rss, Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(4));
            }
        })
    };
    let mut text = String::new();
    if let Some(stdout) = child.stdout.as_mut() {
        stdout.read_to_string(&mut text).ok();
    }
    let status = child.wait().expect("waiting for the child phase");
    done.store(true, Ordering::Relaxed);
    sampler.join().ok();
    print!("{text}");
    let sampled = peak.load(Ordering::Relaxed);
    println!("PEAK {phase} n={n} sampled_peak_rss_bytes={sampled}");
    assert!(status.success(), "the {phase} phase failed");
}

/// Resident set size of process `pid` in bytes, via `ps`.
fn process_resident_bytes(pid: u32) -> Option<u64> {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    text.trim().parse::<u64>().ok().map(|kib| kib * 1024)
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
        Some("stream") => phase_stream(n),
        Some("window") => phase_window(n),
        Some("owner") => phase_owner(n),
        Some("owner_full") => phase_owner_full(n),
        _ => {
            sizes();
            run_phase_in_child(n, "model");
            run_phase_in_child(n, "full");
            run_phase_in_child(n, "measure");
            run_phase_in_child(n, "stream");
            run_phase_in_child(n, "window");
        }
    }
}
