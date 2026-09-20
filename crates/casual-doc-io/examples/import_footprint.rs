//! Measures what **importing** a large document costs in memory — both the
//! resident model it leaves behind and the **peak** it reaches getting there.
//!
//! `casual-doc-layout`'s `model_footprint` example measures a body built by
//! hand, and `layout_footprint` measures the galley and the paginated layout.
//! Neither measures the path a file actually takes, and the difference is not
//! small: a body assembled with `vec![one_inline]` pays no `Vec` capacity
//! slack, while the same body assembled by `Vec::push` — which is what every
//! importer does — pays `RawVec::MIN_NON_ZERO_CAP`, four elements, for a
//! paragraph holding one (`docs/111` §4a).
//!
//! **Peak, not only resident.** A tab dies on an allocation that cannot be
//! served, so the high-water mark is the number that decides whether a
//! document opens. The import runs in a child process whose resident set the
//! parent samples, so peak and resident come from the same run.
//!
//! The synthetic input is the shape of the owner's own file: `paragraphs`
//! lines of `chars` ASCII characters, newline-separated, fed to the registry
//! exactly as the browser feeds a picked file — detection first, then import
//! with `retain_source` on.
//!
//! ```text
//! cargo run --release --example import_footprint [paragraphs] [chars]
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

use casual_doc_io::DetectionRequest;
use casual_doc_io::FormatRegistry;
use casual_doc_io::FormatSelection;
use casual_doc_io::PlainTextAdapter;
use casual_doc_io::PlainTextLimits;
use casual_doc_model::v1::BlockNode;
use casual_doc_model::v1::InlineNode;

/// The owner's file, for the projection line (`docs/111` §1).
const OWNER_PARAGRAPHS: u64 = 1_303_306;

/// Resident set size of process `pid` in bytes, via `ps` (kibibytes on both
/// macOS and Linux).
fn process_resident_bytes(pid: u32) -> Option<u64> {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    text.trim().parse::<u64>().ok().map(|kib| kib * 1024)
}

/// Resident set size of this process in bytes.
fn resident_bytes() -> Option<u64> {
    process_resident_bytes(std::process::id())
}

/// The owner's file shape: `n` newline-separated lines of `chars` characters.
fn synthetic_source(n: u64, chars: usize) -> Vec<u8> {
    const PROSE: &str = "examplefile.com - Sample Files ";
    let line: String = PROSE.chars().cycle().take(chars).collect();
    let mut text = String::with_capacity((chars + 1) * usize::try_from(n).unwrap_or(0));
    for index in 0..n {
        if index != 0 {
            text.push('\n');
        }
        text.push_str(&line);
    }
    text.into_bytes()
}

/// Reports where the imported body's bytes are, read off the live structure.
#[allow(clippy::cast_precision_loss)] // reporting, not arithmetic we branch on
fn itemise(body: &[BlockNode], n: u64) {
    let inline_slot = size_of::<InlineNode>();
    let mut inline_used = 0_usize;
    let mut inline_slack = 0_usize;
    let mut text_bytes = 0_usize;
    for block in body {
        if let BlockNode::Paragraph(paragraph) = block {
            inline_used += paragraph.inlines.len() * inline_slot;
            inline_slack += (paragraph.inlines.capacity() - paragraph.inlines.len()) * inline_slot;
            for inline in &paragraph.inlines {
                if let InlineNode::Run(run) = inline {
                    text_bytes += run.text.capacity();
                }
            }
        }
    }
    let per = |bytes: usize| bytes as f64 / n as f64;
    let used = std::mem::size_of_val(body);
    println!("\n-- itemised import, {n} paragraphs (bytes, then per paragraph) --");
    println!("BlockNode slots (used)     {used:>14}  {:>8.1}", per(used));
    println!(
        "InlineNode slots (used)    {inline_used:>14}  {:>8.1}",
        per(inline_used)
    );
    println!(
        "InlineNode slots (slack)   {inline_slack:>14}  {:>8.1}",
        per(inline_slack)
    );
    println!(
        "run text                   {text_bytes:>14}  {:>8.1}",
        per(text_bytes)
    );
}

/// Imports a synthetic document of `n` paragraphs and reports the resident
/// model it leaves behind.
#[allow(clippy::cast_precision_loss)] // reporting, not arithmetic we branch on
fn phase_import(n: u64, chars: usize) {
    let mut registry = FormatRegistry::new();
    registry
        .register_importer(Arc::new(PlainTextAdapter::new(PlainTextLimits {
            max_input_bytes: 256 * 1024 * 1024,
            max_unicode_scalar_values: 200_000_000,
            max_paragraphs: 8_000_000,
            ..PlainTextLimits::default()
        })))
        .expect("the plain-text importer registers");

    let source = synthetic_source(n, chars);
    let source_bytes = source.len();
    let Some(before) = resident_bytes() else {
        println!("(no `ps`: resident measurement skipped)");
        return;
    };
    let started = Instant::now();
    let imported = registry
        .import(
            DetectionRequest {
                bytes: &source,
                selection: FormatSelection::Auto,
                file_name_hint: None,
                mime_hint: None,
            },
            true,
        )
        .expect("the synthetic plain-text source imports");
    let elapsed = started.elapsed();
    let after = resident_bytes().expect("`ps` answered once already");

    itemise(imported.document.body(), n);
    let delta = after.saturating_sub(before);
    let per_paragraph = delta as f64 / n as f64;
    println!("\n-- resident after import, {n} paragraphs of {chars} chars --");
    println!("source bytes      {source_bytes:>12} B");
    println!("baseline RSS      {before:>12} B");
    println!("after import      {after:>12} B");
    println!("import            {delta:>12} B");
    println!("per paragraph     {per_paragraph:>12.1} B");
    println!("import time       {:>12.2} s", elapsed.as_secs_f64());
    println!(
        "projected for {OWNER_PARAGRAPHS} paragraphs  {:.2} MB",
        per_paragraph * OWNER_PARAGRAPHS as f64 / 1_048_576.0
    );

    assert_eq!(imported.document.body().len(), usize::try_from(n).unwrap());
}

/// Runs this example again in a child process, echoing its output and
/// sampling the child's resident set so the reported **peak** is the
/// high-water mark the import actually reached.
///
/// Sampled rather than read from `getrusage`, because the workspace forbids
/// `unsafe` and has no `libc` dependency; at a 4 ms period over an import
/// that runs for seconds the sample is tight, and it is reported as a sample
/// rather than as an exact maximum.
fn run_in_child(n: u64, chars: usize) {
    let exe = std::env::current_exe().expect("the running example has a path");
    let mut child = std::process::Command::new(exe)
        .args([n.to_string(), chars.to_string(), "child".to_owned()])
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
    let status = child.wait().expect("waiting for the child import");
    done.store(true, Ordering::Relaxed);
    sampler.join().ok();
    print!("{text}");
    let sampled = peak.load(Ordering::Relaxed);
    #[allow(clippy::cast_precision_loss)] // reporting, not arithmetic we branch on
    let per = sampled as f64 / n as f64;
    println!(
        "PEAK import n={n} chars={chars} sampled_peak_rss_bytes={sampled} per_paragraph={per:.1}"
    );
    assert!(status.success(), "the import failed");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let n: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(200_000);
    let chars: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(30);
    if args.next().as_deref() == Some("child") {
        phase_import(n, chars);
    } else {
        run_in_child(n, chars);
    }
}
