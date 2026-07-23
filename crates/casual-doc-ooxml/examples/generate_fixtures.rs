use std::fs;
use std::io::{Cursor, Write};
use std::path::PathBuf;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const CONTENT_TYPES: &[u8] = br#"<?xml version="1.0"?><Types/>"#;
const ROOT_RELATIONSHIPS: &[u8] = br#"<?xml version="1.0"?><Relationships/>"#;
const DOCUMENT: &[u8] = br#"<?xml version="1.0"?><w:document/>"#;
const MIXED_UNICODE_DOCUMENT: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
    "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    "<w:body><w:p><w:r><w:t xml:space=\"preserve\">",
    "Cafe\u{0301} | \u{0939}\u{093f}\u{0928}\u{094d}\u{0926}\u{0940} | ",
    "\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064a}\u{0629} | ",
    "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}\u{200d}\u{1f466}",
    "</w:t></w:r></w:p></w:body></w:document>",
);
const UNKNOWN_SAFE_PART: &[u8] =
    br#"<custom xmlns="urn:opendoc:fixture"><value>preserve-me</value></custom>"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/generated");
    fs::create_dir_all(&output)?;

    fs::write(output.join("minimal-valid.docx"), minimal_package())?;

    fs::write(
        output.join("mixed-unicode.docx"),
        package(&entries_with_document(MIXED_UNICODE_DOCUMENT.as_bytes()))?,
    )?;

    let mut unknown_safe = minimal_entries();
    unknown_safe.push((
        "customXml/item1.xml".to_owned(),
        UNKNOWN_SAFE_PART.to_vec(),
        CompressionMethod::Deflated,
    ));
    fs::write(
        output.join("unknown-safe-part.docx"),
        package(&unknown_safe)?,
    )?;

    let mut traversal = minimal_entries();
    traversal.push((
        "../outside.xml".to_owned(),
        b"unsafe".to_vec(),
        CompressionMethod::Stored,
    ));
    fs::write(output.join("path-traversal.docx"), package(&traversal)?)?;

    let mut expansion = required_stored_entries();
    expansion[2] = (
        "word/document.xml".to_owned(),
        vec![b'A'; 64 * 1024],
        CompressionMethod::Deflated,
    );
    fs::write(output.join("high-expansion.docx"), package(&expansion)?)?;

    let mut duplicate_entries = minimal_entries();
    duplicate_entries.push((
        "word/documenx.xml".to_owned(),
        b"duplicate".to_vec(),
        CompressionMethod::Stored,
    ));
    let mut duplicate = package(&duplicate_entries)?;
    patch_fourth_name_as_document(&mut duplicate)?;
    fs::write(output.join("duplicate-part.docx"), duplicate)?;

    fs::write(
        output.join("malformed-truncated.docx"),
        b"PK\x03\x04truncated",
    )?;
    Ok(())
}

fn minimal_package() -> Vec<u8> {
    package(&minimal_entries()).expect("fixture ZIP generation should succeed")
}

fn minimal_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    entries_with_document(DOCUMENT)
}

fn entries_with_document(document: &[u8]) -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "word/document.xml".to_owned(),
            document.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "[Content_Types].xml".to_owned(),
            CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Deflated,
        ),
    ]
}

fn required_stored_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "[Content_Types].xml".to_owned(),
            CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "word/document.xml".to_owned(),
            DOCUMENT.to_vec(),
            CompressionMethod::Stored,
        ),
    ]
}

fn package(
    entries: &[(String, Vec<u8>, CompressionMethod)],
) -> Result<Vec<u8>, zip::result::ZipError> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes, compression) in entries {
        writer.start_file(
            name,
            SimpleFileOptions::default().compression_method(*compression),
        )?;
        writer.write_all(bytes)?;
    }
    Ok(writer.finish()?.into_inner())
}

fn patch_fourth_name_as_document(bytes: &mut [u8]) -> Result<(), &'static str> {
    const DOCUMENT_PART: &[u8] = b"word/document.xml";
    let central = central_record_positions(bytes)?;
    let duplicate_central = *central.get(3).ok_or("missing fourth central record")?;
    let duplicate_local =
        usize::try_from(read_u32(bytes, duplicate_central + 42)?).map_err(|_| "large offset")?;
    let central_name = duplicate_central + 46;
    let local_name = duplicate_local + 30;
    bytes
        .get_mut(central_name..central_name + DOCUMENT_PART.len())
        .ok_or("central name outside fixture")?
        .copy_from_slice(DOCUMENT_PART);
    bytes
        .get_mut(local_name..local_name + DOCUMENT_PART.len())
        .ok_or("local name outside fixture")?
        .copy_from_slice(DOCUMENT_PART);
    Ok(())
}

fn central_record_positions(bytes: &[u8]) -> Result<Vec<usize>, &'static str> {
    let eocd = bytes
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .ok_or("missing EOCD")?;
    let mut cursor = usize::try_from(read_u32(bytes, eocd + 16)?).map_err(|_| "large directory")?;
    let entries = usize::from(read_u16(bytes, eocd + 10)?);
    let mut positions = Vec::new();
    for _ in 0..entries {
        positions.push(cursor);
        let name = usize::from(read_u16(bytes, cursor + 28)?);
        let extra = usize::from(read_u16(bytes, cursor + 30)?);
        let comment = usize::from(read_u16(bytes, cursor + 32)?);
        cursor += 46 + name + extra + comment;
    }
    Ok(positions)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, &'static str> {
    let value = bytes.get(offset..offset + 2).ok_or("short fixture")?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, &'static str> {
    let value = bytes.get(offset..offset + 4).ok_or("short fixture")?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}
