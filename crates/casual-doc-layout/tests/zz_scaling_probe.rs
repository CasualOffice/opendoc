//! Temporary measurement probe (HF-182). Not a guard; deleted before the PR.
use std::time::Instant;

use casual_doc_layout::document_layout::paginate_document_view_cached;
use casual_doc_layout::flow::{ReviewView, build_galley_cached};
use casual_doc_layout::units::Size;
use casual_doc_layout::paginate::{PageConfig, paginate};
use casual_doc_layout::units::Twip;
use casual_doc_layout::incremental::{DirtySet, GalleyCache};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::SectionId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, Paragraph, ParagraphProperties, Run,
    RunProperties,
};

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

fn doc(n: usize) -> Document {
    let line = "The quick brown fox jumps over the lazy dog and keeps on running. ";
    let body = (0..n)
        .map(|i| {
            let id = i as u64 + 1;
            BlockNode::Paragraph(Paragraph {
                id: node(id),
                properties: ParagraphProperties::default().into(),
                inlines: vec![InlineNode::Run(Run {
                    id: node(id + 1_000_000),
                    properties: RunProperties::default().into(),
                    text: line.repeat(3),
                })],
            })
        })
        .collect();
    Document::new(node(9_000_000), body, Definitions::default()).unwrap()
}

fn retype(document: Document, index: usize, keystroke: usize) -> Document {
    let mut body = document.body().to_vec();
    let target = if index == usize::MAX {
        body.len() - 1
    } else {
        index
    };
    if let Some(BlockNode::Paragraph(p)) = body.get_mut(target)
        && let Some(InlineNode::Run(run)) = p.inlines.first_mut()
    {
        run.text.push(char::from(b'a' + (keystroke % 26) as u8));
    }
    let id = document.id();
    let definitions = document.definitions().clone();
    Document::new(id, body, definitions).unwrap()
}

fn probe(n: usize, at: usize, markup: bool) -> (u128, usize, usize) {
    let shaper = ParleyShaper::new();
    let mut document = doc(n);
    let mut cache = GalleyCache::new();
    let view = if markup {
        ReviewView::Markup
    } else {
        ReviewView::Editing
    };
    let layout =
        paginate_document_view_cached(&document, &shaper, &mut cache, &DirtySet::new(), view);
    let pages = layout.pages.len();
    // warm twice so lazy font work is out of the way
    document = retype(document, at, 0);
    let _ =
        paginate_document_view_cached(&document, &shaper, &mut cache, &DirtySet::new(), view);
    let keystrokes = 20;
    let start = Instant::now();
    let mut shaped = 0;
    for k in 1..=keystrokes {
        document = retype(document, at, k);
        let _ =
            paginate_document_view_cached(&document, &shaper, &mut cache, &DirtySet::new(), view);
        shaped += cache.shaped_last_build();
    }
    let per = start.elapsed().as_micros() / keystrokes as u128;
    (per, pages, shaped / keystrokes)
}

/// Split the per-keystroke cost into galley rebuild vs pagination.
fn split_probe(n: usize) -> (u128, u128) {
    let shaper = ParleyShaper::new();
    let mut document = doc(n);
    let mut cache = GalleyCache::new();
    let config = PageConfig {
        section: SectionId::new(node(9)),
        page_size: Size::new(Twip(12_240), Twip(15_840)),
        margin_top: Twip(1_440),
        margin_bottom: Twip(1_440),
        margin_start: Twip(1_440),
        margin_end: Twip(1_440),
        header_distance: Twip(720),
        footer_distance: Twip(720),
        header_height: Twip::ZERO,
        footer_height: Twip::ZERO,
    };
    let width = Twip(9_360);
    let mut galley = build_galley_cached(&document, &shaper, width, &mut cache, &DirtySet::new());
    let _ = paginate(&galley, &config);
    let keystrokes = 20u128;
    let mut galley_us = 0u128;
    let mut paginate_us = 0u128;
    for k in 1..=keystrokes {
        document = retype(document, 0, k as usize);
        let t = Instant::now();
        galley = build_galley_cached(&document, &shaper, width, &mut cache, &DirtySet::new());
        galley_us += t.elapsed().as_micros();
        let t = Instant::now();
        let _ = paginate(&galley, &config);
        paginate_us += t.elapsed().as_micros();
    }
    (galley_us / keystrokes, paginate_us / keystrokes)
}

#[test]
#[ignore]
fn hf182_phase_split() {
    for &n in &[240usize, 480, 960, 1920] {
        let (galley_us, paginate_us) = split_probe(n);
        println!("n={n:5} galley={galley_us:6}us paginate={paginate_us:6}us");
    }
}

#[test]
#[ignore]
fn hf182_scaling_probe() {
    for &markup in &[false, true] {
        for &n in &[240usize, 480, 960] {
            let (first, pages, shaped_first) = probe(n, 0, markup);
            let (last, _, shaped_last) = probe(n, usize::MAX, markup);
            println!(
                "markup={markup} n={n:4} pages={pages:3} \
                 edit_first={first:6}us shaped={shaped_first:4} \
                 edit_last={last:6}us shaped={shaped_last:4}"
            );
        }
    }
}
