use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::{BoundedPackage, CancellationToken, PackageError, PackageLimits, PartCompression};

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

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let value: [u8; 2] = bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?;
    Some(u16::from_le_bytes(value))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let value: [u8; 4] = bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_le_bytes(value))
}

fn find_eocd(bytes: &[u8]) -> Option<usize> {
    let minimum = bytes.len().saturating_sub(22 + usize::from(u16::MAX));
    for position in (minimum..=bytes.len().checked_sub(22)?).rev() {
        if bytes.get(position..position + 4) != Some(b"PK\x05\x06") {
            continue;
        }
        let comment = usize::from(read_u16(bytes, position + 20)?);
        if position.checked_add(22)?.checked_add(comment)? == bytes.len() {
            return Some(position);
        }
    }
    None
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
fn arbitrary_zip_is_admitted_and_read_deterministically() {
    let bytes = package(&[
        ("z-last.txt", b"last", CompressionMethod::Deflated),
        (
            "mimetype",
            b"application/vnd.oasis.opendocument.text",
            CompressionMethod::Stored,
        ),
        (
            "word/vbaProject.bin",
            b"opaque macro bytes",
            CompressionMethod::Stored,
        ),
    ]);
    let mut package = BoundedPackage::open(&bytes, PackageLimits::default()).unwrap();

    assert_eq!(package.source_order("z-last.txt"), Some(0));
    assert_eq!(package.source_order("mimetype"), Some(1));
    assert_eq!(package.entries()[0].compression, PartCompression::Stored);
    assert_eq!(package.entries()[0].local_extra_bytes, 0);
    assert_eq!(
        package
            .entries()
            .iter()
            .map(|entry| entry.part_name.as_str())
            .collect::<Vec<_>>(),
        vec!["mimetype", "word/vbaProject.bin", "z-last.txt"]
    );
    assert_eq!(package.read_part("z-last.txt").unwrap(), b"last");
    assert_eq!(
        package.read_part("missing"),
        Err(PackageError::PartNotFound)
    );
    assert_eq!(package.package_bytes(), u64::try_from(bytes.len()).unwrap());
    assert_eq!(package.total_expanded_bytes(), 61);
}

#[test]
fn path_and_resource_limits_fail_closed() {
    for unsafe_name in [
        "../evil.xml",
        "/absolute.xml",
        "C:/drive.xml",
        "folder\\evil.xml",
        "folder/%2e%2e/evil.xml",
    ] {
        let bytes = package(&[(unsafe_name, b"unsafe", CompressionMethod::Stored)]);
        assert_eq!(
            BoundedPackage::open(&bytes, PackageLimits::default()).unwrap_err(),
            PackageError::UnsafePartName
        );
    }

    let bytes = package(&[("part", b"payload", CompressionMethod::Stored)]);
    assert!(matches!(
        BoundedPackage::open(
            &bytes,
            PackageLimits {
                max_input_bytes: bytes.len() - 1,
                ..PackageLimits::default()
            }
        ),
        Err(PackageError::LimitExceeded {
            limit: "input_package_bytes",
            ..
        })
    ));
    assert!(matches!(
        BoundedPackage::open(
            &bytes,
            PackageLimits {
                max_entries: 0,
                ..PackageLimits::default()
            }
        ),
        Err(PackageError::LimitExceeded {
            limit: "zip_entries",
            ..
        })
    ));
}

#[test]
fn encrypted_unsupported_special_and_overlapping_entries_are_rejected() {
    let original = package(&[
        ("one", b"one", CompressionMethod::Stored),
        ("two", b"two", CompressionMethod::Stored),
    ]);
    let central = central_record_positions(&original);

    let mut encrypted = original.clone();
    encrypted[6] |= 1;
    encrypted[central[0] + 8] |= 1;
    assert_eq!(
        BoundedPackage::open(&encrypted, PackageLimits::default()).unwrap_err(),
        PackageError::EncryptedEntry
    );

    let mut unsupported = original.clone();
    unsupported[8..10].copy_from_slice(&12_u16.to_le_bytes());
    unsupported[central[0] + 10..central[0] + 12].copy_from_slice(&12_u16.to_le_bytes());
    assert_eq!(
        BoundedPackage::open(&unsupported, PackageLimits::default()).unwrap_err(),
        PackageError::UnsupportedCompression
    );

    let mut special = original.clone();
    special[central[0] + 5] = 3;
    special[central[0] + 38..central[0] + 42].copy_from_slice(&(0o120777_u32 << 16).to_le_bytes());
    assert_eq!(
        BoundedPackage::open(&special, PackageLimits::default()).unwrap_err(),
        PackageError::SpecialEntry
    );

    let mut overlapping = original;
    let first_offset = overlapping[central[0] + 42..central[0] + 46].to_vec();
    overlapping[central[1] + 42..central[1] + 46].copy_from_slice(&first_offset);
    assert_eq!(
        BoundedPackage::open(&overlapping, PackageLimits::default()).unwrap_err(),
        PackageError::OverlappingEntries
    );
}

#[test]
fn cancellation_and_corrupt_part_reads_return_no_partial_bytes() {
    let bytes = package(&[("part", b"document payload", CompressionMethod::Deflated)]);
    let cancellation = CancellationToken::default();
    cancellation.cancel();
    assert_eq!(
        BoundedPackage::open_with_cancellation(&bytes, PackageLimits::default(), &cancellation,)
            .unwrap_err(),
        PackageError::Cancelled
    );

    let mut admitted = BoundedPackage::open(&bytes, PackageLimits::default()).unwrap();
    assert_eq!(
        admitted
            .read_part_with_cancellation("part", &cancellation)
            .unwrap_err(),
        PackageError::Cancelled
    );

    let mut corrupt = bytes;
    let data_start = {
        let mut archive = ZipArchive::new(Cursor::new(corrupt.as_slice())).unwrap();
        usize::try_from(archive.by_index_raw(0).unwrap().data_start()).unwrap()
    };
    corrupt[data_start] ^= 0xff;
    let mut admitted = BoundedPackage::open(&corrupt, PackageLimits::default()).unwrap();
    assert_eq!(
        admitted.read_part("part"),
        Err(PackageError::PartReadFailed)
    );
}
