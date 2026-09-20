//! Measures what the document model costs in memory, per paragraph.
//!
//! `docs/111` (large-document memory) is written against two numbers that have
//! to be measured rather than reasoned about:
//!
//! 1. `size_of` for the node enums and their payloads. A `Vec<BlockNode>` pays
//!    for the enum's *largest* variant on every element, so the enum's size —
//!    not the size of the common payload — is what the document is charged.
//! 2. The resident bytes a real body of N paragraphs actually occupies, which
//!    includes every `Vec` and `String` allocation the `size_of` table cannot
//!    see.
//!
//! `docs/105`'s evidence rules require a published number to derive from a
//! committed artifact; this example is that artifact for `docs/111` §3.1 and
//! §4. Run it as:
//!
//! ```text
//! cargo run --release --example model_footprint [paragraph_count]
//! ```
#![allow(clippy::print_stdout)] // a manual measurement, not library code
use std::mem::size_of;

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

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

/// Resident set size of this process in bytes, via `ps`.
///
/// `ps -o rss=` reports kibibytes on both macOS and Linux. Returns `None` when
/// `ps` is unavailable, in which case the caller prints the `size_of` table
/// alone.
fn resident_bytes() -> Option<u64> {
    let pid = std::process::id().to_string();
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    text.trim().parse::<u64>().ok().map(|kib| kib * 1024)
}

/// A body of `n` paragraphs, each one 130-character prose in a single run —
/// the same synthetic shape `docs/111` §2 measured.
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

fn main() {
    let n: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);

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

    // Resident bytes for a real body. Warm the allocator first so first-touch
    // growth is not charged to the measured document.
    let warmup = big_body(1_000);
    let warm_len = warmup.len();
    drop(warmup);
    let Some(before) = resident_bytes() else {
        println!("\n(no `ps`: resident measurement skipped)");
        return;
    };

    let body = big_body(n);
    let document = Document::new(node(99_000_000), body, Definitions::default())
        .expect("the synthetic body is valid");
    let after = resident_bytes().expect("`ps` answered once already");

    let delta = after.saturating_sub(before);
    #[allow(clippy::cast_precision_loss)] // reporting, not arithmetic we branch on
    let per_paragraph = delta as f64 / n as f64;
    println!("\n-- resident model, {n} paragraphs (warm-up {warm_len}) --");
    println!("baseline RSS      {before:>12} B");
    println!("with document     {after:>12} B");
    println!("model             {delta:>12} B");
    println!("per paragraph     {per_paragraph:>12.0} B");

    // Keep the document alive across the measurement.
    assert_eq!(document.body().len(), usize::try_from(n).unwrap());
}
