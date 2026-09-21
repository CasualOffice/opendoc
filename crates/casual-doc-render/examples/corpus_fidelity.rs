//! Fidelity sweep over a folder of `.docx`: page count, and every feature the
//! import could not map or could not retain.
//!
//! `corpus_fidelity <dir>`. One line per document, then a roll-up of the
//! findings by code across the whole corpus — which is the list worth working,
//! because a feature missing in ten documents is one fix, not ten.
#![allow(clippy::print_stderr, clippy::print_stdout)] // a manual harness
use std::collections::BTreeMap;

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_io::{ModelOutcome, RetentionOutcome};
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_ooxml::{DocxPackage, PackageLimits};

fn main() {
    let dir = std::env::args()
        .nth(1)
        .expect("usage: corpus_fidelity <dir-of-docx>");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "docx"))
        .collect();
    files.sort();

    let shaper = ParleyShaper::new();
    let mut roll: BTreeMap<String, (u32, usize)> = BTreeMap::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                println!("{name}\tREAD FAILED\t{e}");
                continue;
            }
        };
        let limits = PackageLimits {
            max_input_bytes: 256 * 1024 * 1024,
            max_total_expanded_bytes: 1024 * 1024 * 1024,
            max_single_expanded_bytes: 256 * 1024 * 1024,
            ..PackageLimits::default()
        };
        let mut package = match DocxPackage::open(&bytes, limits) {
            Ok(p) => p,
            Err(e) => {
                println!("{name}\tOPEN FAILED\t{e:?}");
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
            Ok(d) => d,
            Err(e) => {
                println!("{name}\tIMPORT FAILED\t{e:?}");
                continue;
            }
        };
        let pages = paginate_document(&imported.document, &shaper).pages.len();
        let mut lost: Vec<String> = Vec::new();
        for entry in &imported.report.entries {
            let unmapped = entry.model_outcome != ModelOutcome::Mapped;
            let unretained = matches!(entry.retention_outcome, RetentionOutcome::NotRetained);
            if unmapped || unretained {
                lost.push(format!("{}x{}", entry.feature, entry.occurrences));
                let slot = roll.entry(entry.feature.to_string()).or_default();
                slot.0 += entry.occurrences;
                slot.1 += 1;
            }
        }
        println!(
            "{name}\t{pages} pages\t{} unmapped/unretained\t{}",
            lost.len(),
            lost.join(" ")
        );
    }

    println!("\n--- rolled up across {} documents ---", files.len());
    let mut rows: Vec<_> = roll.into_iter().collect();
    rows.sort_by_key(|(_, (occurrences, docs))| (std::cmp::Reverse(*docs), std::cmp::Reverse(*occurrences)));
    for (feature, (occurrences, docs)) in rows {
        println!("{docs:3} docs  {occurrences:6} occurrences  {feature}");
    }
}
