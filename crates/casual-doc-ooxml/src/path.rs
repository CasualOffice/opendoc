//! Normalization and safety checks for package-relative part names.

use crate::error::PackageError;
use crate::limits::{enforce_limit, usize_to_u64};

pub(crate) fn normalize_package_path(
    raw: &[u8],
    max_path_bytes: usize,
) -> Result<String, PackageError> {
    enforce_limit(
        "package_path_bytes",
        usize_to_u64(raw.len()),
        usize_to_u64(max_path_bytes),
    )?;
    let path = std::str::from_utf8(raw).map_err(|_| PackageError::UnsafePartName)?;
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return Err(PackageError::UnsafePartName);
    }
    let is_directory = path.ends_with('/');
    let body = path.strip_suffix('/').unwrap_or(path);
    if body.is_empty() {
        return Err(PackageError::UnsafePartName);
    }

    let mut normalized = String::with_capacity(path.len());
    for (index, segment) in body.split('/').enumerate() {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(PackageError::UnsafePartName);
        }
        if index == 0 {
            let bytes = segment.as_bytes();
            if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
                return Err(PackageError::UnsafePartName);
            }
        }
        if index != 0 {
            normalized.push('/');
        }
        normalize_segment(segment, &mut normalized)?;
    }
    if is_directory {
        normalized.push('/');
    }
    Ok(normalized)
}

fn normalize_segment(segment: &str, output: &mut String) -> Result<(), PackageError> {
    let bytes = segment.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = *bytes.get(index + 1).ok_or(PackageError::UnsafePartName)?;
            let low = *bytes.get(index + 2).ok_or(PackageError::UnsafePartName)?;
            let value = hex_value(high)
                .and_then(|high| hex_value(low).map(|low| (high << 4) | low))
                .ok_or(PackageError::UnsafePartName)?;
            if value == 0
                || value == b'/'
                || value == b'\\'
                || value == b'.'
                || is_ascii_unreserved(value)
            {
                return Err(PackageError::UnsafePartName);
            }
            output.push('%');
            output.push(hex_digit(value >> 4));
            output.push(hex_digit(value & 0x0f));
            index += 3;
        } else {
            let character = segment[index..]
                .chars()
                .next()
                .ok_or(PackageError::UnsafePartName)?;
            output.push(character);
            index += character.len_utf8();
        }
    }
    Ok(())
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

const fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + value - 10) as char,
    }
}

const fn is_ascii_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

pub(crate) fn is_macro_part(part_name: &str) -> bool {
    let lower = part_name.to_ascii_lowercase();
    lower.ends_with("/vbaproject.bin")
        || lower.ends_with("/vbaprojectsignature.bin")
        || lower.ends_with("/vbadata.xml")
}
