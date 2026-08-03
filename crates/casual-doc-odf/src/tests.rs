use std::io::{Cursor, Write};

use casual_doc_package::CancellationToken;
use zip::CompressionMethod;
use zip::write::{FullFileOptions, ZipWriter};

use crate::{
    CONTENT_PART, MANIFEST_PART, MIMETYPE_PART, ODT_MIME, OdfError, OdfPackageLimits, OdfVersion,
    OdtPackage,
};

const CONTENT: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content
 xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
 xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
 office:version="1.4"><office:body><office:text><text:p>Hello ODT</text:p></office:text></office:body></office:document-content>"#;

#[derive(Clone)]
struct Entry {
    name: &'static str,
    bytes: Vec<u8>,
    compression: CompressionMethod,
    local_extra: bool,
}

fn manifest(version: &str, root_mime: &str, content_extra: &str) -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="{version}">
  <m:file-entry m:media-type="{root_mime}" m:full-path="/" m:version="{version}"/>
  <m:file-entry m:full-path="content.xml" m:media-type="text/xml">{content_extra}</m:file-entry>
</m:manifest>"#
    )
    .into_bytes()
}

fn minimal_entries(version: &str) -> Vec<Entry> {
    vec![
        Entry {
            name: MIMETYPE_PART,
            bytes: ODT_MIME.as_bytes().to_vec(),
            compression: CompressionMethod::Stored,
            local_extra: false,
        },
        Entry {
            name: MANIFEST_PART,
            bytes: manifest(version, ODT_MIME, ""),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
        Entry {
            name: CONTENT_PART,
            bytes: CONTENT.to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
    ]
}

fn package(entries: &[Entry]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for entry in entries {
        let mut options = FullFileOptions::default().compression_method(entry.compression);
        if entry.local_extra {
            options.add_extra_data(0xcafe, b"odf", false).unwrap();
        }
        writer.start_file(entry.name, options).unwrap();
        writer.write_all(&entry.bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn odf_1_2_through_1_4_are_admitted_deterministically() {
    for (version, expected) in [
        ("1.2", OdfVersion::V1_2),
        ("1.3", OdfVersion::V1_3),
        ("1.4", OdfVersion::V1_4),
    ] {
        let bytes = package(&minimal_entries(version));
        let mut odt = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
        assert_eq!(odt.version(), expected);
        assert_eq!(odt.version().as_str(), version);
        assert_eq!(odt.read_part(CONTENT_PART).unwrap(), CONTENT);
        assert_eq!(
            odt.manifest_entries()
                .iter()
                .map(|entry| entry.full_path.as_str())
                .collect::<Vec<_>>(),
            vec!["/", CONTENT_PART]
        );
        assert!(!odt.has_signatures());
    }
}

#[test]
fn mimetype_must_be_first_stored_exact_and_without_extra_data() {
    let mut not_first = minimal_entries("1.4");
    not_first.swap(0, 1);
    assert_eq!(
        OdtPackage::open(&package(&not_first), OdfPackageLimits::default()).unwrap_err(),
        OdfError::MimetypeNotFirst
    );

    let mut compressed = minimal_entries("1.4");
    compressed[0].compression = CompressionMethod::Deflated;
    assert_eq!(
        OdtPackage::open(&package(&compressed), OdfPackageLimits::default()).unwrap_err(),
        OdfError::MimetypeCompressed
    );

    let mut extra = minimal_entries("1.4");
    extra[0].local_extra = true;
    assert_eq!(
        OdtPackage::open(&package(&extra), OdfPackageLimits::default()).unwrap_err(),
        OdfError::MimetypeExtraField
    );

    let mut wrong = minimal_entries("1.4");
    wrong[0].bytes = b"application/vnd.oasis.opendocument.spreadsheet".to_vec();
    assert_eq!(
        OdtPackage::open(&package(&wrong), OdfPackageLimits::default()).unwrap_err(),
        OdfError::InvalidMimetype
    );
}

#[test]
fn manifest_root_coverage_duplicates_and_versions_fail_closed() {
    let mut wrong_root = minimal_entries("1.4");
    wrong_root[1].bytes = manifest("1.4", "application/octet-stream", "");
    assert_eq!(
        OdtPackage::open(&package(&wrong_root), OdfPackageLimits::default()).unwrap_err(),
        OdfError::ManifestMismatch
    );

    let mut missing_content = minimal_entries("1.4");
    missing_content[1].bytes = format!(
        r#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.4"><manifest:file-entry manifest:full-path="/" manifest:media-type="{ODT_MIME}"/></manifest:manifest>"#
    )
    .into_bytes();
    assert_eq!(
        OdtPackage::open(&package(&missing_content), OdfPackageLimits::default()).unwrap_err(),
        OdfError::ManifestMismatch
    );

    let mut duplicate = minimal_entries("1.4");
    duplicate[1].bytes = manifest(
        "1.4",
        ODT_MIME,
        r#"</m:file-entry><m:file-entry m:full-path="content.xml" m:media-type="text/xml">"#,
    );
    assert_eq!(
        OdtPackage::open(&package(&duplicate), OdfPackageLimits::default()).unwrap_err(),
        OdfError::ManifestMismatch
    );

    let unsupported = package(&minimal_entries("1.5"));
    assert_eq!(
        OdtPackage::open(&unsupported, OdfPackageLimits::default()).unwrap_err(),
        OdfError::UnsupportedVersion
    );

    let mut unsafe_directory = minimal_entries("1.4");
    unsafe_directory[1].bytes = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/><m:file-entry m:full-path="../" m:media-type=""/></m:manifest>"#
    )
    .into_bytes();
    assert_eq!(
        OdtPackage::open(&package(&unsafe_directory), OdfPackageLimits::default()).unwrap_err(),
        OdfError::ManifestMismatch
    );

    let mut encoded_traversal = minimal_entries("1.4");
    encoded_traversal[1].bytes = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/><m:file-entry m:full-path="%2e%2e/" m:media-type=""/></m:manifest>"#
    )
    .into_bytes();
    assert_eq!(
        OdtPackage::open(&package(&encoded_traversal), OdfPackageLimits::default()).unwrap_err(),
        OdfError::ManifestMismatch
    );
}

#[test]
fn encryption_dtd_and_active_content_are_refused() {
    let mut encrypted = minimal_entries("1.4");
    encrypted[1].bytes = manifest("1.4", ODT_MIME, "<m:encryption-data/>");
    assert_eq!(
        OdtPackage::open(&package(&encrypted), OdfPackageLimits::default()).unwrap_err(),
        OdfError::EncryptedDocument
    );

    let mut dtd = minimal_entries("1.4");
    dtd[1].bytes = format!(
        r#"<!DOCTYPE manifest [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.4"><manifest:file-entry manifest:full-path="/" manifest:media-type="{ODT_MIME}"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#
    )
    .into_bytes();
    assert_eq!(
        OdtPackage::open(&package(&dtd), OdfPackageLimits::default()).unwrap_err(),
        OdfError::MalformedManifest
    );

    let mut active = minimal_entries("1.4");
    active.push(Entry {
        name: "Basic/Standard/Module1.xml",
        bytes: b"macro".to_vec(),
        compression: CompressionMethod::Stored,
        local_extra: false,
    });
    active[1].bytes = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/><m:file-entry m:full-path="Basic/Standard/Module1.xml" m:media-type="text/xml"/></m:manifest>"#
    )
    .into_bytes();
    assert_eq!(
        OdtPackage::open(&package(&active), OdfPackageLimits::default()).unwrap_err(),
        OdfError::ActiveContent
    );
}

#[test]
fn signatures_are_presence_facts_not_validity_claims() {
    let mut entries = minimal_entries("1.4");
    entries.push(Entry {
        name: "META-INF/documentsignatures.xml",
        bytes: b"<signatures/>".to_vec(),
        compression: CompressionMethod::Deflated,
        local_extra: false,
    });
    let bytes = package(&entries);
    let odt = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    assert!(odt.has_signatures());
}

#[test]
fn limits_and_cancellation_are_enforced_atomically() {
    let bytes = package(&minimal_entries("1.4"));
    let cancellation = CancellationToken::default();
    cancellation.cancel();
    assert_eq!(
        OdtPackage::open_with_cancellation(&bytes, OdfPackageLimits::default(), &cancellation,)
            .unwrap_err(),
        OdfError::Cancelled
    );

    let limits = OdfPackageLimits {
        max_manifest_bytes: 8,
        ..OdfPackageLimits::default()
    };
    assert!(matches!(
        OdtPackage::open(&bytes, limits),
        Err(OdfError::LimitExceeded {
            limit: "odf_manifest_bytes",
            ..
        })
    ));

    let invalid = OdfPackageLimits {
        max_xml_depth: usize::MAX,
        ..OdfPackageLimits::default()
    };
    assert!(matches!(
        OdtPackage::open(&bytes, invalid),
        Err(OdfError::InvalidLimitConfiguration {
            limit: "odf_xml_depth",
            ..
        })
    ));
}
