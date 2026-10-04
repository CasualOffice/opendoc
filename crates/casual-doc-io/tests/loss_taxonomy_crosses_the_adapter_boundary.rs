// SPDX-License-Identifier: Apache-2.0

//! The two properties `docs/156` Tier 0 row 0.10 exists for, asserted at the
//! format-adapter boundary rather than inside one format.
//!
//! 1. **The DOCX boundary is not a projection.** The importer's report and ledger
//!    reach a host exactly as the importer built them. Paired with
//!    `casual-doc-import`'s blessed `loss_report_equivalence.golden`, which pins
//!    those reports byte for byte, this is the equivalence proof for the whole
//!    path: the golden fixes what the importer produces, and these assertions fix
//!    that the adapter layer hands it over unchanged.
//! 2. **A non-DOCX path can make a ledger-validated preservation claim.** That is
//!    the row's actual deliverable. ODT is the case: its adapter used to upgrade
//!    `not-retained` to `preserved` whenever the source happened to be retained
//!    and cite nothing at all, because the ledger it would have cited was
//!    DOCX-private. These assertions fail if the ledger does not cross the
//!    boundary, if the entry does not cite a record, or if the record does not
//!    account for the bytes it claims.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use casual_doc_io::{
    DetectionRequest, FormatSelection, PreservationKind, RetentionOutcome, builtin_registry,
};

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixtures_with_extension(extension: &str) -> Vec<(String, Vec<u8>)> {
    let root = fixtures_root();
    let mut names: BTreeSet<String> = BTreeSet::new();
    collect(&root, &root, extension, &mut names);
    names
        .into_iter()
        .map(|name| {
            let bytes = std::fs::read(root.join(&name))
                .unwrap_or_else(|error| panic!("read fixture {name}: {error}"));
            (name, bytes)
        })
        .collect()
}

fn collect(root: &Path, directory: &Path, extension: &str, into: &mut BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, extension, into);
        } else if path.extension().is_some_and(|found| found == extension) {
            let relative = path
                .strip_prefix(root)
                .expect("fixture path is under the fixtures root");
            into.insert(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// Every entry the DOCX importer raised reaches the adapter boundary unchanged —
/// same feature, same count, same bounded location, same disposition, same ledger
/// reference, same whole-part disposition — and so does the ledger itself.
///
/// The adapter is allowed to *append* findings of its own (an unreadable media or
/// font part is something only it can see), so the importer's entries must be a
/// subset of the artifact's rather than the whole of it. Nothing may be dropped
/// or rewritten, which is what a projection did.
#[test]
fn the_docx_adapter_hands_over_the_importer_report_and_ledger_unchanged() {
    let fixtures = fixtures_with_extension("docx");
    assert!(
        fixtures.len() >= 30,
        "expected the DOCX fixture corpus; got {}",
        fixtures.len()
    );
    let registry = builtin_registry();
    let mut compared = 0_usize;
    let mut entries_compared = 0_usize;
    for (name, bytes) in fixtures {
        for retain_source in [false, true] {
            // The importer, reached directly, is the reference.
            let Ok(mut package) = casual_doc_ooxml::DocxPackage::open(
                &bytes,
                casual_doc_ooxml::PackageLimits::default(),
            ) else {
                continue;
            };
            let config = casual_doc_import::ImportConfig {
                mode: if retain_source {
                    casual_doc_import::ImportMode::Retention
                } else {
                    casual_doc_import::ImportMode::Semantic
                },
                ..casual_doc_import::ImportConfig::default()
            };
            let Ok(reference) = casual_doc_import::import_package(&mut package, config) else {
                continue;
            };

            let artifact = registry
                .import(
                    DetectionRequest {
                        bytes: &bytes,
                        selection: FormatSelection::Auto,
                        file_name_hint: Some(&name),
                        mime_hint: None,
                    },
                    retain_source,
                )
                .unwrap_or_else(|error| panic!("{name} ({retain_source}): {error}"));

            assert_eq!(
                artifact.ledger, reference.ledger,
                "{name} (retain_source={retain_source}): the preservation ledger must cross \
                 the adapter boundary intact — a host that cannot resolve a `preserved` \
                 entry's record has to take the claim on trust, which is the limitation \
                 35-DISPOSITION-TAXONOMY.md recorded"
            );
            for entry in &reference.report.entries {
                assert!(
                    artifact.report.entries.contains(entry),
                    "{name} (retain_source={retain_source}): the importer raised {entry:?} \
                     and the adapter boundary did not carry it through unchanged. The \
                     boundary may APPEND findings; it may not drop, re-encode or weaken one."
                );
                entries_compared += 1;
            }
            // Every entry the adapter publishes resolves against the ledger it
            // publishes beside it. This is the check that used to be reachable
            // only from inside the DOCX importer.
            assert_eq!(
                artifact.report.validate(&artifact.ledger),
                Ok(()),
                "{name} (retain_source={retain_source})"
            );
            compared += 1;
        }
    }
    assert!(
        compared >= 60,
        "the comparison must actually have run on the corpus; it ran {compared} times"
    );
    // A floor, not an expectation. Its only job is to fail if the loop above goes
    // vacuous — an adapter that stopped publishing entries, or a corpus filter that
    // silently matched nothing, would otherwise pass this test by comparing nothing.
    // **78** is what the committed corpus actually produces, measured rather than
    // guessed: this was first written as `>= 100`, which no corpus satisfied, so the
    // guard failed for a reason that had nothing to do with the code under test.
    // Set below the measurement so adding a fixture cannot break it, and far enough
    // above zero that a vacuous run cannot pass.
    assert!(
        entries_compared >= 70,
        "the comparison must actually have seen findings; it saw {entries_compared}"
    );
}

/// ODT — a non-DOCX path — makes a preservation claim that cites a real record,
/// and the record accounts for the bytes it claims.
///
/// Fails if the ledger does not reach the boundary (no `ledger` to read), if a
/// `preserved` entry cites nothing (`ledger_id` is `None`), or if the record's
/// extent does not match the retained source. Before this row, all three were
/// the case: the claim was an unlicensed upgrade of `not-retained` applied
/// whenever `retain_source` was set.
#[test]
fn odt_preservation_claims_cite_a_validated_ledger_record() {
    let fixtures = fixtures_with_extension("odt");
    assert!(
        !fixtures.is_empty(),
        "expected ODT fixtures under fixtures/"
    );
    let registry = builtin_registry();
    let mut claims = 0_usize;
    let mut importable = 0_usize;
    for (name, bytes) in fixtures {
        // Built fresh per call: `DetectionRequest` is neither `Copy` nor reusable
        // after an import consumes it, and this fixture is imported twice — once
        // retaining the source bytes and once not, which is the whole comparison.
        let request = || DetectionRequest {
            bytes: bytes.as_slice(),
            selection: FormatSelection::Auto,
            file_name_hint: Some(name.as_str()),
            mime_hint: None,
        };
        let Ok(retained) = registry.import(request(), true) else {
            continue;
        };
        importable += 1;

        // With the source retained there is a byte floor, so there is exactly one
        // record and it accounts for the whole package.
        let snapshot = retained
            .ledger
            .source_snapshot()
            .expect("a retained ODT import records its byte floor");
        let record = retained.ledger.get(snapshot).expect("the record resolves");
        assert_eq!(record.kind, PreservationKind::SourceSnapshot, "{name}");
        assert_eq!(
            record.retained_bytes,
            bytes.len(),
            "{name}: the record must account for the bytes actually retained"
        );
        assert_eq!(retained.report.validate(&retained.ledger), Ok(()), "{name}");

        for entry in &retained.report.entries {
            if entry.retention_outcome() == RetentionOutcome::Preserved {
                assert_eq!(
                    entry.ledger_id,
                    Some(snapshot),
                    "{name}: {} claims preserved and must cite the record that licenses it",
                    entry.feature
                );
                claims += 1;
            }
        }

        // Without a byte floor nothing licenses preservation, so no entry may
        // claim it. This is the half that catches an unlicensed upgrade: the old
        // code produced `preserved` here too whenever it felt like it.
        let semantic = registry
            .import(request(), false)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(
            semantic.ledger.records().is_empty(),
            "{name}: a semantic ODT import retains nothing, so it must record nothing"
        );
        for entry in &semantic.report.entries {
            assert_ne!(
                entry.retention_outcome(),
                RetentionOutcome::Preserved,
                "{name}: {} claims preserved on a path that retains nothing",
                entry.feature
            );
        }
        assert_eq!(semantic.report.validate(&semantic.ledger), Ok(()), "{name}");
    }
    assert!(
        importable >= 1,
        "at least one ODT fixture must import, or this guard asserts nothing"
    );
    assert!(
        claims >= 1,
        "at least one ODT finding must make a preservation claim, or this guard \
         asserts nothing about licensing them"
    );
}
