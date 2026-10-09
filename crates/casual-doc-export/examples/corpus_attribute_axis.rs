//! The **attribute axis** of the loss-coverage question, measured over a folder
//! of real `.docx` rather than over the fixture corpus.
//!
//! `corpus_attribute_axis <dir>`.
//!
//! `tests/source_element_coverage.rs` asks "did an element *name* in the source
//! vanish from the written package without a finding naming it?". When this
//! harness was written its module doc recorded the limit — *"Attribute names are
//! a separate axis"* — and that axis was not gated, so an attribute could vanish
//! on import unseen, which is exactly what happened to `w:wrap@wrapText`, imported
//! with a compatibility report of **zero** entries. The axis has been gated since
//! by `casual-doc-import/tests/attribute_loss_coverage.rs` (`109` HF-243), over
//! the committed corpus; this harness remains the way to measure it over a folder
//! the operator supplies.
//!
//! This harness runs the same import → write → diff, over
//! **(element, attribute) pairs** instead of element names. Pairs rather than
//! bare attribute names, because `35-DISPOSITION-TAXONOMY.md`'s one-off 2026-09-30
//! measurement of the bare-name axis (52 names, 19 after subsumption) found it
//! dominated by spelling equivalence: a bare `w:val` conflates every element that
//! carries one, so the pair is the unit at which a loss can actually be named.
//!
//! Three reductions are applied, each the same rule the element gate already
//! applies on its own axis:
//!
//! 1. **Subsumed by an element finding.** If some finding names the element, the
//!    attribute's loss is already reported — the element gate's own "a finding
//!    located on `element/@attribute` counts as naming `element`" rule, run the
//!    other way.
//! 2. **Subsumed by the element vanishing.** If the element itself is absent from
//!    the output, the element axis already gates it, so counting the attribute
//!    too would double-report one loss.
//! 3. **Named by an attribute finding.** A finding whose location is that exact
//!    `element/@attribute` pair is the loss being reported, which is the
//!    behaviour this axis exists to reward.
//!
//! What survives all three is an attribute that was in the source, is not in the
//! output, sits on an element that *did* survive, and which no finding mentions.
//! That is a silent attribute loss.
//!
//! It is a manual harness over files the operator supplies, so it is an example
//! rather than a test: no corpus is committed, and CI only ever compiles it.
#![allow(clippy::print_stdout, clippy::print_stderr)] // a manual harness

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};

use casual_doc_export::export_document_with_retained_parts;
use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use quick_xml::Reader;
use quick_xml::events::Event;

/// An `(element, attribute)` pair of XML local names.
type Pair = (String, String);

/// Per-pair roll-up across the corpus.
#[derive(Default)]
struct Roll {
    /// How many documents lost this pair silently.
    documents: usize,
}

/// Raises the package limits past the defaults, which exist to bound a hostile
/// upload rather than an operator's own files. A refusal is not a fidelity loss,
/// and reporting it as one would be the measurement lying in the easy direction.
fn generous_limits() -> PackageLimits {
    PackageLimits {
        max_input_bytes: 256 * 1024 * 1024,
        max_total_expanded_bytes: 1024 * 1024 * 1024,
        max_single_expanded_bytes: 256 * 1024 * 1024,
        ..PackageLimits::default()
    }
}

/// The XML parts of a package, keyed by name. Both `.xml` and `.rels` are walked:
/// a relationship is document structure, not plumbing.
fn xml_parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(bytes.to_vec())) else {
        return BTreeMap::new();
    };
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let mut parts = BTreeMap::new();
    for name in names {
        if !(name.ends_with(".xml") || name.ends_with(".rels")) {
            continue;
        }
        let Ok(mut entry) = archive.by_name(&name) else {
            continue;
        };
        let mut bytes = Vec::new();
        if entry.read_to_end(&mut bytes).is_ok() {
            parts.insert(name, bytes);
        }
    }
    parts
}

/// Binary (non-XML) parts, which the writer needs handed back to it.
fn binary_parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(bytes.to_vec())) else {
        return BTreeMap::new();
    };
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let mut parts = BTreeMap::new();
    for name in names {
        if name.ends_with(".xml") || name.ends_with(".rels") || name.ends_with('/') {
            continue;
        }
        let Ok(mut entry) = archive.by_name(&name) else {
            continue;
        };
        let mut bytes = Vec::new();
        if entry.read_to_end(&mut bytes).is_ok() {
            parts.insert(name, bytes);
        }
    }
    parts
}

/// The local name of a qualified XML name: everything after the last `:`.
fn local(qualified: &[u8]) -> String {
    let name = qualified
        .rsplit(|byte| *byte == b':')
        .next()
        .unwrap_or(qualified);
    String::from_utf8_lossy(name).into_owned()
}

/// Whether an attribute is XML plumbing rather than document meaning.
///
/// A namespace declaration (`xmlns`, `xmlns:w14`) binds a prefix; the writer
/// declares its own, and `report.rs` records that a prefix is a property of the
/// producer rather than of the construct, so a changed set of declarations is
/// not a loss. `mc:Ignorable` is placed outside the taxonomy by
/// `35-DISPOSITION-TAXONOMY.md` for the same reason.
fn is_plumbing(key: &[u8]) -> bool {
    key == b"xmlns" || key.starts_with(b"xmlns:") || local(key) == "Ignorable"
}

/// Whether an attribute value says "off", "none" or "nothing".
///
/// These are the values a writer legitimately expresses by *omitting* the
/// attribute, and counting them would measure the writer's spelling rather than
/// the document — which is the failure mode `35-DISPOSITION-TAXONOMY.md` recorded
/// when it declined to arm the bare-name attribute axis. `w:tblLook@w:lastRow="0"`
/// and an absent `w:lastRow` are the same table; `w:tab@w:leader="none"` and an
/// absent leader are the same tab stop; `wp:docPr@descr=""` is no alt text.
///
/// This is deliberately *not* applied to ECMA-376 toggle properties, where an
/// explicit off cancels an inherited on and is therefore meaningful — those are
/// element-level constructs (`w:b`, `w:keepNext`) carrying `w:val`, and the
/// element axis already gates them.
fn says_nothing(value: &str) -> bool {
    matches!(value.trim(), "" | "0" | "false" | "off" | "none")
}

/// Every `(element, attribute)` pair of local names in one XML part that carries
/// at least one **substantive** value.
///
/// `O(bytes)`. A malformed part ends the walk and yields what was collected,
/// matching `meaningful_markup`: the audit must not be the thing that fails on
/// input the importer itself accepted.
fn attribute_pairs(xml: &[u8]) -> BTreeSet<Pair> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut pairs = BTreeSet::new();
    while let Ok(event) = reader.read_event_into(&mut buffer) {
        match &event {
            Event::Eof => break,
            Event::Start(element) | Event::Empty(element) => {
                let name = local(element.name().as_ref());
                for attribute in element.attributes().flatten() {
                    let key = attribute.key.as_ref();
                    if is_plumbing(key) {
                        continue;
                    }
                    // The raw bytes, not the unescaped value: every token
                    // `says_nothing` tests for is pure ASCII with no entity
                    // spelling, so unescaping could only change values this
                    // predicate already keeps.
                    let value = String::from_utf8_lossy(attribute.value.as_ref());
                    if says_nothing(&value) {
                        continue;
                    }
                    pairs.insert((name.clone(), local(key)));
                }
            }
            _ => {}
        }
        buffer.clear();
    }
    pairs
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .expect("usage: corpus_attribute_axis <dir-of-docx>");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "docx"))
        .collect();
    files.sort();

    let mut roll: BTreeMap<Pair, Roll> = BTreeMap::new();
    let mut measured = 0_usize;
    let mut skipped = 0_usize;

    for path in &files {
        let name = path
            .file_name()
            .map_or_else(|| "?".to_owned(), |n| n.to_string_lossy().into_owned());
        let Ok(bytes) = std::fs::read(path) else {
            skipped += 1;
            println!("{name}\tREAD FAILED");
            continue;
        };
        let Ok(mut package) = DocxPackage::open(&bytes, generous_limits()) else {
            skipped += 1;
            println!("{name}\tOPEN FAILED");
            continue;
        };
        let import = match import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        ) {
            Ok(import) => import,
            Err(error) => {
                skipped += 1;
                println!("{name}\tIMPORT FAILED\t{error:?}");
                continue;
            }
        };
        let export = match export_document_with_retained_parts(
            &import.document,
            &binary_parts(&bytes),
            &import.retained_parts,
        ) {
            Ok(export) => export,
            Err(error) => {
                skipped += 1;
                println!("{name}\tWRITE FAILED\t{error:?}");
                continue;
            }
        };
        measured += 1;

        let mut source: BTreeSet<Pair> = BTreeSet::new();
        for xml in xml_parts(&bytes).values() {
            source.extend(attribute_pairs(xml));
        }
        let mut written: BTreeSet<Pair> = BTreeSet::new();
        let mut written_elements: BTreeSet<String> = BTreeSet::new();
        for xml in xml_parts(&export.bytes).values() {
            for pair in attribute_pairs(xml) {
                written_elements.insert(pair.0.clone());
                written.insert(pair);
            }
        }

        // Reductions 1 and 3: what the two reports already name, on either axis.
        let mut reported_elements: BTreeSet<String> = BTreeSet::new();
        let mut reported_pairs: BTreeSet<Pair> = BTreeSet::new();
        // The two reports are distinct types that happen to share a shape
        // (`casual_doc_export::CompatibilityEntry` re-declares the entry while
        // re-exporting the taxonomy enums from `casual-doc-import`), so the
        // locations are folded in by one closure applied to each rather than by
        // chaining the iterators.
        let mut absorb = |element: Option<String>, attribute: Option<String>| {
            if let Some(element) = element {
                if let Some(attribute) = attribute {
                    reported_pairs.insert((element.clone(), attribute));
                }
                reported_elements.insert(element);
            }
        };
        for entry in &import.report.entries {
            absorb(
                entry.location.element.clone(),
                entry.location.attribute.clone(),
            );
        }
        for entry in &export.report.entries {
            absorb(
                entry.location.element.clone(),
                entry.location.attribute.clone(),
            );
        }

        let mut silent: Vec<String> = Vec::new();
        for pair in &source {
            if written.contains(pair) {
                continue;
            }
            // 1. some finding names the element: the loss is reported.
            if reported_elements.contains(&pair.0) {
                continue;
            }
            // 3. a finding names this exact pair.
            if reported_pairs.contains(pair) {
                continue;
            }
            // 2. the element itself vanished: the element axis already gates it.
            if !written_elements.contains(&pair.0) {
                continue;
            }
            silent.push(format!("{}@{}", pair.0, pair.1));
            roll.entry(pair.clone()).or_default().documents += 1;
        }
        silent.sort();
        println!("{name}\t{} silent\t{}", silent.len(), silent.join(" "));
    }

    println!();
    println!(
        "== attribute axis: {} documents, {measured} measured, {skipped} skipped ==",
        files.len()
    );
    println!("docs\tpair");
    let mut ranked: Vec<(&Pair, &Roll)> = roll.iter().collect();
    ranked.sort_by(|left, right| {
        right
            .1
            .documents
            .cmp(&left.1.documents)
            .then(left.0.cmp(right.0))
    });
    for (pair, slot) in ranked {
        println!("{}\t{}@{}", slot.documents, pair.0, pair.1);
    }
}
