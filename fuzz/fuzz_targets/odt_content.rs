#![no_main]

use casual_doc_odf::{OdfImportLimits, OdfVersion, import_content_xml};
use libfuzzer_sys::fuzz_target;

const FUZZ_LIMITS: OdfImportLimits = OdfImportLimits {
    max_content_bytes: 1024 * 1024,
    max_styles_bytes: 1024 * 1024,
    max_xml_depth: 64,
    max_xml_elements: 20_000,
    max_xml_attributes: 60_000,
    max_xml_attribute_bytes: 512 * 1024,
    max_xml_name_bytes: 512,
    max_paragraphs: 10_000,
    max_inline_nodes: 40_000,
    max_lists: 10_000,
    max_list_depth: 32,
    max_tables: 1_000,
    max_table_rows: 10_000,
    max_table_cells: 40_000,
    max_table_depth: 8,
    max_notes: 10_000,
    max_text_bytes: 512 * 1024,
    max_space_repeat: 8_192,
    max_report_features: 512,
};

fuzz_target!(|data: &[u8]| {
    let (selector, xml) = data
        .split_first()
        .map_or((0, data), |(first, rest)| (*first, rest));
    let version = match selector % 3 {
        0 => OdfVersion::V1_2,
        1 => OdfVersion::V1_3,
        _ => OdfVersion::V1_4,
    };
    if let Ok(imported) = import_content_xml(xml, version, FUZZ_LIMITS) {
        imported.document.validate().unwrap();
        assert!(imported.report.entries.len() <= FUZZ_LIMITS.max_report_features + 1);
    }
});
