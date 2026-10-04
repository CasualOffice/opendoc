// SPDX-License-Identifier: Apache-2.0

//! A blessed snapshot of **every** DOCX loss report and preservation ledger the
//! fixture corpus produces, in both import modes.
//!
//! This guard exists for one job: the `35-DISPOSITION-TAXONOMY.md` vocabulary is
//! the thing that proves this engine does not lose document data in silence, and
//! moving it between crates is therefore the one refactor whose silent failure
//! would be worse than not doing it at all. The round-trip tests cannot see such
//! a failure — they assert a fixed point whose left side is the importer's own
//! output, so a mis-mapped disposition is a perfect fixed point. The export-side
//! coverage gate cannot see it either: it checks that a *name* appears somewhere
//! in the report, not which of the nine dispositions it was charged.
//!
//! So the snapshot records the whole report, down to the occurrence counts, the
//! bounded locations, the per-construct disposition, the ledger reference and the
//! ledger's own records with their retained byte counts. A disposition that
//! changes from `omitted` + `not-retained` to `omitted` + `preserved` moves one
//! character in this file and fails.
//!
//! The golden covers `fixtures/**/*.docx` in sorted order, in `Semantic` and then
//! `Retention` mode. Packages the admission layer refuses (the deliberately
//! malformed and path-traversal fixtures) are recorded *as* refusals, so a
//! refusal silently becoming an acceptance is also a diff.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use casual_doc_import::{
    CompatibilityReport, ImportConfig, ImportMode, PreservationLedger, RSID_CLASS_FEATURE,
    WATERMARK_CLASS_FEATURE,
};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/loss_report_equivalence.golden")
}

/// Every `.docx` under `fixtures/`, by path relative to the fixtures root, in
/// sorted order so the snapshot does not depend on directory iteration order.
fn docx_fixtures() -> Vec<(String, Vec<u8>)> {
    let root = fixtures_root();
    let mut relative: BTreeSet<String> = BTreeSet::new();
    collect(&root, &root, &mut relative);
    assert!(
        relative.len() >= 30,
        "expected the DOCX fixture corpus to be found under {}; got {} files",
        root.display(),
        relative.len()
    );
    relative
        .into_iter()
        .map(|name| {
            let bytes = std::fs::read(root.join(&name))
                .unwrap_or_else(|error| panic!("read fixture {name}: {error}"));
            (name, bytes)
        })
        .collect()
}

fn collect(root: &Path, directory: &Path, into: &mut BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, into);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "docx")
        {
            let relative = path
                .strip_prefix(root)
                .expect("fixture path is under the fixtures root");
            into.insert(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// Renders one report deterministically, one line per entry, with every field
/// the taxonomy defines. `Debug` is used for the enums deliberately: a renamed
/// or re-ordered variant then shows up as a diff rather than being absorbed by a
/// hand-written mapping in the test, which is exactly the drift this guard is
/// here to catch.
fn render_report(report: &CompatibilityReport, into: &mut String) {
    writeln!(into, "  report: {} entries", report.entries.len()).expect("write");
    for entry in &report.entries {
        let location = &entry.location;
        writeln!(
            into,
            "    {feature} x{occurrences} {disposition:?} model={model:?} \
             retention={retention:?} part={part:?} content_type={content_type:?} \
             loc.part={loc_part:?} loc.element={element:?} loc.attribute={attribute:?} \
             ledger={ledger:?}",
            feature = entry.feature,
            occurrences = entry.occurrences,
            disposition = entry.disposition,
            model = entry.model_outcome(),
            retention = entry.retention_outcome(),
            part = entry.part.as_ref().map(|part| part.part_name.as_str()),
            content_type = entry
                .part
                .as_ref()
                .and_then(|part| part.content_type.as_deref()),
            loc_part = location.part_name.as_deref(),
            element = location.element.as_deref(),
            attribute = location.attribute.as_deref(),
            ledger = entry.ledger_id.map(|id| id.get()),
        )
        .expect("write");
    }
}

fn render_ledger(ledger: &PreservationLedger, into: &mut String) {
    writeln!(
        into,
        "  ledger: {} records, snapshot={:?}",
        ledger.records().len(),
        ledger.source_snapshot().map(|id| id.get())
    )
    .expect("write");
    for record in ledger.records() {
        writeln!(
            into,
            "    #{id} {kind:?} covers={covers:?} bytes={bytes} retains={retains}",
            id = record.id.get(),
            kind = record.kind,
            covers = record.covers.as_deref(),
            bytes = record.retained_bytes,
            retains = record.retains_something(),
        )
        .expect("write");
    }
}

fn snapshot() -> String {
    let mut out = String::new();
    // The two class identifiers are part of the report's stable vocabulary, so a
    // rename is a wire-visible change and belongs in the snapshot rather than in
    // a separate assertion that a refactor could forget to run.
    writeln!(out, "rsid-class-feature: {RSID_CLASS_FEATURE}").expect("write");
    writeln!(out, "watermark-class-feature: {WATERMARK_CLASS_FEATURE}").expect("write");
    for (name, bytes) in docx_fixtures() {
        for mode in [ImportMode::Semantic, ImportMode::Retention] {
            writeln!(out, "\n== {name} [{mode:?}]").expect("write");
            let limits = PackageLimits::default();
            let mut package = match DocxPackage::open(&bytes, limits) {
                Ok(package) => package,
                Err(error) => {
                    writeln!(out, "  package refused: {error}").expect("write");
                    continue;
                }
            };
            let config = ImportConfig {
                mode,
                ..ImportConfig::default()
            };
            match casual_doc_import::import_package(&mut package, config) {
                Ok(import) => {
                    render_report(&import.report, &mut out);
                    render_ledger(&import.ledger, &mut out);
                    // The claim every `preserved` entry makes must still resolve
                    // against the ledger. `import_package` already checks this and
                    // fails the import, so this re-asserts it at the snapshot
                    // boundary: if the check were ever removed, the snapshot
                    // would go on matching but this line would not.
                    writeln!(
                        out,
                        "  validate: {:?}",
                        import.report.validate(&import.ledger)
                    )
                    .expect("write");
                }
                Err(error) => {
                    writeln!(out, "  import refused: {error:?}").expect("write");
                }
            }
        }
    }
    out
}

/// The whole corpus's loss reports and ledgers, byte for byte.
///
/// Re-bless with `REBLESS_LOSS_REPORTS=1` **only** when a disposition change is
/// the intended subject of the commit, and explain the diff there. A refactor
/// that moves the taxonomy between crates must leave this file untouched; that
/// is the whole evidence it is behaviour-preserving.
#[test]
fn docx_loss_reports_and_ledgers_match_the_golden_snapshot() {
    let actual = snapshot();
    let path = golden_path();
    if std::env::var_os("REBLESS_LOSS_REPORTS").is_some() {
        std::fs::write(&path, &actual).expect("write loss-report golden");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing loss-report golden {}; create it with REBLESS_LOSS_REPORTS=1",
            path.display()
        )
    });
    assert_eq!(
        actual, expected,
        "a DOCX loss report or preservation ledger changed (35-DISPOSITION-TAXONOMY.md). \
         If that is intended, re-bless with REBLESS_LOSS_REPORTS=1 and justify the diff."
    );
}
