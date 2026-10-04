//! A diagnostic that imports one real deck and prints what the engine made of it.
//!
//! NOT a guard, and deliberately not one: it reads a file from `fixtures/local/`,
//! which is gitignored because a real presentation carries its author's copyright
//! and is mostly media (the first deck tested against this engine was 9.3 MB, of
//! which 92% was images and 0.40 MB was the XML that exercises any code). A test
//! that depended on an untracked file would fail for everyone else.
//!
//! What it is for is the step before a fixture exists: a real file shows which
//! constructs a producer actually writes and in what proportion, and that is what
//! turns "the slides overlap" into "`a:noFill` is reported 90 times". The deck it
//! names is never what a guard asserts against — a synthetic fixture reproducing
//! the STRUCTURE it revealed is.
//!
//! ```text
//! cargo run --quiet -p casual-pres-import --example probe_real
//! cargo run --quiet -p casual-pres-import --example probe_real -- some.pptx
//! ```

// Printing IS this example's output; it has no other product. The workspace
// denies `print_stdout` so a library cannot write to a terminal nobody is
// watching, which is the right default and the wrong rule for a diagnostic.
#![allow(
    clippy::print_stdout,
    reason = "a diagnostic whose entire output is what it prints"
)]

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        // Default to the first deck in the local corpus, so `cargo run ... --example
        // probe_real` with no argument does the useful thing.
        let dir = std::path::Path::new("fixtures/local");
        std::fs::read_dir(dir)
            .ok()
            .and_then(|entries| {
                let mut decks: Vec<_> = entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.extension()
                            .and_then(|ext| ext.to_str())
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("pptx"))
                    })
                    .collect();
                decks.sort();
                decks.pop()
            })
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| {
                panic!("pass a .pptx path, or drop one in fixtures/local/ (see its README)")
            })
    });
    println!("probing {path}\n");
    let bytes = std::fs::read(&path).expect("read");
    let imported = casual_pres_import::import_pptx(
        &bytes,
        casual_doc_package::PackageLimits::default(),
        casual_pres_import::ImportLimits::default(),
    )
    .expect("the real deck imports");
    let p = &imported.presentation;
    println!(
        "slides={} masters={} layouts={}",
        p.slides().len(),
        p.masters().len(),
        p.layouts().len()
    );
    println!(
        "surface = {} x {} EMU",
        p.slide_size().width_emu,
        p.slide_size().height_emu
    );
    // The top losses, by occurrence.
    let mut entries: Vec<_> = imported.report.entries.iter().collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.occurrences));
    println!(
        "\n-- top 25 findings of {} --",
        imported.report.entries.len()
    );
    for e in entries.iter().take(25) {
        println!("{:>5}  {}", e.occurrences, e.feature);
    }
    paint_census(&imported);
    // Slide 1's shapes: where are they, and did they inherit a box?
    for (i, slide) in p.slides().iter().enumerate().take(3) {
        println!(
            "\n-- slide {} ({:?}) : {} shapes --",
            i + 1,
            slide.name,
            slide.shapes.children.len()
        );
        for node in &slide.shapes.children {
            let ph = node
                .placeholder
                .map(|p| format!("{}#{}", p.kind.token(), p.index));
            let geo = match &node.content {
                casual_doc_model::v1::GroupChild::Shape(s) => format!(
                    "sp  off=({},{}) ext=({},{})",
                    s.offset.x_emu, s.offset.y_emu, s.extent.width_emu, s.extent.height_emu
                ),
                casual_doc_model::v1::GroupChild::Picture(s) => format!(
                    "pic off=({},{}) ext=({},{})",
                    s.offset.x_emu, s.offset.y_emu, s.extent.width_emu, s.extent.height_emu
                ),
                casual_doc_model::v1::GroupChild::Group(s) => format!(
                    "grp off=({},{}) ext=({},{})",
                    s.transform.offset.x_emu,
                    s.transform.offset.y_emu,
                    s.transform.extent.width_emu,
                    s.transform.extent.height_emu
                ),
                casual_doc_model::v1::GroupChild::TextBox(_) => "textbox".to_owned(),
            };
            let text = node.text.as_ref().map(|t| {
                let s = t.plain_text();
                s.chars().take(28).collect::<String>().replace('\n', "⏎")
            });
            println!(
                "   {:<46} ph={:<14} {:?}",
                geo,
                ph.unwrap_or_default(),
                text.unwrap_or_default()
            );
        }
    }
}

/// What the deck's shapes STATE about their fill and their outline, per tier.
///
/// The census the `a:noFill` work was read from, and the reason it is here rather
/// than in a guard: the report says how often a construct was LOST, and once the
/// loss is fixed the report goes quiet about it — so a deck's `a:noFill` count stops
/// being visible exactly when it starts mattering. This says how much of a real deck
/// turns on the distinction whether or not anything is still lost.
///
/// `suppressed` is the column to read: every one of those shapes paints nothing and
/// inherits nothing, and before `casual_pres_model::SlidePaint` every one of them
/// was a reported loss instead.
fn paint_census(imported: &casual_pres_import::ImportedPresentation) {
    use casual_pres_model::{SlideNode, SlidePaint};

    let p = &imported.presentation;
    let tiers: [(&str, Vec<&SlideNode>); 3] = [
        (
            "masters",
            p.masters()
                .iter()
                .flat_map(|master| master.shapes.children.iter())
                .collect(),
        ),
        (
            "layouts",
            p.layouts()
                .iter()
                .flat_map(|layout| layout.shapes.children.iter())
                .collect(),
        ),
        (
            "slides",
            p.slides()
                .iter()
                .flat_map(|slide| slide.shapes.children.iter())
                .collect(),
        ),
    ];
    println!("\n-- stated fill / outline, inherited/authored/suppressed --");
    for (tier, nodes) in tiers {
        let tally = |of: &dyn Fn(&SlideNode) -> SlidePaint| {
            let count = |wanted: SlidePaint| nodes.iter().filter(|node| of(node) == wanted).count();
            format!(
                "{}/{}/{}",
                count(SlidePaint::Inherited),
                count(SlidePaint::Authored),
                count(SlidePaint::Suppressed)
            )
        };
        println!(
            "{tier:>8}: {:>4} shapes   fill {:<12} outline {}",
            nodes.len(),
            tally(&|node: &SlideNode| node.fill),
            tally(&|node: &SlideNode| node.outline),
        );
    }
}
