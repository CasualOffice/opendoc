//! Synthetic compound files, built here from [MS-CFB] rather than copied.
//!
//! Every fixture in this file is **built in code**. There is no encrypted
//! document anywhere in `fixtures/`, the owner's own documents are private and
//! must never be committed or derived from, and a recognition test does not need
//! a real one: what is being tested is whether a container with a given shape is
//! named correctly, and the shape is exactly what a builder can state.
//!
//! The builder writes version 3 (512-byte sectors), which is what Office has
//! written since 2007 SP2.

use crate::{CfbError, CfbKind, CfbLimits, SIGNATURE, classify, has_signature};

const SECTOR: usize = 512;
const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
const FREE_SECTOR: u32 = 0xFFFF_FFFF;

/// A minimal, valid, version-3 compound file carrying the named streams.
struct Builder {
    /// Stream names, in directory order after the root entry.
    streams: Vec<&'static str>,
    /// Directory sectors to emit, so a chain longer than one can be built.
    directory_sectors: usize,
    /// Overrides for the FAT, applied after the default chain is laid down.
    fat_overrides: Vec<(u32, u32)>,
    /// Override for one entry's declared stream size.
    stream_size_override: Option<u64>,
    /// Override for the declared FAT sector count.
    fat_sector_count: Option<u32>,
}

impl Builder {
    fn new(streams: &[&'static str]) -> Self {
        Self {
            streams: streams.to_vec(),
            directory_sectors: 1,
            fat_overrides: Vec::new(),
            stream_size_override: None,
            fat_sector_count: None,
        }
    }

    fn build(&self) -> Vec<u8> {
        // Sector 0 is the FAT. Sectors 1.. are the directory.
        let sectors = 1 + self.directory_sectors;
        let mut bytes = vec![0_u8; SECTOR * (1 + sectors)];
        bytes[..8].copy_from_slice(&SIGNATURE);
        write_u16(&mut bytes, 24, 0x003E); // minor version
        write_u16(&mut bytes, 26, 3); // major version 3
        write_u16(&mut bytes, 28, 0xFFFE); // byte order
        write_u16(&mut bytes, 30, 9); // 512-byte sectors
        write_u16(&mut bytes, 32, 6); // mini sector shift
        write_u32(&mut bytes, 44, self.fat_sector_count.unwrap_or(1));
        write_u32(&mut bytes, 48, 1); // first directory sector
        write_u32(&mut bytes, 56, 4096); // mini stream cutoff
        write_u32(&mut bytes, 60, END_OF_CHAIN); // first mini FAT sector
        write_u32(&mut bytes, 68, END_OF_CHAIN); // first DIFAT sector
        // Header DIFAT: FAT sector 0, then free.
        write_u32(&mut bytes, 76, 0);
        for index in 1..109 {
            write_u32(&mut bytes, 76 + index * 4, FREE_SECTOR);
        }

        // The FAT itself, in sector 0 (file offset 512). Sector 0 is the FAT
        // (FATSECT), the directory sectors chain, and everything else is free.
        let fat = SECTOR;
        for slot in 0..SECTOR / 4 {
            write_u32(&mut bytes, fat + slot * 4, FREE_SECTOR);
        }
        write_u32(&mut bytes, fat, 0xFFFF_FFFD); // FATSECT
        for index in 0..self.directory_sectors {
            let sector = 1 + index;
            let next = if index + 1 == self.directory_sectors {
                END_OF_CHAIN
            } else {
                u32::try_from(sector + 1).expect("sector fits u32")
            };
            write_u32(&mut bytes, fat + sector * 4, next);
        }
        for &(sector, value) in &self.fat_overrides {
            write_u32(
                &mut bytes,
                fat + usize::try_from(sector).unwrap() * 4,
                value,
            );
        }

        // The directory. Entry 0 is the root storage, then one stream each.
        let directory = SECTOR * 2;
        write_entry(&mut bytes, directory, "Root Entry", 5, 0);
        for (index, name) in self.streams.iter().enumerate() {
            let size = self.stream_size_override.unwrap_or(64);
            write_entry(&mut bytes, directory + (index + 1) * 128, name, 2, size);
        }
        bytes
    }
}

fn write_entry(bytes: &mut [u8], at: usize, name: &str, object_type: u8, size: u64) {
    let mut units = 0;
    for (index, unit) in name.encode_utf16().enumerate() {
        write_u16(bytes, at + index * 2, unit);
        units = index + 1;
    }
    write_u16(
        bytes,
        at + 64,
        u16::try_from(units * 2 + 2).expect("name fits"),
    );
    bytes[at + 66] = object_type;
    bytes[at + 67] = 1; // black
    write_u32(bytes, at + 68, FREE_SECTOR); // left sibling
    write_u32(bytes, at + 72, FREE_SECTOR); // right sibling
    write_u32(bytes, at + 76, FREE_SECTOR); // child
    write_u32(bytes, at + 116, FREE_SECTOR); // starting sector
    write_u64(bytes, at + 120, size);
}

fn write_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

/// The fixture the whole programme exists for: the shape of a password-protected
/// `.docx`, which is a compound file and not a ZIP.
fn encrypted_package() -> Vec<u8> {
    Builder::new(&["EncryptionInfo", "EncryptedPackage", "\u{6}DataSpaces"]).build()
}

#[test]
fn the_signature_sniff_costs_eight_bytes_and_accepts_nothing_else() {
    assert!(has_signature(&encrypted_package()));
    assert!(has_signature(&SIGNATURE));
    // A ZIP, which is what every format this engine actually reads looks like.
    assert!(!has_signature(b"PK\x03\x04and then some"));
    assert!(!has_signature(b"{\\rtf1"));
    assert!(
        !has_signature(&SIGNATURE[..7]),
        "a truncated signature is not one"
    );
    assert!(!has_signature(b""));
}

#[test]
fn an_encrypted_office_package_is_named_by_its_two_streams() {
    assert_eq!(
        classify(&encrypted_package(), CfbLimits::default()),
        Ok(CfbKind::EncryptedOfficePackage)
    );
}

#[test]
fn either_encryption_stream_alone_is_not_an_encrypted_package() {
    // [MS-OFFCRYPTO] §2.3.4 requires the pair. Calling one of them encrypted
    // would tell a reader their document is password-protected when it is
    // something else, which is the overstatement `SKILL` §9 rule 6 forbids in
    // both directions.
    for streams in [
        vec!["EncryptionInfo"],
        vec!["EncryptedPackage"],
        vec!["\u{6}DataSpaces"],
    ] {
        assert_eq!(
            classify(&Builder::new(&streams).build(), CfbLimits::default()),
            Ok(CfbKind::Unrecognised),
            "{streams:?} alone must not be called an encrypted package"
        );
    }
}

#[test]
fn a_legacy_binary_office_file_is_recognised_and_not_mistaken_for_encryption() {
    for (streams, kind) in [
        (vec!["WordDocument", "1Table"], CfbKind::LegacyWord),
        (vec!["Workbook"], CfbKind::LegacyExcel),
        (vec!["Book"], CfbKind::LegacyExcel),
        (vec!["PowerPoint Document"], CfbKind::LegacyPowerPoint),
    ] {
        assert_eq!(
            classify(&Builder::new(&streams).build(), CfbLimits::default()),
            Ok(kind),
            "{streams:?}"
        );
    }
}

#[test]
fn a_legacy_word_file_that_is_also_encrypted_reports_the_encryption() {
    // An RC4 CryptoAPI-protected `.doc` carries both. The actionable fact for a
    // reader is that it needs a password, not that it is old.
    assert_eq!(
        classify(
            &Builder::new(&["WordDocument", "EncryptionInfo", "EncryptedPackage"]).build(),
            CfbLimits::default()
        ),
        Ok(CfbKind::EncryptedOfficePackage)
    );
}

#[test]
fn an_input_that_is_not_a_compound_file_is_refused_as_such() {
    for bytes in [b"PK\x03\x04".as_slice(), b"", b"{\\rtf1}"] {
        assert_eq!(
            classify(bytes, CfbLimits::default()),
            Err(CfbError::NotCompoundFile)
        );
    }
}

// ---- The adversarial fixtures. Each one must refuse, and refuse QUICKLY -------
//
// These are the four shapes `docs/163` §10 lists as belonging in the fuzz corpus,
// driven here as unit tests as well so a regression is a named failure rather
// than a fuzz finding weeks later.

#[test]
fn a_truncated_compound_file_is_refused_rather_than_read_past_its_end() {
    let whole = encrypted_package();
    // Every prefix, which covers a header cut in half, a header with no
    // directory, and a directory sector that stops mid-entry.
    for cut in [8, 100, 511, 512, 513, 1023, whole.len() - 1] {
        let error = classify(&whole[..cut], CfbLimits::default())
            .expect_err("a truncated container must be refused");
        assert!(
            matches!(
                error,
                CfbError::MalformedHeader | CfbError::TruncatedSector { .. }
            ),
            "cut at {cut} gave {error:?}"
        );
    }
}

#[test]
fn a_fat_cycle_is_refused_by_the_chain_ceiling_and_does_not_hang() {
    // The directory chain points sector 1 back at itself: a self-referential
    // linked list. Without the ceiling this walk never ends.
    let mut builder = Builder::new(&["EncryptionInfo", "EncryptedPackage"]);
    builder.fat_overrides.push((1, 1));
    let bytes = builder.build();

    let start = std::time::Instant::now();
    let error = classify(&bytes, CfbLimits::default()).expect_err("a cycle must be refused");
    // Under the DEFAULT limits the entry ceiling is what stops it, not the chain
    // ceiling: a 512-byte sector holds four 128-byte entries, so 4,096 entries
    // are reached after 1,024 sectors and the 8,192-sector ceiling is never
    // approached. That is the measured truth and the test says so rather than
    // asserting the limit it would be tidier to credit — a guard pinned to the
    // wrong mechanism passes for the wrong reason (`SKILL` §4).
    assert_eq!(
        error,
        CfbError::LimitExceeded {
            limit: "cfb_directory_entries",
            observed: 4_097,
            allowed: 4_096,
        }
    );
    // A bound on the WORK, not on the clock: a few thousand 512-byte reads,
    // however loaded the machine is. Deliberately loose, for the reason `SKILL`
    // §6 gives about clock-bound tests.
    assert!(
        start.elapsed() < std::time::Duration::from_secs(5),
        "a FAT cycle must not hang: took {:?}",
        start.elapsed()
    );
}

#[test]
fn the_chain_ceiling_is_armed_as_well_as_the_entry_ceiling() {
    // Both guards are real and either alone would leave a hole: the entry
    // ceiling counts entries and a chain of sectors holding only *unallocated*
    // slots still has to terminate. With the entry ceiling raised out of the
    // way, the chain ceiling is what refuses the same cycle.
    let mut builder = Builder::new(&["EncryptionInfo", "EncryptedPackage"]);
    builder.fat_overrides.push((1, 1));
    let limits = CfbLimits {
        max_directory_entries: CfbLimits::HARD_MAX_DIRECTORY_ENTRIES,
        max_chain_sectors: 64,
        ..CfbLimits::default()
    };
    assert_eq!(
        classify(&builder.build(), limits),
        Err(CfbError::LimitExceeded {
            limit: "cfb_chain_sectors",
            observed: 65,
            allowed: 64,
        })
    );
}

#[test]
fn a_two_sector_cycle_is_refused_too() {
    // A cycle does not have to be a self-loop, and a visited-set guard written
    // only against self-loops would pass this.
    let mut builder = Builder::new(&["EncryptionInfo"]);
    builder.directory_sectors = 2;
    builder.fat_overrides.push((2, 1));
    let error = classify(&builder.build(), CfbLimits::default()).expect_err("refused");
    assert!(
        matches!(
            error,
            CfbError::LimitExceeded {
                limit: "cfb_directory_entries" | "cfb_chain_sectors",
                ..
            }
        ),
        "a two-sector cycle must hit one of the two ceilings, not run: {error:?}"
    );
}

#[test]
fn an_absurd_stream_size_is_refused_rather_than_believed() {
    // A stream cannot be longer than the file holding it, and a reader that
    // trusted this number would allocate or read from it.
    let mut builder = Builder::new(&["EncryptionInfo", "EncryptedPackage"]);
    builder.stream_size_override = Some(u64::MAX);
    assert_eq!(
        classify(&builder.build(), CfbLimits::default()),
        Err(CfbError::MalformedDirectoryEntry)
    );

    let mut just_over = Builder::new(&["EncryptionInfo", "EncryptedPackage"]);
    just_over.stream_size_override = Some(u64::try_from(SECTOR * 3 + 1).expect("fits"));
    assert_eq!(
        classify(&just_over.build(), CfbLimits::default()),
        Err(CfbError::MalformedDirectoryEntry),
        "one byte over the input length is still over it"
    );
}

#[test]
fn a_directory_larger_than_the_limit_is_refused() {
    // The "2^31 directory entries" fixture, driven at the limit rather than at
    // the claim: a declared entry count is not what bounds the walk, the LIMIT
    // is, so the test lowers the limit and proves the walk stops.
    let mut builder = Builder::new(&["EncryptionInfo", "EncryptedPackage"]);
    builder.directory_sectors = 4;
    let limits = CfbLimits {
        max_directory_entries: 5,
        ..CfbLimits::default()
    };
    assert_eq!(
        classify(&builder.build(), limits),
        Err(CfbError::LimitExceeded {
            limit: "cfb_directory_entries",
            observed: 6,
            allowed: 5,
        })
    );
}

#[test]
fn an_oversized_input_is_refused_before_the_header_is_parsed() {
    let limits = CfbLimits {
        max_input_bytes: 16,
        ..CfbLimits::default()
    };
    assert_eq!(
        classify(&encrypted_package(), limits),
        Err(CfbError::LimitExceeded {
            limit: "cfb_input_bytes",
            observed: 1536,
            allowed: 16,
        })
    );
}

#[test]
fn a_limit_above_its_hard_ceiling_is_refused_and_never_silently_clamped() {
    let limits = CfbLimits {
        max_chain_sectors: CfbLimits::HARD_MAX_CHAIN_SECTORS + 1,
        ..CfbLimits::default()
    };
    assert_eq!(
        classify(&encrypted_package(), limits),
        Err(CfbError::InvalidLimitConfiguration {
            limit: "cfb_chain_sectors",
            value: u64::from(CfbLimits::HARD_MAX_CHAIN_SECTORS) + 1,
            hard_ceiling: u64::from(CfbLimits::HARD_MAX_CHAIN_SECTORS),
        })
    );
}

#[test]
fn a_header_with_a_version_or_sector_shift_outside_the_spec_is_refused() {
    let mut bytes = encrypted_package();
    write_u16(&mut bytes, 30, 11); // 2,048-byte sectors: no such version
    assert_eq!(
        classify(&bytes, CfbLimits::default()),
        Err(CfbError::UnsupportedVersion {
            major: 3,
            sector_shift: 11,
        })
    );

    let mut wrong_order = encrypted_package();
    write_u16(&mut wrong_order, 28, 0xFEFF);
    assert_eq!(
        classify(&wrong_order, CfbLimits::default()),
        Err(CfbError::MalformedHeader),
        "no big-endian compound file was ever written; this is a file that merely \
         starts with the signature"
    );
}

#[test]
fn a_malformed_directory_entry_is_refused() {
    for (at, value) in [
        (66_usize, 7_u8), // object type outside §2.6.1
        (64, 1),          // odd name length
        (65, 1),          // name length 257, past the 64-byte field
    ] {
        let mut bytes = encrypted_package();
        bytes[SECTOR * 2 + 128 + at] = value;
        assert_eq!(
            classify(&bytes, CfbLimits::default()),
            Err(CfbError::MalformedDirectoryEntry),
            "byte {at} set to {value}"
        );
    }
}

#[test]
fn a_storage_named_like_an_encryption_stream_is_not_one() {
    // `EncryptionInfo` is a STREAM in §2.3.4. A storage of the same name holds
    // no parameters, and naming the file encrypted on that basis would be a
    // claim about a shape the file does not have.
    let mut bytes = encrypted_package();
    bytes[SECTOR * 2 + 128 + 66] = 1; // the EncryptionInfo entry becomes a storage
    assert_eq!(
        classify(&bytes, CfbLimits::default()),
        Ok(CfbKind::Unrecognised)
    );
}

#[test]
fn every_kind_has_a_stable_machine_name() {
    for kind in [
        CfbKind::EncryptedOfficePackage,
        CfbKind::LegacyWord,
        CfbKind::LegacyExcel,
        CfbKind::LegacyPowerPoint,
        CfbKind::Unrecognised,
    ] {
        let name = kind.as_str();
        assert!(
            !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'-'),
            "{kind:?} is named {name:?}, which is not a machine-routable slug"
        );
    }
}
