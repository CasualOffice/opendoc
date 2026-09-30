// SPDX-License-Identifier: Apache-2.0

//! The loss-coverage gate: **no element name in the source may vanish from the
//! written package without a compatibility-report entry naming it** (`105`
//! FID-P-03).
//!
//! # Why the suite needed this
//!
//! Every round-trip test in this repository asserts a *fixed point*:
//! `reopen(source) == reopen(write(reopen(source)))`. The left side is the
//! importer's own output, so **anything dropped on first import is a perfect fixed
//! point and passes.** Roughly two hundred `*_survive_the_semantic_round_trip`
//! tests share that blind spot, and nothing in the suite compared output against
//! the *source XML*. A test that cannot fail is worse than no test, because it gets
//! cited as evidence — and this one was, for the claim that direct OOXML beats a
//! `DOCX -> Editor.bin -> DOCX` converter. That claim is only true if loss is
//! detected and reported, which is what this file measures.
//!
//! # The known pattern
//!
//! This is *coverage instrumentation with a justified allowlist*: a coverage report
//! paired with a lint-suppression file that requires a reason per entry
//! (`#[allow(…, reason = "…")]`, `eslint-disable-next-line -- why`, `cargo-deny`'s
//! `[advisories] ignore`). Two properties of the good versions are copied
//! deliberately:
//!
//! 1. **Every exception is enumerated and individually justified** — [`EXCEPTIONS`]
//!    is a table of name and reason, not a set of names.
//! 2. **An exception that no longer fires is itself a failure** — the analogue of
//!    `--report-unused-disable-directives`. Without that, an allowlist grows
//!    monotonically and the gate decays into a no-op, which is how a guard becomes
//!    the thing it was written to prevent.
//!
//! # What this guard does and does not claim
//!
//! It compares **sets of local names over the whole package**, so it is the
//! conservative form of the question and its limits are worth stating:
//!
//! - A name that vanishes from one part while surviving in another is not flagged.
//!   Part identity is not stable across a semantic save (parts are regenerated and
//!   renamed), and an invented per-part comparison would report losses that did not
//!   happen.
//! - A finding aggregates by feature, so a finding raised for one occurrence of a
//!   construct satisfies the guard for every occurrence of that name.
//! - It is about **names**, not values. A construct emitted under a different name
//!   with different content would pass. Attribute names are a separate axis, and
//!   the measured reason they are not yet gated is recorded in
//!   `35-DISPOSITION-TAXONOMY.md`.
//!
//! What it does catch is the whole class that was invisible before: a construct the
//! importer drops and nobody mentions.
//!
//! # Complexity
//!
//! `O(package bytes)` per fixture: each XML part of the source and of the output is
//! walked exactly once and merged into an accumulator. No lookup-by-name runs
//! inside a walk, so it is linear in the package rather than quadratic in its parts.
//! It is a save-time and test-time audit; nothing here is on an edit path.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use casual_doc_export::export_document_with_retained_parts;
use casual_doc_import::{
    CompatibilityReport, ImportConfig, ImportMode, MeaningfulMarkup, RSID_CLASS_FEATURE,
    import_package, meaningful_markup,
};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

/// Source element local names that legitimately vanish from a written package,
/// each with the reason it is not a loss.
///
/// Adding a row here is a **decision that nothing was lost**, and it must say why
/// in terms of the document rather than of the code. The preferred fix is the other
/// one: add the missing report site, so the loss is named instead of excused. Every
/// row is exercised by at least one fixture
/// ([`no_coverage_exception_is_stale`]), so this list cannot accumulate entries
/// that no longer describe anything.
const EXCEPTIONS: &[(&str, &str)] = &[
    (
        "left",
        "FID-P-03: `w:left` is the PHYSICAL spelling of a border, cell-margin or \
         indent edge. The model normalizes physical edges onto logical ones (import \
         `tables.rs` maps `w:left` and `w:start` onto the same field) and the writer \
         emits the logical `w:start`, so the edge itself survives under the other \
         name. What is not modeled is the physical-vs-logical distinction, which \
         differs only in an RTL table; that is a fidelity gap of its own and not \
         something this guard can express, because the construct is present.",
    ),
    (
        "right",
        "FID-P-03: the `w:end` half of the `w:left`/`w:right` normalization above.",
    ),
    (
        "trPr",
        "FID-P-03: real Word and LibreOffice output both write an EMPTY \
         `<w:trPr></w:trPr>` — a row-property container with no property in it. The \
         writer emits `w:trPr` whenever the model holds a row property, and an \
         unmodeled child of a populated one raises a finding under its own name, so \
         the container has no meaning of its own to lose. (Not in the no-op class \
         by name: the class is judged per element, and `<w:trPr>` with children is \
         not a no-op.)",
    ),
    (
        "separator",
        "FID-P-03 / `35` no-op class: the stock rule Word draws above footnotes, \
         which this engine's layout draws for itself. `35` excludes the separator \
         NOTE, decided at the site that can see whether the note holds content of \
         its own; the element inside it cannot be judged by name here, so it is \
         excused here instead of being silenced globally.",
    ),
    (
        "continuationSeparator",
        "FID-P-03 / `35` no-op class: the continuation rule, same reasoning as \
         `separator` above.",
    ),
    (
        "pict",
        "FID-P-03: a VML `w:pict` is modeled (text box, horizontal rule, picture) \
         and re-emitted in the writer's single output vocabulary, DrawingML \
         (`w:drawing`/`wps:wsp`). The construct survives under the modern spelling; \
         not re-emitting VML is a deliberate choice, not a drop.",
    ),
    (
        "fldSimple",
        "FID-P-03: `w:fldSimple` is modeled as a field and re-emitted in the \
         equivalent complex-field form (`w:fldChar` + `w:instrText`), which is the \
         writer's single field spelling. ECMA-376 defines the two as alternative \
         encodings of one field.",
    ),
    (
        "rsids",
        "FID-R-03 / `35`: revision-save-ID markup is dispositioned ONCE per \
         document as the class `docx.rsid`, with an occurrence count, because \
         per-construct reporting would add ~14 rows to a report that holds 28-30 in \
         total and none of them would be actionable. A class entry is deliberately \
         unlocated — it stands for a whole vocabulary, not for one element — so no \
         finding can name `w:rsids` itself. The loss IS reported; it is reported as \
         a class.",
    ),
    (
        "rsid",
        "FID-R-03 / `35`: a `w:rsids` entry, folded into the same `docx.rsid` class \
         as the table above.",
    ),
    (
        "rsidRoot",
        "FID-R-03 / `35`: the root revision-save ID, folded into the same \
         `docx.rsid` class as the table above.",
    ),
    (
        "settings",
        "FID-P-03: `w:settings` is a PART ROOT, not document content. The writer \
         omits `word/settings.xml` entirely when the model holds no non-default \
         setting, so the root goes with the part; every setting inside it is either \
         modeled or reported under its own name. A part container carries no \
         meaning that a reader could act on, which is the same reason `35` puts \
         `[Content_Types].xml` and `_rels/*` outside the taxonomy.",
    ),
];

/// A DOCX shaped the way Word writes one, built here rather than committed as a
/// fixture.
///
/// **No `.docx` in `fixtures/` carries a single `w:rsid*` or `w14:paraId`** — the
/// corpus is LibreOffice output and repository-generated packages — so the
/// highest-volume attribute in WordprocessingML, and the durable identities
/// `commentsExtended.xml` joins on, were exercised only by synthetic reporter unit
/// tests and never through a package. The guards below need a source that has them,
/// and building one in the test keeps the fixture manifest and its generator (owned
/// elsewhere) out of it.
fn word_shaped_package() -> Vec<u8> {
    let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/fontTable.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"#;
    let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable" Target="fontTable.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#;
    // `mc:Ignorable`, five rsid names across `w:p`/`w:r`/`w:tr`, and the durable
    // `w14:paraId`/`w14:textId` pair — the attribute shapes FID-R-03 named.
    let document = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" mc:Ignorable="w14">
        <w:body>
            <w:p w:rsidR="00A12B34" w:rsidRDefault="00A12B34" w:rsidRPr="00C56D78" w14:paraId="1A2B3C4D" w14:textId="77777777">
                <w:r w:rsidR="00A12B34" w:rsidRPr="00C56D78"><w:t>Word-shaped</w:t></w:r>
            </w:p>
            <w:tbl>
                <w:tblPr><w:tblW w:w="5000" w:type="pct"/></w:tblPr>
                <w:tblGrid><w:gridCol w:w="4680"/></w:tblGrid>
                <w:tr w:rsidR="00A12B34" w:rsidTr="00A12B34" w14:paraId="5E6F7A8B">
                    <w:tc><w:tcPr><w:tcW w:w="4680" w:type="dxa"/></w:tcPr><w:p w:rsidR="00A12B34"><w:r><w:t>cell</w:t></w:r></w:p></w:tc>
                </w:tr>
            </w:tbl>
            <w:sectPr w:rsidR="00A12B34" w:rsidSect="00A12B34"><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>
        </w:body>
    </w:document>"#;
    // The `w:rsids` table, plus a font table whose unmodeled child FID-R-04's new
    // report sites have to notice on a package path and not only in a unit test.
    let settings = br#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:rsids><w:rsidRoot w:val="00A12B34"/><w:rsid w:val="00A12B34"/><w:rsid w:val="00C56D78"/></w:rsids></w:settings>"#;
    let font_table = br#"<w:fonts xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:font w:name="Calibri"><w:panose1 w:val="020F0502"/><w:family w:val="handwritten"/><w:futureFontDetail w:val="1"/></w:font></w:fonts>"#;
    zip_package(&[
        ("[Content_Types].xml", content_types.as_slice()),
        ("_rels/.rels", root_rels.as_slice()),
        ("word/document.xml", document.as_slice()),
        ("word/_rels/document.xml.rels", document_rels.as_slice()),
        ("word/settings.xml", settings.as_slice()),
        ("word/fontTable.xml", font_table.as_slice()),
    ])
}

/// Zips the named parts verbatim into a package.
fn zip_package(parts: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in parts {
        writer.start_file(*name, options).expect("a zip entry");
        writer.write_all(bytes).expect("a written entry");
    }
    writer.finish().expect("a finished zip").into_inner()
}

/// Fixtures that never reach the comparison because the package reader or the
/// importer refuses them, with the refusal each one exists to exercise.
///
/// Enumerated for the same reason the exceptions are: a fixture that silently
/// stopped being refused would silently stop being covered.
const REFUSED_BY_DESIGN: &[(&str, &str)] = &[
    ("duplicate-part.docx", "two entries for one part name"),
    ("high-expansion.docx", "a zip bomb's expansion ratio"),
    ("malformed-truncated.docx", "a truncated archive"),
    ("path-traversal.docx", "a `../` part name"),
];

/// One fixture's source-versus-output comparison.
struct Comparison {
    /// Names the source carries that carry document meaning.
    source: MeaningfulMarkup,
    /// Names the written package carries, judged by the same policy.
    written: BTreeSet<String>,
    /// Element local names some report entry names (import and export together).
    reported: BTreeSet<String>,
    /// The import report, for the axis assertions further down this file.
    import_report: CompatibilityReport,
}

impl Comparison {
    /// Source names that are absent from the output and named by no finding.
    ///
    /// A name whose every occurrence sits under a *reported* ancestor is covered:
    /// `35` permits a whole-subtree loss to be "reported once on its outermost
    /// element" with its descendants skipped, which is how one `custGeom` finding
    /// accounts for the bézier curve that made the geometry unusable.
    fn unreported_losses(&self) -> Vec<&str> {
        self.source
            .elements()
            .iter()
            .filter(|name| !self.written.contains(*name))
            .filter(|name| !self.reported.contains(*name))
            .filter(|name| {
                !self
                    .source
                    .ancestors_of_every_occurrence(name)
                    .is_some_and(|ancestors| ancestors.iter().any(|a| self.reported.contains(a)))
            })
            .map(String::as_str)
            .collect()
    }
}

/// Imports a fixture semantically, writes it back, and collects both name sets.
fn compare(bytes: &[u8]) -> Comparison {
    let mut package =
        DocxPackage::open(bytes, PackageLimits::default()).expect("the fixture is admitted");
    let import = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the fixture imports");
    let export = export_document_with_retained_parts(
        &import.document,
        &binary_parts(bytes),
        &import.retained_parts,
    )
    .expect("the model is writable");

    let mut source = MeaningfulMarkup::default();
    for xml in xml_parts(bytes).values() {
        source.absorb(meaningful_markup(xml));
    }
    let mut written = MeaningfulMarkup::default();
    for xml in xml_parts(&export.bytes).values() {
        written.absorb(meaningful_markup(xml));
    }

    // A finding located on `element/@attribute` counts as naming `element`. That is
    // deliberate and it is the looser of the two readings: a modeled element whose
    // only content is a lost attribute is not emitted at all, so `w:family` with an
    // unrecognised `w:val` disappears from the package while the finding that
    // describes it is `family/@val`. Demanding an element-located finding there
    // would force an exception whose reason is "the report says it, in the other
    // field" — which says less than the finding does.
    let mut reported = BTreeSet::new();
    for element in import
        .report
        .entries
        .iter()
        .filter_map(|entry| entry.location.element.as_ref())
        .chain(
            export
                .report
                .entries
                .iter()
                .filter_map(|entry| entry.location.element.as_ref()),
        )
    {
        reported.insert(element.clone());
    }

    Comparison {
        source,
        written: written.elements().clone(),
        reported,
        import_report: import.report,
    }
}

/// The XML parts of a package, keyed by name. Both `.xml` and `.rels` are walked:
/// a relationship is document structure, not only plumbing.
fn xml_parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a zip package");
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let mut parts = BTreeMap::new();
    for name in names {
        if !(name.ends_with(".xml") || name.ends_with(".rels")) {
            continue;
        }
        let mut buffer = Vec::new();
        archive
            .by_name(&name)
            .expect("a listed entry")
            .read_to_end(&mut buffer)
            .expect("a readable entry");
        parts.insert(name, buffer);
    }
    parts
}

/// Media and embedded-font bytes, which the writer needs to keep the references it
/// was given (a reference whose bytes are missing is dropped, FID-R-06).
fn binary_parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a zip package");
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let mut parts = BTreeMap::new();
    for name in names {
        if !name.starts_with("word/media/") && !name.ends_with(".odttf") {
            continue;
        }
        let mut buffer = Vec::new();
        archive
            .by_name(&name)
            .expect("a listed entry")
            .read_to_end(&mut buffer)
            .expect("a readable entry");
        parts.insert(name, buffer);
    }
    parts
}

/// Every `.docx` under `fixtures/`, sorted, so a new fixture joins the gate by
/// existing rather than by being remembered.
fn fixtures() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let mut paths: Vec<PathBuf> = ["corpus", "generated"]
        .iter()
        .flat_map(|sub| {
            std::fs::read_dir(root.join(sub))
                .expect("a fixture directory")
                .map(|entry| entry.expect("a directory entry").path())
        })
        .filter(|path| path.extension().is_some_and(|ext| ext == "docx"))
        .collect();
    paths.sort();
    paths
}

/// Whether a fixture is one of the adversarial packages the readers refuse.
fn refused_by_design(name: &str) -> bool {
    REFUSED_BY_DESIGN
        .iter()
        .any(|(fixture, _)| *fixture == name)
}

/// Every package the gate covers: the admitted fixtures, plus the Word-shaped one
/// the corpus has no equivalent of.
fn gated_sources() -> Vec<(String, Vec<u8>)> {
    let mut sources: Vec<(String, Vec<u8>)> = fixtures()
        .into_iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy().into_owned();
            (!refused_by_design(&name))
                .then(|| (name, std::fs::read(&path).expect("a readable fixture")))
        })
        .collect();
    sources.push((
        "(word-shaped, built in-test)".to_owned(),
        word_shaped_package(),
    ));
    sources
}

#[test]
fn no_source_element_name_disappears_without_a_finding() {
    let mut failures: Vec<String> = Vec::new();
    for (name, bytes) in gated_sources() {
        let comparison = compare(&bytes);
        for lost in comparison.unreported_losses() {
            if EXCEPTIONS.iter().any(|(excused, _)| *excused == lost) {
                continue;
            }
            failures.push(format!(
                "  {name}: `{lost}` is in the source, absent from the written \
                 package, and named by no compatibility-report entry"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "a save dropped source markup in silence — add the report site, or add a \
         justified row to EXCEPTIONS saying why nothing was lost:\n{}",
        failures.join("\n")
    );
}

#[test]
fn no_coverage_exception_is_stale() {
    // The `--report-unused-disable-directives` half of the pattern. An allowlist
    // nobody prunes stops being a statement about the code, and the row that goes
    // stale first is the one whose loss has just been fixed — exactly the row whose
    // removal would tighten the gate.
    let mut unused: BTreeSet<&str> = EXCEPTIONS.iter().map(|(name, _)| *name).collect();
    let mut still_refused: BTreeSet<&str> =
        REFUSED_BY_DESIGN.iter().map(|(name, _)| *name).collect();
    for (_, bytes) in gated_sources() {
        for lost in compare(&bytes).unreported_losses() {
            unused.remove(lost);
        }
    }
    for path in fixtures() {
        let name = path.file_name().expect("a file name").to_string_lossy();
        if !refused_by_design(&name) {
            continue;
        }
        let bytes = std::fs::read(&path).expect("a readable fixture");
        let admitted =
            DocxPackage::open(&bytes, PackageLimits::default()).is_ok_and(|mut package| {
                import_package(
                    &mut package,
                    ImportConfig {
                        mode: ImportMode::Semantic,
                        ..ImportConfig::default()
                    },
                )
                .is_ok()
            });
        if !admitted {
            still_refused.remove(name.as_ref());
        }
    }
    assert!(
        unused.is_empty(),
        "these EXCEPTIONS rows no longer describe anything the fixtures do; delete \
         them so the gate keeps its teeth: {unused:?}"
    );
    assert!(
        still_refused.is_empty(),
        "these REFUSED_BY_DESIGN fixtures now import, so they belong in the gate \
         rather than beside it: {still_refused:?}"
    );
}

#[test]
fn the_report_describes_losses_that_are_attributes_rather_than_elements() {
    // FID-R-03's guarantee at corpus level, not at unit level: before the report
    // vocabulary had an attribute axis, a lost attribute of a MODELED element could
    // not be described at all — there was no field for it — so the only way to
    // mention one was to misname it as an element. A real document must therefore
    // produce at least one finding located on an attribute, and it must say the
    // element was `degraded` rather than omitted, because the element itself is
    // modeled.
    let report = compare(&word_shaped_package()).import_report;
    let attribute_findings: Vec<&casual_doc_import::CompatibilityEntry> = report
        .entries
        .iter()
        .filter(|entry| entry.location.attribute.is_some())
        .collect();
    assert!(
        !attribute_findings.is_empty(),
        "a real Word/LibreOffice document lost no attribute? the axis is unused: {:?}",
        report
            .entries
            .iter()
            .map(|e| &e.feature)
            .collect::<Vec<_>>()
    );
    for entry in attribute_findings {
        assert_eq!(
            entry.model_outcome(),
            casual_doc_import::ModelOutcome::Degraded,
            "{}: an attribute of a modeled element is a partial mapping, not an \
             omission",
            entry.feature
        );
        let element = entry.location.element.as_deref().unwrap_or_default();
        let attribute = entry.location.attribute.as_deref().unwrap_or_default();
        assert_eq!(
            entry.feature,
            format!("{element}/@{attribute}"),
            "the feature identifier must spell out the located attribute"
        );
    }
}

#[test]
fn revision_save_ids_are_one_counted_class_entry_not_one_entry_each() {
    // The aggregation rule `35` settles, asserted against a real document rather
    // than against a synthetic reporter: `w:rsid*` is the highest-volume attribute
    // in WordprocessingML, and one entry per occurrence — or even one per
    // element-and-attribute pair — would inflate every report by roughly half with
    // rows carrying nothing a reader can act on.
    let report = compare(&word_shaped_package()).import_report;
    let rsid: Vec<&casual_doc_import::CompatibilityEntry> = report
        .entries
        .iter()
        .filter(|entry| entry.feature == RSID_CLASS_FEATURE)
        .collect();
    assert_eq!(
        rsid.len(),
        1,
        "the whole revision-save-ID class is one entry: {:?}",
        rsid.iter().map(|e| &e.feature).collect::<Vec<_>>()
    );
    assert!(
        rsid[0].occurrences > 1,
        "the one entry carries the tally, so a reader can see the volume it stands \
         for: {}",
        rsid[0].occurrences
    );
    assert!(
        !report
            .entries
            .iter()
            .any(|entry| entry.feature.contains("/@rsid")
                || entry.feature == "rsid"
                || entry.feature == "rsids"),
        "no revision-save ID may be reported outside its class: {:?}",
        report
            .entries
            .iter()
            .map(|entry| &entry.feature)
            .collect::<Vec<_>>()
    );
}
