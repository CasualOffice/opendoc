#![no_main]

//! The MS-CFB recognition pass, against arbitrary bytes.
//!
//! What this hunts is not a wrong answer but a *non-answer*: a panic, an
//! arithmetic overflow, a read past the end, or a FAT chain that never
//! terminates. `classify` is reached from `FormatRegistry::detect`, which runs on
//! every `open` of every file, so an input that hangs here hangs the tab.

use casual_doc_cfb::{CfbLimits, classify, has_signature};
use libfuzzer_sys::fuzz_target;

/// Much tighter than the production defaults so a single case stays fast, and
/// tighter than the entry ceiling needs to be so the chain ceiling is reachable
/// by the fuzzer rather than always pre-empted.
const FUZZ_LIMITS: CfbLimits = CfbLimits {
    max_input_bytes: 1024 * 1024,
    max_directory_entries: 512,
    max_chain_sectors: 256,
    max_difat_sectors: 32,
};

fuzz_target!(|data: &[u8]| {
    // The sniff is the gate the product relies on: if it says no, `classify`
    // must agree, or a non-CFB input could still reach the walk.
    let sniffed = has_signature(data);
    let result = classify(data, FUZZ_LIMITS);
    if !sniffed {
        assert!(
            result.is_err(),
            "classify admitted an input the signature sniff rejected"
        );
    }
    // Deterministic: the same bytes must classify the same way twice. A parser
    // that reads uninitialised or position-dependent state would not.
    assert_eq!(result, classify(data, FUZZ_LIMITS));
});
