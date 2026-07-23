#![no_main]

use casual_doc_ooxml::{DocxPackage, PackageLimits};
use libfuzzer_sys::fuzz_target;

const FUZZ_LIMITS: PackageLimits = PackageLimits {
    max_input_bytes: 1024 * 1024,
    max_entries: 128,
    max_total_expanded_bytes: 8 * 1024 * 1024,
    max_single_expanded_bytes: 2 * 1024 * 1024,
    max_expansion_ratio: 100,
    max_path_bytes: 512,
};

fuzz_target!(|data: &[u8]| {
    let Ok(mut package) = DocxPackage::open(data, FUZZ_LIMITS) else {
        return;
    };

    let entries = package.entries().to_vec();
    let declared_total = entries
        .iter()
        .try_fold(0_u64, |total, entry| {
            total.checked_add(entry.expanded_bytes)
        })
        .expect("admitted expanded-size sum must not overflow");
    assert_eq!(declared_total, package.total_expanded_bytes());
    assert!(declared_total <= FUZZ_LIMITS.max_total_expanded_bytes);

    for entry in entries {
        if let Ok(bytes) = package.read_part(&entry.part_name) {
            assert_eq!(
                u64::try_from(bytes.len()).expect("part length must fit u64"),
                entry.expanded_bytes
            );
        }
    }
});
