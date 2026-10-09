#![no_main]
//! The recovering open, on arbitrary bytes.
//!
//! # Why this target exists
//!
//! `docx_package` fuzzes package **admission** and stops there; nothing fuzzed
//! the semantic importer at all, so `casual-doc-import`'s 44 malformed-XML
//! refusals and 37 limit checks had never seen a hostile input. That was
//! tolerable only while every one of them refused: a refusal is a safe outcome
//! whatever the bytes said.
//!
//! The recovering open changes the bargain. It deliberately *continues* past
//! damage — rebuilding a ZIP directory from a header scan, closing tags a
//! truncated file left open, substituting a replacement character for bytes that
//! are not text, carrying on after a part it could not read — so every one of
//! those paths now runs on input a producer never wrote. **A panic is never an
//! acceptable outcome of opening a document**, and the only way to establish that
//! for a recovery path is to feed it bytes nobody chose.
//!
//! # What it asserts
//!
//! Beyond not panicking, two invariants that would otherwise be easy to break
//! silently:
//!
//! - **A repaired archive is still admitted under the same bounds.** The repair
//!   writes a new container; if that container could be made to claim more than
//!   the limits allow, the repair would be a way around them.
//! - **A recovery is never silent.** If the strict open refused and the
//!   recovering open produced a document, the recovery report must say something.
//!   A document that opens with content missing and reports nothing is worse than
//!   a refusal, because the reader saves over the original.

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_ooxml::{DocxPackage, PackageLimits, repair_archive};
use libfuzzer_sys::fuzz_target;

const FUZZ_LIMITS: PackageLimits = PackageLimits {
    max_input_bytes: 1024 * 1024,
    max_entries: 128,
    max_total_expanded_bytes: 8 * 1024 * 1024,
    max_single_expanded_bytes: 2 * 1024 * 1024,
    max_expansion_ratio: 100,
    max_path_bytes: 512,
};

/// Bounds low enough that a fuzz iteration stays fast, and still far above
/// anything the generated inputs reach.
const FUZZ_IMPORT: ImportConfig = ImportConfig {
    id_namespace: 1,
    mode: ImportMode::Semantic,
    max_elements: 200_000,
    max_depth: 64,
    max_text_bytes: 4 * 1024 * 1024,
    recover: true,
};

fuzz_target!(|data: &[u8]| {
    let strict_package = DocxPackage::open(data, FUZZ_LIMITS).is_ok();

    // Rung 1 of the host's ladder: rebuild the archive directory when admission
    // refused the bytes outright.
    let repaired = if strict_package {
        None
    } else {
        repair_archive(data)
    };
    let bytes = repaired.as_deref().unwrap_or(data);

    let Ok((mut package, package_repairs)) = DocxPackage::open_recovering(bytes, FUZZ_LIMITS)
    else {
        return;
    };

    // A repaired container is admitted under the same bounds as any other, so the
    // repair cannot be used to smuggle an over-limit package past them.
    let declared: u64 = package
        .entries()
        .iter()
        .try_fold(0_u64, |total, entry| {
            total.checked_add(entry.expanded_bytes)
        })
        .expect("admitted expanded-size sum must not overflow");
    assert!(declared <= FUZZ_LIMITS.max_total_expanded_bytes);
    assert!(package.entries().len() <= FUZZ_LIMITS.max_entries);

    let Ok(import) = import_package(&mut package, FUZZ_IMPORT) else {
        return;
    };

    // A recovery is never silent. The strict import is the reference: if it would
    // have refused these bytes and the recovering one produced a document, then
    // something was repaired and the report has to say so.
    let strict_import = DocxPackage::open(bytes, FUZZ_LIMITS).is_ok_and(|mut package| {
        import_package(
            &mut package,
            ImportConfig {
                recover: false,
                ..FUZZ_IMPORT
            },
        )
        .is_ok()
    });
    if !strict_import {
        assert!(
            !import.recovery.is_empty()
                || !package_repairs.is_empty()
                || repaired.is_some()
                || !import.report.is_empty(),
            "a document was recovered from bytes a strict open refused, and nothing said so"
        );
    }
});
