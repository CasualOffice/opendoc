use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::archive::{find_eocd, read_u16, read_u32};
use crate::limits::usize_to_u64;
use crate::package::{CONTENT_TYPES_PART, ROOT_RELATIONSHIPS_PART};
use crate::relationships::{OFFICE_DOCUMENT_REL_STRICT, OFFICE_DOCUMENT_REL_TRANSITIONAL};
use crate::{CancellationToken, DocxPackage, PackageError, PackageLimits, TargetMode};

const DOCUMENT_PART: &str = "word/document.xml";
const CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
const ROOT_RELATIONSHIPS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
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

fn package(entries: &[(&str, &[u8], CompressionMethod)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes, compression) in entries {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(*compression),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn minimal_entries() -> [(&'static str, &'static [u8], CompressionMethod); 3] {
    [
        (DOCUMENT_PART, DOCUMENT, CompressionMethod::Deflated),
        (CONTENT_TYPES_PART, CONTENT_TYPES, CompressionMethod::Stored),
        (
            ROOT_RELATIONSHIPS_PART,
            ROOT_RELATIONSHIPS,
            CompressionMethod::Stored,
        ),
    ]
}

fn minimal_package() -> Vec<u8> {
    package(&minimal_entries())
}

fn central_record_positions(bytes: &[u8]) -> Vec<usize> {
    let eocd = find_eocd(bytes).unwrap();
    let mut cursor = usize::try_from(read_u32(bytes, eocd + 16).unwrap()).unwrap();
    let entries = usize::from(read_u16(bytes, eocd + 10).unwrap());
    let mut positions = Vec::new();
    for _ in 0..entries {
        positions.push(cursor);
        let name = usize::from(read_u16(bytes, cursor + 28).unwrap());
        let extra = usize::from(read_u16(bytes, cursor + 30).unwrap());
        let comment = usize::from(read_u16(bytes, cursor + 32).unwrap());
        cursor += 46 + name + extra + comment;
    }
    positions
}

#[test]
fn minimal_package_metadata_and_part_reads_are_deterministic() {
    let bytes = minimal_package();
    let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();

    assert_eq!(
        package
            .entries()
            .iter()
            .map(|entry| entry.part_name.as_str())
            .collect::<Vec<_>>(),
        vec![CONTENT_TYPES_PART, ROOT_RELATIONSHIPS_PART, DOCUMENT_PART]
    );
    assert_eq!(package.read_part(DOCUMENT_PART).unwrap(), DOCUMENT);
    assert_eq!(
        package.read_part("word/missing.xml"),
        Err(PackageError::PartNotFound)
    );
    assert_eq!(
        package.total_expanded_bytes(),
        usize_to_u64(CONTENT_TYPES.len() + ROOT_RELATIONSHIPS.len() + DOCUMENT.len())
    );
}

#[test]
fn every_package_limit_accepts_its_boundary_and_rejects_above_it() {
    let bytes = minimal_package();
    let opened = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
    let total = opened.total_expanded_bytes();
    let single = opened
        .entries()
        .iter()
        .map(|entry| entry.expanded_bytes)
        .max()
        .unwrap();
    let path = minimal_entries()
        .iter()
        .map(|(name, _, _)| name.len())
        .max()
        .unwrap();

    let boundary = PackageLimits {
        max_input_bytes: bytes.len(),
        max_entries: 3,
        max_total_expanded_bytes: total,
        max_single_expanded_bytes: single,
        max_expansion_ratio: 1,
        max_path_bytes: path,
    };
    DocxPackage::open(&bytes, boundary).unwrap();

    let cases = [
        PackageLimits {
            max_input_bytes: bytes.len() - 1,
            ..boundary
        },
        PackageLimits {
            max_entries: 2,
            ..boundary
        },
        PackageLimits {
            max_total_expanded_bytes: total - 1,
            ..boundary
        },
        PackageLimits {
            max_single_expanded_bytes: single - 1,
            ..boundary
        },
        PackageLimits {
            max_expansion_ratio: 0,
            ..boundary
        },
        PackageLimits {
            max_path_bytes: path - 1,
            ..boundary
        },
    ];
    for limits in cases {
        assert!(matches!(
            DocxPackage::open(&bytes, limits),
            Err(PackageError::LimitExceeded { .. })
        ));
    }

    let invalid_configurations = [
        (
            "input_package_bytes",
            PackageLimits {
                max_input_bytes: usize::MAX,
                ..PackageLimits::default()
            },
        ),
        (
            "zip_entries",
            PackageLimits {
                max_entries: usize::MAX,
                ..PackageLimits::default()
            },
        ),
        (
            "total_expanded_bytes",
            PackageLimits {
                max_total_expanded_bytes: u64::MAX,
                ..PackageLimits::default()
            },
        ),
        (
            "single_expanded_entry_bytes",
            PackageLimits {
                max_single_expanded_bytes: u64::MAX,
                ..PackageLimits::default()
            },
        ),
        (
            "entry_expansion_ratio",
            PackageLimits {
                max_expansion_ratio: u64::MAX,
                ..PackageLimits::default()
            },
        ),
        (
            "package_path_bytes",
            PackageLimits {
                max_path_bytes: usize::MAX,
                ..PackageLimits::default()
            },
        ),
    ];
    for (expected_limit, invalid) in invalid_configurations {
        assert!(matches!(
            DocxPackage::open(&bytes, invalid),
            Err(PackageError::InvalidLimitConfiguration { limit, .. })
                if limit == expected_limit
        ));
    }
}

#[test]
fn traversal_ambiguous_escapes_and_duplicates_are_rejected() {
    for unsafe_name in [
        "../evil.xml",
        "/absolute.xml",
        "C:/drive.xml",
        "word\\evil.xml",
        "word/%2e%2e/evil.xml",
        "word/%64ocument.xml",
    ] {
        let mut entries = minimal_entries().to_vec();
        entries.push((unsafe_name, b"unsafe", CompressionMethod::Stored));
        assert_eq!(
            DocxPackage::open(&package(&entries), PackageLimits::default()).unwrap_err(),
            PackageError::UnsafePartName
        );
    }

    let mut entries = minimal_entries().to_vec();
    entries.push(("word/documenx.xml", b"duplicate", CompressionMethod::Stored));
    let mut duplicate = package(&entries);
    let central = central_record_positions(&duplicate);
    let duplicate_central = central[3];
    let duplicate_local =
        usize::try_from(read_u32(&duplicate, duplicate_central + 42).unwrap()).unwrap();
    duplicate[duplicate_central + 46..duplicate_central + 46 + DOCUMENT_PART.len()]
        .copy_from_slice(DOCUMENT_PART.as_bytes());
    duplicate[duplicate_local + 30..duplicate_local + 30 + DOCUMENT_PART.len()]
        .copy_from_slice(DOCUMENT_PART.as_bytes());
    assert_eq!(
        DocxPackage::open(&duplicate, PackageLimits::default()).unwrap_err(),
        PackageError::DuplicatePart
    );
}

#[test]
fn high_expansion_malformed_missing_and_macro_packages_are_rejected() {
    let expanded = vec![b'A'; 64 * 1024];
    let entries = [
        (
            DOCUMENT_PART,
            expanded.as_slice(),
            CompressionMethod::Deflated,
        ),
        (CONTENT_TYPES_PART, CONTENT_TYPES, CompressionMethod::Stored),
        (
            ROOT_RELATIONSHIPS_PART,
            ROOT_RELATIONSHIPS,
            CompressionMethod::Stored,
        ),
    ];
    let limits = PackageLimits {
        max_expansion_ratio: 2,
        ..PackageLimits::default()
    };
    assert!(matches!(
        DocxPackage::open(&package(&entries), limits),
        Err(PackageError::LimitExceeded {
            limit: "entry_expansion_ratio",
            ..
        })
    ));
    assert_eq!(
        DocxPackage::open(b"not a zip", PackageLimits::default()).unwrap_err(),
        PackageError::MalformedArchive
    );
    assert!(matches!(
        DocxPackage::open(&package(&minimal_entries()[..2]), PackageLimits::default()),
        Err(PackageError::MissingRequiredPart { .. })
    ));

    let mut macro_entries = minimal_entries().to_vec();
    macro_entries.push(("word/vbaProject.bin", b"macro", CompressionMethod::Stored));
    assert_eq!(
        DocxPackage::open(&package(&macro_entries), PackageLimits::default()).unwrap_err(),
        PackageError::MacroPart
    );
}

#[test]
fn encrypted_unsupported_symlink_and_overlapping_entries_are_rejected() {
    let original = minimal_package();
    let central = central_record_positions(&original);

    let mut encrypted = original.clone();
    encrypted[6] |= 1;
    encrypted[central[0] + 8] |= 1;
    assert_eq!(
        DocxPackage::open(&encrypted, PackageLimits::default()).unwrap_err(),
        PackageError::EncryptedEntry
    );

    let mut unsupported = original.clone();
    unsupported[8..10].copy_from_slice(&12_u16.to_le_bytes());
    unsupported[central[0] + 10..central[0] + 12].copy_from_slice(&12_u16.to_le_bytes());
    assert_eq!(
        DocxPackage::open(&unsupported, PackageLimits::default()).unwrap_err(),
        PackageError::UnsupportedCompression
    );

    let mut symlink = original.clone();
    symlink[central[0] + 5] = 3;
    symlink[central[0] + 38..central[0] + 42].copy_from_slice(&(0o120777_u32 << 16).to_le_bytes());
    assert_eq!(
        DocxPackage::open(&symlink, PackageLimits::default()).unwrap_err(),
        PackageError::SpecialEntry
    );

    let mut overlapping = original;
    let first_offset = overlapping[central[0] + 42..central[0] + 46].to_vec();
    overlapping[central[1] + 42..central[1] + 46].copy_from_slice(&first_offset);
    assert_eq!(
        DocxPackage::open(&overlapping, PackageLimits::default()).unwrap_err(),
        PackageError::OverlappingEntries
    );
}

#[test]
fn committed_package_fixtures_match_manifest_outcomes() {
    let minimal = include_bytes!("../../../fixtures/generated/minimal-valid.docx");
    let mut package = DocxPackage::open(minimal, PackageLimits::default()).unwrap();
    assert_eq!(package.read_part(DOCUMENT_PART).unwrap(), DOCUMENT);

    let mixed_unicode = include_bytes!("../../../fixtures/generated/mixed-unicode.docx");
    let mut package = DocxPackage::open(mixed_unicode, PackageLimits::default()).unwrap();
    assert_eq!(
        package.read_part(DOCUMENT_PART).unwrap(),
        MIXED_UNICODE_DOCUMENT.as_bytes()
    );

    let unknown_safe = include_bytes!("../../../fixtures/generated/unknown-safe-part.docx");
    let mut package = DocxPackage::open(unknown_safe, PackageLimits::default()).unwrap();
    assert!(
        package
            .entries()
            .iter()
            .any(|entry| entry.part_name == "customXml/item1.xml")
    );
    assert_eq!(
        package.read_part("customXml/item1.xml").unwrap(),
        UNKNOWN_SAFE_PART
    );

    let traversal = include_bytes!("../../../fixtures/generated/path-traversal.docx");
    assert_eq!(
        DocxPackage::open(traversal, PackageLimits::default()).unwrap_err(),
        PackageError::UnsafePartName
    );

    let high_expansion = include_bytes!("../../../fixtures/generated/high-expansion.docx");
    assert!(matches!(
        DocxPackage::open(
            high_expansion,
            PackageLimits {
                max_expansion_ratio: 2,
                ..PackageLimits::default()
            }
        ),
        Err(PackageError::LimitExceeded {
            limit: "entry_expansion_ratio",
            ..
        })
    ));

    let duplicate = include_bytes!("../../../fixtures/generated/duplicate-part.docx");
    assert_eq!(
        DocxPackage::open(duplicate, PackageLimits::default()).unwrap_err(),
        PackageError::DuplicatePart
    );

    let malformed = include_bytes!("../../../fixtures/generated/malformed-truncated.docx");
    assert_eq!(
        DocxPackage::open(malformed, PackageLimits::default()).unwrap_err(),
        PackageError::MalformedArchive
    );
}

#[test]
fn cancellation_returns_no_package_or_partial_part() {
    let bytes = minimal_package();
    let cancellation = CancellationToken::default();
    cancellation.cancel();
    assert_eq!(
        DocxPackage::open_with_cancellation(&bytes, PackageLimits::default(), &cancellation)
            .unwrap_err(),
        PackageError::Cancelled
    );

    let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
    assert_eq!(
        package
            .read_part_with_cancellation(DOCUMENT_PART, &cancellation)
            .unwrap_err(),
        PackageError::Cancelled
    );
}

#[test]
fn corrupt_part_data_returns_no_partial_bytes_or_document_text() {
    let mut bytes = minimal_package();
    let data_start = {
        let mut archive = ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
        usize::try_from(archive.by_index_raw(0).unwrap().data_start()).unwrap()
    };
    bytes[data_start] ^= 0xff;

    let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
    let error = package.read_part(DOCUMENT_PART).unwrap_err();
    assert_eq!(error, PackageError::PartReadFailed);
    assert!(!error.to_string().contains("<w:document"));

    let malformed = PackageError::MalformedArchive.to_string();
    assert!(!malformed.contains("secret-document-text"));
}

const CONTENT_TYPES_HEAD: &str = concat!(
    "<?xml version=\"1.0\"?>",
    "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
    "<Default Extension=\"rels\" ",
    "ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
);
const MAIN_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

fn content_types_for(part: &str, content_type: &str) -> Vec<u8> {
    format!(
        "{CONTENT_TYPES_HEAD}<Override PartName=\"/{part}\" ContentType=\"{content_type}\"/></Types>"
    )
    .into_bytes()
}

fn relationships(inner: &str) -> Vec<u8> {
    format!(
        "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{inner}</Relationships>"
    )
    .into_bytes()
}

fn office_document_relationship(rel_type: &str, target: &str, external: bool) -> String {
    let mode = if external {
        " TargetMode=\"External\""
    } else {
        ""
    };
    format!("<Relationship Id=\"rId1\" Type=\"{rel_type}\" Target=\"{target}\"{mode}/>")
}

fn discovery_package(content_types: &[u8], rels: &[u8], main_part: &str) -> Vec<u8> {
    package(&[
        (CONTENT_TYPES_PART, content_types, CompressionMethod::Stored),
        (ROOT_RELATIONSHIPS_PART, rels, CompressionMethod::Stored),
        (main_part, DOCUMENT, CompressionMethod::Deflated),
    ])
}

#[test]
fn main_document_is_discovered_at_a_non_conventional_path_without_word_document_xml() {
    let content_types = content_types_for("word/primary.xml", MAIN_TYPE);
    let rels = relationships(&office_document_relationship(
        OFFICE_DOCUMENT_REL_TRANSITIONAL,
        "word/primary.xml",
        false,
    ));
    let bytes = discovery_package(&content_types, &rels, "word/primary.xml");
    let package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
    assert_eq!(package.main_document_part(), "word/primary.xml");
    assert!(
        package
            .entries()
            .iter()
            .all(|entry| entry.part_name != DOCUMENT_PART)
    );
}

#[test]
fn strict_and_transitional_office_document_relationships_are_both_discovered() {
    for rel_type in [OFFICE_DOCUMENT_REL_TRANSITIONAL, OFFICE_DOCUMENT_REL_STRICT] {
        let content_types = content_types_for(DOCUMENT_PART, MAIN_TYPE);
        let rels = relationships(&office_document_relationship(
            rel_type,
            DOCUMENT_PART,
            false,
        ));
        let bytes = discovery_package(&content_types, &rels, DOCUMENT_PART);
        let package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        assert_eq!(package.main_document_part(), DOCUMENT_PART);
    }
}

#[test]
fn missing_ambiguous_external_and_mistyped_main_documents_are_rejected() {
    let content_types = content_types_for(DOCUMENT_PART, MAIN_TYPE);

    let empty = relationships("");
    assert_eq!(
        DocxPackage::open(
            &discovery_package(&content_types, &empty, DOCUMENT_PART),
            PackageLimits::default()
        )
        .unwrap_err(),
        PackageError::MissingMainDocument
    );

    let mut two =
        office_document_relationship(OFFICE_DOCUMENT_REL_TRANSITIONAL, DOCUMENT_PART, false);
    two.push_str(
        "<Relationship Id=\"rId2\" \
         Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" \
         Target=\"word/other.xml\"/>",
    );
    assert_eq!(
        DocxPackage::open(
            &discovery_package(&content_types, &relationships(&two), DOCUMENT_PART),
            PackageLimits::default()
        )
        .unwrap_err(),
        PackageError::AmbiguousMainDocument
    );

    let external = relationships(&office_document_relationship(
        OFFICE_DOCUMENT_REL_TRANSITIONAL,
        "https://example.invalid/document.xml",
        true,
    ));
    assert_eq!(
        DocxPackage::open(
            &discovery_package(&content_types, &external, DOCUMENT_PART),
            PackageLimits::default()
        )
        .unwrap_err(),
        PackageError::MissingMainDocument
    );

    let wrong_type = content_types_for(DOCUMENT_PART, "application/xml");
    let rels = relationships(&office_document_relationship(
        OFFICE_DOCUMENT_REL_TRANSITIONAL,
        DOCUMENT_PART,
        false,
    ));
    assert_eq!(
        DocxPackage::open(
            &discovery_package(&wrong_type, &rels, DOCUMENT_PART),
            PackageLimits::default()
        )
        .unwrap_err(),
        PackageError::UnsupportedMainDocumentType
    );
}

#[test]
fn main_document_relationship_target_escaping_the_root_is_rejected() {
    let content_types = content_types_for(DOCUMENT_PART, MAIN_TYPE);
    let rels = relationships(&office_document_relationship(
        OFFICE_DOCUMENT_REL_TRANSITIONAL,
        "../escape.xml",
        false,
    ));
    assert_eq!(
        DocxPackage::open(
            &discovery_package(&content_types, &rels, DOCUMENT_PART),
            PackageLimits::default()
        )
        .unwrap_err(),
        PackageError::UnsafePartName
    );
}

#[test]
fn malformed_relationships_xml_is_rejected_without_leaking_text() {
    let content_types = content_types_for(DOCUMENT_PART, MAIN_TYPE);
    let malformed = b"<Relationships><Relationship secret-token".to_vec();
    let error = DocxPackage::open(
        &discovery_package(&content_types, &malformed, DOCUMENT_PART),
        PackageLimits::default(),
    )
    .unwrap_err();
    assert_eq!(
        error,
        PackageError::MalformedPackageXml {
            part: ROOT_RELATIONSHIPS_PART
        }
    );
    assert!(!error.to_string().contains("secret-token"));
}

#[test]
fn main_document_relationships_are_resolved_and_classified() {
    let content_types = format!(
        "{CONTENT_TYPES_HEAD}<Default Extension=\"png\" ContentType=\"image/png\"/>\
         <Override PartName=\"/word/document.xml\" ContentType=\"{MAIN_TYPE}\"/></Types>"
    )
    .into_bytes();
    let root_rels = relationships(&office_document_relationship(
        OFFICE_DOCUMENT_REL_TRANSITIONAL,
        DOCUMENT_PART,
        false,
    ));
    let document_rels: &[u8] = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.invalid/" TargetMode="External"/><Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml" Target="../customXml/item1.xml"/><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/oleObject" Target="../../escape.bin"/></Relationships>"#;
    let entries: [(&str, &[u8], CompressionMethod); 7] = [
        (
            CONTENT_TYPES_PART,
            content_types.as_slice(),
            CompressionMethod::Stored,
        ),
        (
            ROOT_RELATIONSHIPS_PART,
            root_rels.as_slice(),
            CompressionMethod::Stored,
        ),
        (DOCUMENT_PART, DOCUMENT, CompressionMethod::Deflated),
        (
            "word/_rels/document.xml.rels",
            document_rels,
            CompressionMethod::Stored,
        ),
        ("word/styles.xml", b"<styles/>", CompressionMethod::Stored),
        (
            "word/media/image1.png",
            b"PNGDATA",
            CompressionMethod::Stored,
        ),
        ("customXml/item1.xml", b"<x/>", CompressionMethod::Stored),
    ];
    let bytes = package(&entries);
    let package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();

    let relationships = package.main_document_relationships();
    assert_eq!(relationships.len(), 5);
    assert_eq!(relationships[0].id, "rId1");
    assert_eq!(relationships[0].target_mode, TargetMode::Internal);
    assert_eq!(
        relationships[0].resolved_part.as_deref(),
        Some("word/styles.xml")
    );
    assert_eq!(
        relationships[1].resolved_part.as_deref(),
        Some("word/media/image1.png")
    );
    assert_eq!(relationships[2].target_mode, TargetMode::External);
    assert_eq!(relationships[2].resolved_part, None);
    assert_eq!(
        relationships[3].resolved_part.as_deref(),
        Some("customXml/item1.xml")
    );
    // Internal target escaping the package root resolves to nothing.
    assert_eq!(relationships[4].target_mode, TargetMode::Internal);
    assert_eq!(relationships[4].resolved_part, None);

    assert_eq!(package.content_type("word/document.xml"), Some(MAIN_TYPE));
    assert_eq!(
        package.content_type("word/media/image1.png"),
        Some("image/png")
    );
    assert_eq!(package.content_type("word/unknown.dat"), None);
}

#[test]
fn main_document_without_a_rels_part_has_no_relationships() {
    let content_types = content_types_for(DOCUMENT_PART, MAIN_TYPE);
    let root_rels = relationships(&office_document_relationship(
        OFFICE_DOCUMENT_REL_TRANSITIONAL,
        DOCUMENT_PART,
        false,
    ));
    let bytes = discovery_package(&content_types, &root_rels, DOCUMENT_PART);
    let package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
    assert!(package.main_document_relationships().is_empty());
}

#[test]
fn source_snapshot_is_deterministic_with_content_types_and_relationships() {
    let content_types = format!(
        "{CONTENT_TYPES_HEAD}<Default Extension=\"png\" ContentType=\"image/png\"/>\
         <Override PartName=\"/word/document.xml\" ContentType=\"{MAIN_TYPE}\"/></Types>"
    )
    .into_bytes();
    let root_rels = relationships(&office_document_relationship(
        OFFICE_DOCUMENT_REL_TRANSITIONAL,
        DOCUMENT_PART,
        false,
    ));
    let document_rels: &[u8] = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let entries: [(&str, &[u8], CompressionMethod); 5] = [
        (
            CONTENT_TYPES_PART,
            content_types.as_slice(),
            CompressionMethod::Stored,
        ),
        (
            ROOT_RELATIONSHIPS_PART,
            root_rels.as_slice(),
            CompressionMethod::Stored,
        ),
        (DOCUMENT_PART, DOCUMENT, CompressionMethod::Deflated),
        (
            "word/_rels/document.xml.rels",
            document_rels,
            CompressionMethod::Stored,
        ),
        (
            "word/media/image1.png",
            b"PNGDATA",
            CompressionMethod::Stored,
        ),
    ];
    let bytes = package(&entries);
    let package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();

    let snapshot = package.source_snapshot();
    // Deterministic: identical to a second call.
    assert_eq!(snapshot, package.source_snapshot());
    // Parts ordered by name, with resolved content types.
    let names: Vec<&str> = snapshot
        .parts
        .iter()
        .map(|part| part.part_name.as_str())
        .collect();
    assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    let document = snapshot
        .parts
        .iter()
        .find(|part| part.part_name == DOCUMENT_PART)
        .unwrap();
    assert_eq!(document.content_type.as_deref(), Some(MAIN_TYPE));
    let image = snapshot
        .parts
        .iter()
        .find(|part| part.part_name == "word/media/image1.png")
        .unwrap();
    assert_eq!(image.content_type.as_deref(), Some("image/png"));
    assert_eq!(snapshot.main_document_part, DOCUMENT_PART);
    assert_eq!(snapshot.main_document_relationships.len(), 1);
    assert_eq!(
        snapshot.main_document_relationships[0]
            .resolved_part
            .as_deref(),
        Some("word/media/image1.png")
    );
}
