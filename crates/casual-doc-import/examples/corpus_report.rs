//! Import-loss sweep over a folder of `.docx`: every construct the import
//! could not map, or mapped but could not retain.
//!
//! `corpus_report <dir>`. One line per document, then a roll-up of the findings
//! across the whole corpus ranked by **document frequency** — which is the list
//! worth working, because a construct missing in ten documents is one fix, not
//! ten, and a single exotic construct in one document is not the place to start.
//!
//! This is the measurement half of `docs/158-IMPORT-LOSS-MEASUREMENT.md`. It
//! deliberately stops at the compatibility report and does **not** paginate: the
//! question it answers is "what did the importer fail to understand", which the
//! report alone decides, and keeping the layout engine out keeps the sweep fast
//! enough to run over a large private corpus.
//!
//! It is a manual harness over files the operator supplies, so it is an example
//! rather than a test: no corpus is committed, and CI only ever compiles it.
#![allow(clippy::print_stdout, clippy::print_stderr)] // a manual harness

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_import::{
    Disposition, ImportConfig, ImportMode, ModelOutcome, RetentionOutcome, import_package,
};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

/// Per-construct roll-up across the corpus.
#[derive(Default)]
struct Roll {
    /// How many documents raised this construct at all.
    documents: usize,
    /// Total occurrences summed over every document.
    occurrences: u64,
    /// Every disposition seen for this construct, so a construct that is
    /// `degraded` in one document and `omitted` in another is visible as both
    /// rather than collapsing to whichever was read last.
    dispositions: BTreeSet<&'static str>,
}

/// The short taxonomy name of a disposition, for the roll-up table.
fn disposition_name(disposition: Disposition) -> &'static str {
    match disposition {
        Disposition::MappedComplete => "mapped-complete",
        Disposition::MappedPreserved => "mapped-preserved",
        Disposition::DegradedPreserved => "degraded-preserved",
        Disposition::DegradedNotRetained => "degraded-not-retained",
        Disposition::DegradedBlocked => "degraded-blocked",
        Disposition::OmittedPreserved => "omitted-preserved",
        Disposition::OmittedNotRetained => "omitted-not-retained",
        Disposition::OmittedBlocked => "omitted-blocked",
        Disposition::OmittedRejected => "omitted-rejected",
    }
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .expect("usage: corpus_report <dir-of-docx>");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "docx"))
        .collect();
    files.sort();

    let mut roll: BTreeMap<String, Roll> = BTreeMap::new();
    let mut imported_documents = 0_usize;
    let mut failed_documents = 0_usize;

    for path in &files {
        let name = path
            .file_name()
            .map_or_else(|| "?".to_owned(), |n| n.to_string_lossy().into_owned());
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                failed_documents += 1;
                println!("{name}\tREAD FAILED\t{error}");
                continue;
            }
        };
        // A private corpus holds real documents, including large ones; the
        // default limits exist to bound a hostile upload, not an operator's own
        // files, so the sweep raises them rather than reporting a refusal as a
        // fidelity loss.
        let limits = PackageLimits {
            max_input_bytes: 256 * 1024 * 1024,
            max_total_expanded_bytes: 1024 * 1024 * 1024,
            max_single_expanded_bytes: 256 * 1024 * 1024,
            ..PackageLimits::default()
        };
        let mut package = match DocxPackage::open(&bytes, limits) {
            Ok(package) => package,
            Err(error) => {
                failed_documents += 1;
                println!("{name}\tOPEN FAILED\t{error:?}");
                continue;
            }
        };
        let imported = match import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        ) {
            Ok(imported) => imported,
            Err(error) => {
                failed_documents += 1;
                println!("{name}\tIMPORT FAILED\t{error:?}");
                continue;
            }
        };
        imported_documents += 1;

        let mut lost: Vec<String> = Vec::new();
        for entry in &imported.report.entries {
            let unmapped = entry.disposition.model_outcome() != ModelOutcome::Mapped;
            let unretained = matches!(
                entry.disposition.retention_outcome(),
                RetentionOutcome::NotRetained
                    | RetentionOutcome::Blocked
                    | RetentionOutcome::Rejected
            );
            if !(unmapped || unretained) {
                continue;
            }
            lost.push(format!("{}x{}", entry.feature, entry.occurrences));
            let slot = roll.entry(entry.feature.clone()).or_default();
            slot.documents += 1;
            slot.occurrences += u64::from(entry.occurrences);
            slot.dispositions
                .insert(disposition_name(entry.disposition));
        }
        println!("{name}\t{} findings\t{}", lost.len(), lost.join(" "));
    }

    println!();
    println!(
        "== roll-up: {} documents scanned, {imported_documents} imported, {failed_documents} failed ==",
        files.len()
    );
    println!("docs\toccurrences\tdispositions\tfeature");
    let mut ranked: Vec<(&String, &Roll)> = roll.iter().collect();
    ranked.sort_by(|left, right| {
        right
            .1
            .documents
            .cmp(&left.1.documents)
            .then(right.1.occurrences.cmp(&left.1.occurrences))
            .then(left.0.cmp(right.0))
    });
    for (feature, slot) in ranked {
        println!(
            "{}\t{}\t{}\t{feature}",
            slot.documents,
            slot.occurrences,
            slot.dispositions
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .join(",")
        );
    }
}
