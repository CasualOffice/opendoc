#![no_main]

use casual_doc_odf::{CONTENT_PART, OdfPackageLimits, OdtPackage};
use casual_doc_package::PackageLimits;
use libfuzzer_sys::fuzz_target;

const FUZZ_LIMITS: OdfPackageLimits = OdfPackageLimits {
    package: PackageLimits {
        max_input_bytes: 1024 * 1024,
        max_entries: 128,
        max_total_expanded_bytes: 8 * 1024 * 1024,
        max_single_expanded_bytes: 2 * 1024 * 1024,
        max_expansion_ratio: 100,
        max_path_bytes: 512,
    },
    max_manifest_bytes: 512 * 1024,
    max_xml_depth: 64,
    max_xml_elements: 10_000,
    max_xml_attributes: 40_000,
    max_xml_attribute_bytes: 512 * 1024,
};

fuzz_target!(|data: &[u8]| {
    let Ok(mut package) = OdtPackage::open(data, FUZZ_LIMITS) else {
        return;
    };
    assert_eq!(package.entries().first().map(|entry| entry.part_name.as_str()), Some("META-INF/manifest.xml"));
    assert!(package
        .manifest_entries()
        .iter()
        .any(|entry| entry.full_path == CONTENT_PART));
    let _ = package.read_part(CONTENT_PART);
});
