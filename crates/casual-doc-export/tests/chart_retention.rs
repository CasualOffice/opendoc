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

/// A package carrying one inline chart whose part is `chart`.
///
/// Built here rather than taken from the fixture corpus because the point of the
/// second guard below is a chart the projection covers *completely*, and the
/// committed fixture deliberately carries an out-of-scope `c:trendline` so that it
/// exercises the partial case.
fn package_with_chart(chart: &[u8]) -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let content_types = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/charts/chart1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/></Types>"#;
    let root_rels = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><w:body><w:p><w:r><w:drawing><wp:inline><wp:extent cx="5486400" cy="3200400"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart r:id="rId5"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:body></w:document>"#;
    let document_rels = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="charts/chart1.xml"/></Relationships>"#;

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("[Content_Types].xml", content_types.as_slice()),
        ("_rels/.rels", root_rels.as_slice()),
        ("word/document.xml", document.as_slice()),
        ("word/_rels/document.xml.rels", document_rels.as_slice()),
        ("word/charts/chart1.xml", chart),
    ] {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// A chart every construct of which the projection represents, so its coverage is
/// `Complete` and `ChartCoverage::permits_regeneration` is true.
const COMPLETE_CHART: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:layout/><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:order val="0"/><c:val><c:numRef><c:f>Sheet1!$B$2:$B$3</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>4.30</c:v></c:pt><c:pt idx="1"><c:v>2.5</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser><c:gapWidth val="150"/><c:axId val="111"/><c:axId val="222"/></c:barChart><c:catAx><c:axId val="111"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="222"/></c:catAx><c:valAx><c:axId val="222"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="111"/></c:valAx></c:plotArea><c:plotVisOnly val="1"/><c:dispBlanksAs val="gap"/></c:chart></c:chartSpace>"#;

/// Reads the chart projection out of an import, asserting there is exactly one.
fn single_projection(import: &casual_doc_import::Import) -> &casual_doc_model::v1::Chart {
    let charts = &import.document.definitions().charts;
    assert_eq!(charts.len(), 1, "exactly one chart projection expected");
    charts.iter().next().expect("the projection").1
}

/// Adding the typed projection changes **no output byte** — `docs/155` §6.1
/// consequence 2, and the half of increment 4 a user can actually reach.
///
/// The guard above (`saving_a_document_with_a_chart_changes_no_byte_of_the_chart_closure`)
/// predates the projection and asserts the closure survives. This one asserts the
/// projection **exists** and the closure still survives, which is a different
/// claim: a byte check alone passes vacuously once the reader declines, and a
/// projection check alone says nothing about the output. Both, together, are the
/// invariant.
#[test]
fn the_chart_projection_changes_no_byte_of_the_retained_part() {
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

    // The projection is real: the fixture's cached values are in the model, in
    // their source spelling.
    let chart = single_projection(&import);
    assert_eq!(
        chart.plot_area.groups.len(),
        2,
        "the fixture is a bar+line combo, so the projection holds two groups"
    );
    assert_eq!(
        chart.plot_area.axes.len(),
        3,
        "and three axes, the third being the secondary value axis"
    );
    let first_value = &chart.plot_area.groups[0].series[0].values.points[0].1;
    assert_eq!(
        first_value,
        &casual_doc_model::v1::ChartValue::Number("4.30".to_owned()),
        "the cache is read verbatim"
    );
    // The fixture carries an out-of-scope `c:trendline`. Since `docs/155` §17
    // it is carried verbatim on its series, so it would survive a regenerated
    // save and the projection is complete — but the chart is NOT dirty, so
    // retention still wins and the closure must still come back byte-identical
    // below. That second half is what this test is for.
    assert_eq!(
        chart.coverage,
        casual_doc_model::v1::ChartCoverage::Complete,
        "a carried construct is not a loss"
    );
    assert!(
        chart
            .plot_area
            .groups
            .iter()
            .flat_map(|group| &group.series)
            .any(|series| series
                .retained
                .iter()
                .any(|fragment| fragment.name == "trendline")),
        "the trendline must be carried on its series"
    );
    assert!(!chart.dirty, "an imported chart starts clean");

    // And every part of the closure still comes back byte-identical.
    let written = write_document_with_retained_parts(
        &import.document,
        &BTreeMap::new(),
        &import.retained_parts,
    )
    .expect("the document writes");
    let reopened = read_all(&written);
    for name in source.keys().filter(|name| !is_regenerated(name)) {
        let before = &source[name];
        let after = reopened
            .get(name)
            .unwrap_or_else(|| panic!("{name} is missing from the written package"));
        assert_identical(name, before, after);
    }
}

/// A chart the projection covers **completely** is still byte-copied.
///
/// This is the guard that the byte-copy is unconditional rather than a side effect
/// of the reader declining. `ChartCoverage::Complete` is the state that *licenses*
/// regeneration, so a chart in that state is the only input on which a writer
/// could legitimately decide to rewrite the part — and it must still not, because
/// nothing has modified the chart. "Clean" beats "licensed".
#[test]
fn a_completely_projected_chart_is_still_byte_copied() {
    let source = package_with_chart(COMPLETE_CHART);
    let mut package =
        DocxPackage::open(&source, PackageLimits::default()).expect("the package opens");
    let import = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("it imports");

    let chart = single_projection(&import);
    assert_eq!(
        chart.coverage,
        casual_doc_model::v1::ChartCoverage::Complete,
        "this chart is entirely tier 1, or the test is not testing what it says"
    );
    assert!(
        chart.coverage.permits_regeneration(),
        "a complete projection is the state that licenses regeneration"
    );

    let written = write_document_with_retained_parts(
        &import.document,
        &BTreeMap::new(),
        &import.retained_parts,
    )
    .expect("the document writes");
    let mut reopened =
        DocxPackage::open(&written, PackageLimits::default()).expect("the written package opens");
    let after = reopened
        .read_part("word/charts/chart1.xml")
        .expect("the chart part reads");
    assert_identical("word/charts/chart1.xml", COMPLETE_CHART, &after);
    assert_eq!(
        reopened.content_type("word/charts/chart1.xml"),
        Some("application/vnd.openxmlformats-officedocument.drawingml.chart+xml"),
        "and it keeps the content type that makes it readable again"
    );
}
