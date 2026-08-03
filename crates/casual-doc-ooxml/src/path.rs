//! DOCX-specific package path policy.

pub(crate) fn is_macro_part(part_name: &str) -> bool {
    let lower = part_name.to_ascii_lowercase();
    lower.ends_with("/vbaproject.bin")
        || lower.ends_with("/vbaprojectsignature.bin")
        || lower.ends_with("/vbadata.xml")
}
