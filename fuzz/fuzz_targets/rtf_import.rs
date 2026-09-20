#![no_main]

use casual_doc_rtf::{RtfLimits, import_rtf};
use libfuzzer_sys::fuzz_target;

/// Small bounds so the fuzzer explores structure rather than size.
const FUZZ_LIMITS: RtfLimits = RtfLimits {
    max_input_bytes: 1024 * 1024,
    max_group_depth: 64,
    max_paragraphs: 10_000,
    max_inline_nodes: 40_000,
    max_text_scalar_values: 512 * 1024,
    max_table_rows: 10_000,
    max_table_cells: 40_000,
    max_fonts: 512,
    max_colors: 512,
    max_list_definitions: 512,
    max_pictures: 512,
    max_picture_bytes: 1024 * 1024,
    max_total_picture_bytes: 4 * 1024 * 1024,
    max_findings: 512,
};

fuzz_target!(|data: &[u8]| {
    // Most random inputs carry no signature, so prefix one for half the corpus
    // to keep the fuzzer inside the parser rather than at the front door.
    let prefixed = {
        let mut bytes = Vec::with_capacity(data.len() + 12);
        bytes.extend_from_slice(br"{\rtf1\ansi ");
        bytes.extend_from_slice(data);
        bytes
    };
    for candidate in [data, prefixed.as_slice()] {
        if let Ok(imported) = import_rtf(candidate, FUZZ_LIMITS) {
            // The importer promises an already-validated document; re-checking
            // here is what turns "it did not panic" into "it produced a
            // document the rest of the runtime can hold".
            imported.document.validate().unwrap();
            assert!(imported.report.entries.len() <= FUZZ_LIMITS.max_findings);
        }
    }
});
