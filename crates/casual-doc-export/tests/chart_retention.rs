//! Saving a document with a chart does not change one byte of the chart.
//!
//! This is the invariant `docs/155` §6.1 and ADR-050 part 1 rest on: the typed
//! chart model that this programme is adding is a **read projection** of a
//! retained part, never a replacement for it, so export copies `chart1.xml` and
//! everything reachable from it rather than regenerating them. The guard exists
//! *before* the projection, on purpose — an invariant's guard is worth most when
//! it predates the change that could break it.
//!
//! ## What this adds over what already existed
//!
//! `casual-doc-export`'s own `chart_drawing_round_trips_as_an_editable_reference`
//! already asserts byte-equality of a chart part and an embedded workbook. Its
//! chart, however, is `<c:chartSpace><c:chart/></c:chartSpace>` — two elements
//! and no content. It proves a *reference* survives, which is a different and
//! much smaller claim than a *chart* surviving. This test runs the committed
//! `chart.docx` fixture: a bar+line combo with a secondary axis, cached string and
//! numeric data, an out-of-scope `c:trendline`, and the full relationship closure
//! (`chartColorStyle`, `chartStyle`, an opaque embedded workbook), each part with
//! its own declared content type.
//!
//! ## Why the closure is DERIVED and not listed
//!
//! The parts checked are computed from the source package rather than named here,
//! so a part added to the fixture tomorrow is covered without anybody remembering
//! to extend an assertion. `SKILL` §"guards assert the guarantee, not the
//! circumstance": the guarantee is "every part the chart's closure contains comes
//! back identical", and a hard-coded list of five names asserts something weaker
//! and quietly stops tracking the fixture.
//!
//! A derived set can also pass vacuously, which is the failure mode that makes a
//! guard worse than nothing, so `CLOSURE_FLOOR` pins the two parts whose survival
//! is the actual product claim — the chart and its workbook — and the test fails
//! if the derived set does not contain them.

use std::collections::BTreeMap;

use casual_doc_export::write_document_with_retained_parts;
use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_model::v1::{BlockNode, EmbeddedKind, InlineNode};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

const CHART_DOCX: &[u8] = include_bytes!("../../../fixtures/generated/chart.docx");

/// The parts whose survival is the claim, as opposed to the parts that merely
/// happen to be in the fixture. If the derived closure misses either of these the
/// derivation is broken and the test must say so rather than pass on an empty set.
const CLOSURE_FLOOR: [&str; 2] = [
    "word/charts/chart1.xml",
    "word/embeddings/Microsoft_Excel_Worksheet1.xlsx",
];

/// Package parts that the semantic writer legitimately regenerates: the main
/// document, the content-type manifest, and the two relationship manifests the
/// writer owns. Everything else in a package carrying a chart is closure.
///
/// `word/charts/_rels/chart1.xml.rels` is deliberately NOT here — a part's own
/// `_rels` companion is retained verbatim (`casual-doc-import/src/opaque.rs`
/// `RetainedRels`), and it is the edge that keeps the colour style, the chart
/// style and the workbook reachable, so it is exactly the thing that must not
/// drift.
fn is_regenerated(part_name: &str) -> bool {
    matches!(
        part_name,
        "word/document.xml"
            | "[Content_Types].xml"
            | "_rels/.rels"
            | "word/_rels/document.xml.rels"
    )
}

/// Asserts two part payloads are byte-identical, reporting the first divergence
/// rather than both payloads.
///
/// `assert_eq!` on a pair of `Vec<u8>` prints several thousand decimal byte
/// values, which is the kind of failure output that gets skimmed instead of read.
/// A chart part is a few kilobytes of XML, so the useful report is: how long each
/// side is, where they first differ, and what is around that offset.
fn assert_identical(part_name: &str, before: &[u8], after: &[u8]) {
    if before == after {
        return;
    }
    let at = before
        .iter()
        .zip(after)
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| before.len().min(after.len()));
    let window = |bytes: &[u8]| {
        let start = at.saturating_sub(24);
        let end = (at + 24).min(bytes.len());
        String::from_utf8_lossy(&bytes[start..end]).into_owned()
    };
    panic!(
        "{part_name} is not byte-identical after a save; the chart closure is \
         copied, never regenerated (docs/155 §6.1).\n  \
         source  {} bytes\n  written {} bytes\n  \
         first difference at byte {at}\n    source  …{}…\n    written …{}…",
        before.len(),
        after.len(),
        window(before),
        window(after),
    );
}

fn read_all(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut package =
        DocxPackage::open(bytes, PackageLimits::default()).expect("the package opens");
    let names: Vec<String> = package
        .entries()
        .iter()
        .map(|entry| entry.part_name.clone())
        .collect();
    names
        .into_iter()
        .map(|name| {
            let part = package.read_part(&name).expect("an admitted part reads");
            (name, part)
        })
        .collect()
}

#[test]
fn saving_a_document_with_a_chart_changes_no_byte_of_the_chart_closure() {
    let source = read_all(CHART_DOCX);

    let mut package =
        DocxPackage::open(CHART_DOCX, PackageLimits::default()).expect("the fixture opens");
    let import = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the fixture imports");

    // The chart is a first-class reference, so the writer re-links it rather than
    // leaving the part orphaned. If this regressed the byte checks below would
    // still pass while the document lost its chart, so it is asserted first.
    let chart = import
        .document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(&paragraph.inlines),
            _ => None,
        })
        .flatten()
        .find_map(|inline| match inline {
            InlineNode::EmbeddedObject(object) => Some(object),
            _ => None,
        })
        .expect("the fixture's chart imports as an embedded object");
    assert_eq!(chart.kind, EmbeddedKind::Chart);
    assert_eq!(chart.part.part_name, "word/charts/chart1.xml");

    let written = write_document_with_retained_parts(
        &import.document,
        &BTreeMap::new(),
        &import.retained_parts,
    )
    .expect("the document writes");
    let reopened = read_all(&written);

    let closure: Vec<&String> = source.keys().filter(|name| !is_regenerated(name)).collect();
    for required in CLOSURE_FLOOR {
        assert!(
            closure.iter().any(|name| name.as_str() == required),
            "the derived closure must contain {required}; derived: {closure:?}"
        );
    }

    for name in closure {
        let before = &source[name];
        let after = reopened
            .get(name)
            .unwrap_or_else(|| panic!("{name} is missing from the written package"));
        assert_identical(name, before, after);
    }

    // The cached value `4.30` keeps its trailing zero. This is implied by the
    // byte check above and is asserted separately because it is the one that
    // names the guarantee: `docs/155` §8.2 stores a cached number in its verbatim
    // lexical form precisely because `4.30` and `4.3` are the same number and
    // different documents, so a projection that round-trips a value through an
    // `f64` is wrong in a way no numeric assertion would catch.
    let chart_xml = String::from_utf8(reopened["word/charts/chart1.xml"].clone())
        .expect("the chart part is UTF-8");
    assert!(
        chart_xml.contains("<c:v>4.30</c:v>"),
        "the cached value lost its source spelling"
    );
    // And the out-of-scope construct is still there rather than quietly gone.
    assert!(
        chart_xml.contains("<c:trendline>"),
        "an out-of-scope chart construct must survive, not be dropped"
    );

    // A retained part keeps the content type the producer declared, which is what
    // makes it readable again. Checked through the reopened package's manifest
    // rather than by string-matching the XML.
    let mut written_package =
        DocxPackage::open(&written, PackageLimits::default()).expect("the written package opens");
    for (name, expected) in [
        (
            "word/charts/chart1.xml",
            "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
        ),
        (
            "word/charts/colors1.xml",
            "application/vnd.openxmlformats-officedocument.drawingml.chartColorStyle+xml",
        ),
        (
            "word/charts/style1.xml",
            "application/vnd.openxmlformats-officedocument.drawingml.chartStyle+xml",
        ),
    ] {
        assert_eq!(
            written_package.content_type(name),
            Some(expected),
            "{name} lost its declared content type"
        );
    }

    // Exactly one relationship targets the chart: the node's, not the node's plus
    // an orphan re-add.
    let rels = String::from_utf8(
        written_package
            .read_part("word/_rels/document.xml.rels")
            .expect("the document rels part reads"),
    )
    .expect("the rels part is UTF-8");
    assert_eq!(
        rels.matches("charts/chart1.xml").count(),
        1,
        "the chart relationship must be emitted exactly once: {rels}"
    );
}
