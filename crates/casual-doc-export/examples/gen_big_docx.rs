//! Writes an N-paragraph prose `.docx` for latency testing in the WASM viewer.
//! `cargo run --example gen_big_docx -- <out.docx> [paragraph_count]`
#![allow(clippy::print_stderr)]
use std::collections::BTreeMap;

use casual_doc_export::write_document;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, Paragraph, ParagraphProperties, Run,
    RunProperties,
};

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("usage: gen_big_docx <out.docx> [count]");
    let n: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1000);

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
    let document = Document::new(node(9_000_000), body, Definitions::default()).unwrap();
    let bytes = write_document(&document, &BTreeMap::new()).unwrap();
    std::fs::write(&out, &bytes).unwrap();
    eprintln!("wrote {} paragraphs, {} bytes -> {out}", n, bytes.len());
}
