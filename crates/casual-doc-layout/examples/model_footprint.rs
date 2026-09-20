//! Measures what the document model costs in memory, per paragraph.
//!
//! `docs/111` (large-document memory) is written against three numbers that
//! have to be measured rather than reasoned about:
//!
//! 1. `size_of` for the node enums and their payloads. A `Vec<BlockNode>` pays
//!    for the enum's *largest* variant on every element, so the enum's size —
//!    not the size of the common payload — is what the document is charged.
//! 2. The resident bytes a real body of N paragraphs actually occupies, which
//!    includes every `Vec` and `String` allocation the `size_of` table cannot
//!    see, **and every byte of `Vec` capacity those allocations round up to**.
//! 3. The **peak** resident set reached while building it, which is not the
//!    same number: growing a `Vec` of a million paragraphs reallocates, and
//!    for the moment of the copy both the old and the new buffer are live.
//!    Peak is what fails an allocation, so peak is what decides whether a
//!    document opens.
//!
//! `docs/105`'s evidence rules require a published number to derive from a
//! committed artifact; this example is that artifact for `docs/111` §3.1 and
//! §4. Run it as:
//!
//! ```text
//! cargo run --release --example model_footprint [paragraphs] [chars] [shape]
//! ```
//!
//! `chars` is the length of each paragraph's single run — the default 130 is
//! the synthetic prose `docs/111` §2 measured, and **30** is the shape of the
//! owner's own 1,303,306-paragraph file. `shape` is `push` (the default: how a
//! bulk importer builds a body, one `Vec::push` at a time) or `collect`
//! (`vec![one_inline]` and a sized `collect`, which pays no capacity slack and
//! is therefore a floor rather than a measurement of the production path).
#![allow(clippy::print_stdout)] // a manual measurement, not library code
use std::io::Read as _;
use std::mem::size_of;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;

use casual_doc_model::NodeId;
use casual_doc_model::v1::AltChunk;
use casual_doc_model::v1::AnchoredDrawing;
use casual_doc_model::v1::BlockNode;
use casual_doc_model::v1::BlockSdt;
use casual_doc_model::v1::BookmarkEnd;
use casual_doc_model::v1::BookmarkStart;
use casual_doc_model::v1::Break;
use casual_doc_model::v1::CommentRangeEnd;
use casual_doc_model::v1::CommentRangeStart;
use casual_doc_model::v1::CommentReference;
use casual_doc_model::v1::Definitions;
use casual_doc_model::v1::Document;
use casual_doc_model::v1::Drawing;
use casual_doc_model::v1::EmbeddedObject;
use casual_doc_model::v1::Field;
use casual_doc_model::v1::HorizontalRule;
use casual_doc_model::v1::Hyperlink;
use casual_doc_model::v1::InlineNode;
use casual_doc_model::v1::InlineSdt;
use casual_doc_model::v1::Math;
use casual_doc_model::v1::MoveRangeEnd;
use casual_doc_model::v1::MoveRangeStart;
use casual_doc_model::v1::NoBreakHyphen;
use casual_doc_model::v1::NoteNumberMark;
use casual_doc_model::v1::NoteReference;
use casual_doc_model::v1::Paragraph;
use casual_doc_model::v1::ParagraphProperties;
use casual_doc_model::v1::PositionalTab;
use casual_doc_model::v1::Revision;
use casual_doc_model::v1::Run;
use casual_doc_model::v1::RunProperties;
use casual_doc_model::v1::SoftHyphen;
use casual_doc_model::v1::Symbol;
use casual_doc_model::v1::Tab;
use casual_doc_model::v1::Table;
use casual_doc_model::v1::TextBox;
use casual_doc_model::v1::WordprocessingGroup;

/// The owner's file, for the projection line (`docs/111` §1).
const OWNER_PARAGRAPHS: u64 = 1_303_306;

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

/// Resident set size of this process in bytes, via `ps`.
///
/// `ps -o rss=` reports kibibytes on both macOS and Linux. Returns `None` when
/// `ps` is unavailable, in which case the caller prints the `size_of` table
/// alone.
fn resident_bytes() -> Option<u64> {
    process_resident_bytes(std::process::id())
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

/// One paragraph's text: `chars` ASCII characters of prose.
fn sentence(chars: usize) -> String {
    const PROSE: &str = "The quick brown fox jumps over the lazy dog while the \
        editor reflows this paragraph and every other one on the page. ";
    PROSE.chars().cycle().take(chars).collect()
}

/// A body of `n` paragraphs built the way a bulk importer builds one:
/// `Vec::new()` plus `push`, for the body and for each paragraph's `inlines`.
///
/// **This shape is the production path and it is not the cheap one.**
/// `Vec::push` onto an empty vector allocates `RawVec::MIN_NON_ZERO_CAP`
/// elements, which is **4** for any element of 1,024 bytes or less — so a
/// paragraph holding one inline pays for four `InlineNode` slots. The body
/// vector then doubles as it grows, so it finishes holding between 1x and 2x
/// the elements it needs. [`collected_body`] pays neither cost, which is why
/// it measures lower than anything a real import produces.
#[allow(clippy::vec_init_then_push)] // the capacity `push` reserves is what this measures
fn pushed_body(n: u64, chars: usize) -> Vec<BlockNode> {
    let text = sentence(chars);
    let mut body = Vec::new();
    for i in 0..n {
        let id = i + 1;
        // Deliberately `Vec::new()` + `push`: the capacity this rounds up to is
        // half of what this example exists to measure.
        let mut inlines = Vec::new();
        inlines.push(InlineNode::Run(Run {
            id: node(id + 10_000_000),
            properties: RunProperties::default().into(),
            text: text.clone(),
        }));
        body.push(BlockNode::Paragraph(Paragraph {
            id: node(id),
            properties: ParagraphProperties::default().into(),
            inlines,
        }));
    }
    body
}

/// The same body with every vector sized exactly: a floor, not the production
/// path. `docs/111` §4's 1,004 B/paragraph was measured on this shape.
fn collected_body(n: u64, chars: usize) -> Vec<BlockNode> {
    let text = sentence(chars);
    (0..n)
        .map(|i| {
            let id = i + 1;
            BlockNode::Paragraph(Paragraph {
                id: node(id),
                properties: ParagraphProperties::default().into(),
                inlines: vec![InlineNode::Run(Run {
                    id: node(id + 10_000_000),
                    properties: RunProperties::default().into(),
                    text: text.clone(),
                })],
            })
        })
        .collect()
}

/// Prints one `size_of` row.
fn row(name: &str, bytes: usize) {
    println!("{name:<28} {bytes:>6}");
}

/// Prints every `InlineNode` payload, largest first, so the variant that sets
/// the enum's size is named rather than guessed at.
fn inline_variant_sizes() {
    let mut variants: Vec<(&str, usize)> = vec![
        ("Run", size_of::<Run>()),
        ("Tab", size_of::<Tab>()),
        ("Break", size_of::<Break>()),
        ("Drawing", size_of::<Drawing>()),
        ("AnchoredDrawing", size_of::<AnchoredDrawing>()),
        ("EmbeddedObject", size_of::<EmbeddedObject>()),
        ("Hyperlink", size_of::<Hyperlink>()),
        ("Field", size_of::<Field>()),
        ("TextBox", size_of::<TextBox>()),
        ("Group", size_of::<WordprocessingGroup>()),
        ("NoteReference", size_of::<NoteReference>()),
        ("NoteNumberMark", size_of::<NoteNumberMark>()),
        ("CommentReference", size_of::<CommentReference>()),
        ("CommentRangeStart", size_of::<CommentRangeStart>()),
        ("CommentRangeEnd", size_of::<CommentRangeEnd>()),
        ("Revision", size_of::<Revision>()),
        ("BookmarkStart", size_of::<BookmarkStart>()),
        ("BookmarkEnd", size_of::<BookmarkEnd>()),
        ("MoveRangeStart", size_of::<MoveRangeStart>()),
        ("MoveRangeEnd", size_of::<MoveRangeEnd>()),
        ("Sdt", size_of::<InlineSdt>()),
        ("Math", size_of::<Math>()),
        ("Symbol", size_of::<Symbol>()),
        ("HorizontalRule", size_of::<HorizontalRule>()),
        ("NoBreakHyphen", size_of::<NoBreakHyphen>()),
        ("SoftHyphen", size_of::<SoftHyphen>()),
        ("PositionalTab", size_of::<PositionalTab>()),
    ];
    variants.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    println!("\n-- InlineNode payloads, largest first --");
    for (name, bytes) in variants {
        row(&format!("  InlineNode::{name}"), bytes);
    }
}

/// Prints every `BlockNode` payload, largest first.
fn block_variant_sizes() {
    let mut variants: Vec<(&str, usize)> = vec![
        ("Paragraph", size_of::<Paragraph>()),
        ("Table", size_of::<Table>()),
        ("Sdt", size_of::<BlockSdt>()),
        ("AltChunk", size_of::<AltChunk>()),
    ];
    variants.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    println!("\n-- BlockNode payloads, largest first --");
    for (name, bytes) in variants {
        row(&format!("  BlockNode::{name}"), bytes);
    }
}

fn sizes() {
    println!("-- size_of --");
    row("BlockNode", size_of::<BlockNode>());
    row("InlineNode", size_of::<InlineNode>());
    row("Paragraph", size_of::<Paragraph>());
    row("Run", size_of::<Run>());
    row("Table", size_of::<Table>());
    row("ParagraphProperties", size_of::<ParagraphProperties>());
    row("RunProperties", size_of::<RunProperties>());
    block_variant_sizes();
    inline_variant_sizes();
}

/// Walks the built body and reports where its bytes are, by allocation, so the
/// resident figure is itemised rather than attributed by reasoning. Everything
/// here is read off the live structure (`Vec::capacity`, `String::capacity`),
/// not computed from an assumed layout.
#[allow(clippy::cast_precision_loss)] // reporting, not arithmetic we branch on
fn itemise(body: &[BlockNode], capacity: usize, n: u64) {
    let slot = size_of::<BlockNode>();
    let inline_slot = size_of::<InlineNode>();
    let body_used = std::mem::size_of_val(body);
    let body_slack = capacity.saturating_sub(body.len()) * slot;
    let mut inline_used = 0_usize;
    let mut inline_slack = 0_usize;
    let mut text_bytes = 0_usize;
    let mut text_slack = 0_usize;
    for block in body {
        if let BlockNode::Paragraph(paragraph) = block {
            inline_used += paragraph.inlines.len() * inline_slot;
            inline_slack += (paragraph.inlines.capacity() - paragraph.inlines.len()) * inline_slot;
            for inline in &paragraph.inlines {
                if let InlineNode::Run(run) = inline {
                    text_bytes += run.text.len();
                    text_slack += run.text.capacity() - run.text.len();
                }
            }
        }
    }
    let total = body_used + body_slack + inline_used + inline_slack + text_bytes + text_slack;
    let per = |bytes: usize| bytes as f64 / n as f64;
    println!("\n-- itemised, {n} paragraphs (bytes, then per paragraph) --");
    println!(
        "BlockNode slots (used)     {body_used:>14}  {:>8.1}",
        per(body_used)
    );
    println!(
        "BlockNode slots (slack)    {body_slack:>14}  {:>8.1}",
        per(body_slack)
    );
    println!(
        "InlineNode slots (used)    {inline_used:>14}  {:>8.1}",
        per(inline_used)
    );
    println!(
        "InlineNode slots (slack)   {inline_slack:>14}  {:>8.1}",
        per(inline_slack)
    );
    println!(
        "run text (used)            {text_bytes:>14}  {:>8.1}",
        per(text_bytes)
    );
    println!(
        "run text (slack)           {text_slack:>14}  {:>8.1}",
        per(text_slack)
    );
    println!(
        "accounted                  {total:>14}  {:>8.1}",
        per(total)
    );
}

/// Builds `n` paragraphs, reports the resident delta and the itemised
/// breakdown, and keeps the document alive across the measurement.
#[allow(clippy::cast_precision_loss)] // reporting, not arithmetic we branch on
fn phase_body(n: u64, chars: usize, shape: &str) {
    // Warm the allocator first so first-touch growth is not charged to the
    // measured document.
    drop(pushed_body(1_000, chars));
    let Some(before) = resident_bytes() else {
        println!("(no `ps`: resident measurement skipped)");
        return;
    };

    let body = if shape == "collect" {
        collected_body(n, chars)
    } else {
        pushed_body(n, chars)
    };
    let capacity = body.capacity();
    let after = resident_bytes().expect("`ps` answered once already");
    itemise(&body, capacity, n);

    let document = Document::new(node(99_000_000), body, Definitions::default())
        .expect("the synthetic body is valid");
    let delta = after.saturating_sub(before);
    let per_paragraph = delta as f64 / n as f64;
    println!("\n-- resident model, {n} paragraphs of {chars} chars, shape={shape} --");
    println!("baseline RSS      {before:>12} B");
    println!("with document     {after:>12} B");
    println!("model             {delta:>12} B");
    println!("per paragraph     {per_paragraph:>12.1} B");
    println!(
        "projected for {OWNER_PARAGRAPHS} paragraphs  {:.2} MB",
        per_paragraph * OWNER_PARAGRAPHS as f64 / 1_048_576.0
    );

    // Keep the document alive across the measurement.
    assert_eq!(document.body().len(), usize::try_from(n).unwrap());
}

/// Runs this example again in a child process for one shape, echoing its
/// output and sampling the child's resident set so the reported **peak** is
/// the high-water mark the build actually reached.
///
/// Sampled rather than read from `getrusage`, because the workspace forbids
/// `unsafe` and has no `libc` dependency; at a 4 ms period over a build that
/// runs for seconds the sample is tight, and it is reported as a sample rather
/// than as an exact maximum.
fn run_shape_in_child(n: u64, chars: usize, shape: &str) {
    let exe = std::env::current_exe().expect("the running example has a path");
    let mut child = std::process::Command::new(exe)
        .args([n.to_string(), chars.to_string(), shape.to_owned()])
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
    let status = child.wait().expect("waiting for the child build");
    done.store(true, Ordering::Relaxed);
    sampler.join().ok();
    print!("{text}");
    let sampled = peak.load(Ordering::Relaxed);
    #[allow(clippy::cast_precision_loss)] // reporting, not arithmetic we branch on
    let per = sampled as f64 / n as f64;
    println!(
        "PEAK shape={shape} n={n} chars={chars} sampled_peak_rss_bytes={sampled} per_paragraph={per:.1}"
    );
    assert!(status.success(), "the {shape} build failed");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let n: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let chars: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(130);
    match args.next().as_deref() {
        Some(shape @ ("push" | "collect")) => phase_body(n, chars, shape),
        _ => {
            sizes();
            run_shape_in_child(n, chars, "collect");
            run_shape_in_child(n, chars, "push");
        }
    }
}
