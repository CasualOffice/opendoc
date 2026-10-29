#![no_main]

use casual_doc_pdf_import::{PdfReadLimits, import_pdf};
use libfuzzer_sys::fuzz_target;

/// Small bounds so the fuzzer explores structure rather than size
/// (`docs/21` PDF import table, scaled down).
fn fuzz_limits() -> PdfReadLimits {
    let mut limits = PdfReadLimits::default();
    limits.max_input_bytes = 1024 * 1024;
    limits.max_objects = 10_000;
    limits.max_pages = 16;
    limits.max_stream_decoded_bytes = 4 * 1024 * 1024;
    limits.max_document_decoded_bytes = 16 * 1024 * 1024;
    limits.max_operators_per_page = 100_000;
    limits.max_evidence_per_page = 20_000;
    limits.max_image_pixels = 4_000_000;
    limits.max_fonts = 256;
    limits.max_font_program_bytes = 1024 * 1024;
    limits
}

fuzz_target!(|data: &[u8]| {
    // Most random inputs carry no header, so prefix one for half the corpus
    // to keep the fuzzer inside the parser rather than at the front door.
    let mut prefixed = b"%PDF-1.7\n".to_vec();
    prefixed.extend_from_slice(data);
    for candidate in [data, prefixed.as_slice()] {
        if let Ok(imported) = import_pdf(candidate.to_vec(), fuzz_limits()) {
            // The importer promises a validated document; re-checking turns
            // "it did not panic" into "it produced a document the runtime can
            // hold".
            imported.document.validate().unwrap();
            assert!(imported.report.findings.len() <= 64);
        }
    }
});
