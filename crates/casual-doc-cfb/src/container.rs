//! Bounded MS-CFB header parse, FAT access, and directory enumeration.
//!
//! # The established pattern, named before designing it
//!
//! This is a **virtual filesystem over a block device with a File Allocation
//! Table** — sectors, a FAT, a chained DIFAT indexing the FAT itself, and a
//! directory stored as a red-black tree. The prior art is FAT12/16, and the
//! admission pattern copied here is this repository's own `BoundedPackage`:
//! enumerate once under explicit limits, refuse out-of-range and cyclic
//! structures up front, and only then answer questions.
//!
//! One deliberate departure from [MS-CFB]: the directory is **not** walked as a
//! tree. Entries live in fixed 128-byte slots in the directory stream, so a flat
//! scan of that stream finds every entry in one pass with no sibling pointers
//! followed at all. A red-black tree with a cycle in its `left`/`right`/`child`
//! pointers is a hang; a flat scan cannot have one, and for recognition the tree
//! order carries no information we need.

use crate::error::CfbError;
use crate::limits::{CfbLimits, enforce_limit, usize_to_u64};

/// The MS-CFB header signature: [MS-CFB] §2.2, "MUST be set to the value 0xD0,
/// 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1".
pub const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// Bytes of the header, which also fixes the minimum size of any compound file.
const HEADER_BYTES: usize = 512;
/// Bytes of one directory entry ([MS-CFB] §2.6.1).
const DIRECTORY_ENTRY_BYTES: usize = 128;
/// Maximum bytes of a directory entry's UTF-16 name field.
const NAME_FIELD_BYTES: usize = 64;
/// Header DIFAT entries ([MS-CFB] §2.2).
const HEADER_DIFAT_ENTRIES: usize = 109;
/// Offset of the header's own DIFAT array.
const HEADER_DIFAT_OFFSET: usize = 76;

/// `MAXREGSECT`: the largest sector number that names real storage.
const MAX_REGULAR_SECTOR: u32 = 0xFFFF_FFFA;
/// `ENDOFCHAIN`.
const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
/// `FREESECT`.
const FREE_SECTOR: u32 = 0xFFFF_FFFF;

/// Object type of a stream object ([MS-CFB] §2.6.1).
const OBJECT_TYPE_STREAM: u8 = 2;
/// Object type of a storage object.
const OBJECT_TYPE_STORAGE: u8 = 1;
/// Object type of the root storage object.
const OBJECT_TYPE_ROOT: u8 = 5;
/// Object type of an unallocated directory slot.
const OBJECT_TYPE_UNALLOCATED: u8 = 0;

/// Returns whether `bytes` opens with the MS-CFB header signature.
///
/// This is the whole cheap half of detection and it is why a CFB probe is not a
/// tax on the common path: `FormatRegistry::detect` probes *every* registered
/// importer on *every* open, so the question "is this even a compound file" has
/// to be answerable in eight byte comparisons. Nothing else in this crate runs
/// unless this returns `true`.
///
/// Complexity: O(1).
#[must_use]
pub fn has_signature(bytes: &[u8]) -> bool {
    bytes.starts_with(&SIGNATURE)
}

/// What a compound file turned out to hold.
///
/// Named by the well-known streams found at the root, never by the file
/// extension or the MIME type: an encrypted `.docx` is a compound file whose
/// extension still says `.docx` and whose MIME type still says
/// `…wordprocessingml.document`, so both of those lie by construction
/// (`docs/163` §2.1).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CfbKind {
    /// `EncryptionInfo` and `EncryptedPackage` are both present: an ECMA-376
    /// package encrypted per [MS-OFFCRYPTO] §2.3.4, held in the data-spaces
    /// structure of §2.1.
    EncryptedOfficePackage,
    /// A legacy binary Word document (`WordDocument` stream). Not a format this
    /// engine reads at all; recognised so it can be refused by name.
    LegacyWord,
    /// A legacy binary Excel workbook (`Workbook` or `Book` stream).
    LegacyExcel,
    /// A legacy binary PowerPoint presentation.
    LegacyPowerPoint,
    /// A valid compound file holding none of the above.
    Unrecognised,
}

impl CfbKind {
    /// A stable, lowercase, machine-routable name. Never a sentence.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EncryptedOfficePackage => "encrypted-office-package",
            Self::LegacyWord => "legacy-word",
            Self::LegacyExcel => "legacy-excel",
            Self::LegacyPowerPoint => "legacy-powerpoint",
            Self::Unrecognised => "unrecognised",
        }
    }
}

/// Classifies a compound file by the well-known streams at its root.
///
/// Reads the header, validates it, then enumerates the directory stream under
/// `limits` and reports what it found. Nothing in the file is decrypted,
/// decompressed or interpreted; no stream contents are read.
///
/// Complexity: O(directory entries + directory sectors), both bounded by
/// `limits`. One pass, no allocation beyond a fixed-size set of flags.
///
/// # Errors
///
/// [`CfbError::NotCompoundFile`] when the signature is absent, and one of the
/// other variants when the container is malformed, truncated, or past a limit.
pub fn classify(bytes: &[u8], limits: CfbLimits) -> Result<CfbKind, CfbError> {
    let container = Container::open(bytes, limits)?;
    container.classify()
}

/// A validated header plus the bytes it describes.
struct Container<'a> {
    bytes: &'a [u8],
    limits: CfbLimits,
    sector_bytes: usize,
    fat_sector_count: u32,
    first_directory_sector: u32,
    first_difat_sector: u32,
}

impl<'a> Container<'a> {
    fn open(bytes: &'a [u8], limits: CfbLimits) -> Result<Self, CfbError> {
        limits.validate()?;
        if !has_signature(bytes) {
            return Err(CfbError::NotCompoundFile);
        }
        enforce_limit(
            "cfb_input_bytes",
            usize_to_u64(bytes.len()),
            usize_to_u64(limits.max_input_bytes),
        )?;
        if bytes.len() < HEADER_BYTES {
            return Err(CfbError::MalformedHeader);
        }
        // Byte order MUST be 0xFFFE ([MS-CFB] §2.2). A file that fails this is
        // not a big-endian compound file — no such thing was ever written — it is
        // a file that happens to start with the signature.
        if read_u16(bytes, 28)? != 0xFFFE {
            return Err(CfbError::MalformedHeader);
        }
        let major = read_u16(bytes, 26)?;
        let sector_shift = read_u16(bytes, 30)?;
        // §2.2: version 3 uses 512-byte sectors (shift 0x0009) and version 4
        // uses 4096 (shift 0x000C). No other pairing is defined, and accepting
        // one would mean computing sector offsets from a number the format does
        // not allow.
        let sector_bytes = match (major, sector_shift) {
            (3, 9) => 512,
            (4, 12) => 4096,
            _ => {
                return Err(CfbError::UnsupportedVersion {
                    major,
                    sector_shift,
                });
            }
        };
        // The mini sector shift is fixed at 0x0006 in every version.
        if read_u16(bytes, 32)? != 6 {
            return Err(CfbError::MalformedHeader);
        }
        Ok(Self {
            bytes,
            limits,
            sector_bytes,
            fat_sector_count: read_u32(bytes, 44)?,
            first_directory_sector: read_u32(bytes, 48)?,
            first_difat_sector: read_u32(bytes, 68)?,
        })
    }

    /// Byte range of one sector. Sector `n` begins at `(n + 1) << sector_shift`
    /// because sector numbering starts *after* the header ([MS-CFB] §2.1).
    fn sector(&self, sector: u32) -> Result<&'a [u8], CfbError> {
        if sector > MAX_REGULAR_SECTOR {
            return Err(CfbError::TruncatedSector { sector });
        }
        let start = (usize::try_from(sector).map_err(|_| CfbError::TruncatedSector { sector })?
            + 1)
        .checked_mul(self.sector_bytes)
        .ok_or(CfbError::TruncatedSector { sector })?;
        let end = start
            .checked_add(self.sector_bytes)
            .ok_or(CfbError::TruncatedSector { sector })?;
        self.bytes
            .get(start..end)
            .ok_or(CfbError::TruncatedSector { sector })
    }

    /// The sector number of FAT sector `index`, from the header DIFAT and then
    /// from the chained DIFAT sectors.
    ///
    /// The DIFAT is itself a chain, so it gets its own ceiling: a DIFAT whose
    /// last pointer loops back to an earlier DIFAT sector would otherwise be an
    /// infinite walk before the FAT is ever consulted.
    fn fat_sector(&self, index: u32) -> Result<u32, CfbError> {
        let index = usize::try_from(index).map_err(|_| CfbError::MalformedHeader)?;
        if index < HEADER_DIFAT_ENTRIES {
            return read_u32(self.bytes, HEADER_DIFAT_OFFSET + index * 4);
        }
        let per_sector = self.sector_bytes / 4 - 1;
        let mut remaining = index - HEADER_DIFAT_ENTRIES;
        let mut difat = self.first_difat_sector;
        let mut walked = 0_u64;
        loop {
            if difat > MAX_REGULAR_SECTOR {
                return Err(CfbError::MalformedHeader);
            }
            walked += 1;
            enforce_limit(
                "cfb_difat_sectors",
                walked,
                u64::from(self.limits.max_difat_sectors),
            )?;
            let sector = self.sector(difat)?;
            if remaining < per_sector {
                return read_u32(sector, remaining * 4);
            }
            remaining -= per_sector;
            difat = read_u32(sector, per_sector * 4)?;
        }
    }

    /// The FAT entry for `sector`: the next sector in its chain, or a terminator.
    fn next_in_chain(&self, sector: u32) -> Result<u32, CfbError> {
        let per_sector = u32::try_from(self.sector_bytes / 4).unwrap_or(u32::MAX);
        let fat_index = sector / per_sector;
        if fat_index >= self.fat_sector_count {
            return Err(CfbError::TruncatedSector { sector });
        }
        let fat = self.sector(self.fat_sector(fat_index)?)?;
        let offset = usize::try_from(sector % per_sector)
            .map_err(|_| CfbError::MalformedHeader)?
            .checked_mul(4)
            .ok_or(CfbError::MalformedHeader)?;
        read_u32(fat, offset)
    }

    fn classify(&self) -> Result<CfbKind, CfbError> {
        let mut found = Found::default();
        let mut sector = self.first_directory_sector;
        let mut sectors_walked = 0_u64;
        let mut entries_seen = 0_u64;
        let entries_per_sector = self.sector_bytes / DIRECTORY_ENTRY_BYTES;
        while sector <= MAX_REGULAR_SECTOR {
            sectors_walked += 1;
            // The cycle guard. A FAT chain that loops — the adversarial case the
            // design calls out — walks here forever otherwise, and a hung tab is
            // a hung tab whatever the profiler says (`SKILL` §8).
            enforce_limit(
                "cfb_chain_sectors",
                sectors_walked,
                u64::from(self.limits.max_chain_sectors),
            )?;
            let directory = self.sector(sector)?;
            for slot in 0..entries_per_sector {
                entries_seen += 1;
                enforce_limit(
                    "cfb_directory_entries",
                    entries_seen,
                    u64::from(self.limits.max_directory_entries),
                )?;
                let entry = &directory[slot * DIRECTORY_ENTRY_BYTES..][..DIRECTORY_ENTRY_BYTES];
                found.record(self.inspect_entry(entry)?);
            }
            sector = self.next_in_chain(sector)?;
        }
        if sector != END_OF_CHAIN && sector != FREE_SECTOR {
            return Err(CfbError::MalformedHeader);
        }
        Ok(found.kind())
    }

    /// Validates one 128-byte directory entry and returns its name, when it has
    /// one worth matching.
    fn inspect_entry(&self, entry: &[u8]) -> Result<Option<Name>, CfbError> {
        let object_type = entry[66];
        match object_type {
            OBJECT_TYPE_UNALLOCATED => return Ok(None),
            OBJECT_TYPE_STREAM | OBJECT_TYPE_STORAGE | OBJECT_TYPE_ROOT => {}
            _ => return Err(CfbError::MalformedDirectoryEntry),
        }
        let name_bytes = usize::from(read_u16(entry, 64)?);
        // §2.6.1: the length counts the UTF-16 terminator, so it is even, at
        // least 2, and at most 64.
        if !(2..=NAME_FIELD_BYTES).contains(&name_bytes) || name_bytes % 2 != 0 {
            return Err(CfbError::MalformedDirectoryEntry);
        }
        // The absurd `StreamSize` the design asks for. A stream cannot be longer
        // than the file that holds it, and a reader that trusted this number
        // would allocate or read from it. Refused here, in the pass that is
        // already looking at the entry, rather than by whatever reads it later.
        let stream_size = read_u64(entry, 120)?;
        if object_type == OBJECT_TYPE_STREAM && stream_size > usize_to_u64(self.bytes.len()) {
            return Err(CfbError::MalformedDirectoryEntry);
        }
        Ok(Name::decode(&entry[..name_bytes - 2], object_type))
    }
}

/// A recognised directory-entry name, as a flag rather than a string.
///
/// Comparing against a fixed set of ASCII names means nothing has to be
/// allocated or lowercased, and an attacker-controlled name never becomes a
/// `String` this crate owns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Name {
    EncryptionInfo,
    EncryptedPackage,
    WordDocument,
    Workbook,
    PowerPoint,
}

impl Name {
    /// Decodes a UTF-16LE directory-entry name far enough to match it.
    ///
    /// Only the names below matter, all of which are ASCII, so a code unit with
    /// a non-zero high byte can never match one and the decode stops at the
    /// first. Surrogate pairs, combining marks and the `\u{5}`/`\u{6}`-prefixed
    /// reserved names therefore cost nothing here.
    fn decode(utf16: &[u8], object_type: u8) -> Option<Self> {
        let mut ascii = [0_u8; NAME_FIELD_BYTES / 2];
        let mut length = 0;
        for unit in utf16.chunks_exact(2) {
            if unit[1] != 0 {
                return None;
            }
            ascii[length] = unit[0];
            length += 1;
        }
        let name = &ascii[..length];
        // Streams only, except the legacy PowerPoint name, which is also a
        // stream. A *storage* called `EncryptionInfo` is not the §2.3.4 stream
        // and claiming it was would be an overstatement by shape.
        if object_type != OBJECT_TYPE_STREAM {
            return None;
        }
        match name {
            b"EncryptionInfo" => Some(Self::EncryptionInfo),
            b"EncryptedPackage" => Some(Self::EncryptedPackage),
            b"WordDocument" => Some(Self::WordDocument),
            b"Workbook" | b"Book" => Some(Self::Workbook),
            b"PowerPoint Document" => Some(Self::PowerPoint),
            _ => None,
        }
    }
}

/// Which well-known streams the directory held.
#[derive(Default)]
struct Found {
    encryption_info: bool,
    encrypted_package: bool,
    word: bool,
    excel: bool,
    powerpoint: bool,
}

impl Found {
    fn record(&mut self, name: Option<Name>) {
        match name {
            Some(Name::EncryptionInfo) => self.encryption_info = true,
            Some(Name::EncryptedPackage) => self.encrypted_package = true,
            Some(Name::WordDocument) => self.word = true,
            Some(Name::Workbook) => self.excel = true,
            Some(Name::PowerPoint) => self.powerpoint = true,
            None => {}
        }
    }

    /// Encryption first, and only when BOTH streams are present.
    ///
    /// [MS-OFFCRYPTO] §2.3.4 requires the pair: `\EncryptionInfo` carries the
    /// parameters and `\EncryptedPackage` carries the ciphertext, and either one
    /// alone is not an encrypted package. Reporting one as encrypted would be a
    /// claim the file does not support — and the user-visible consequence would
    /// be telling someone their document is password-protected when it is
    /// something else entirely.
    fn kind(&self) -> CfbKind {
        if self.encryption_info && self.encrypted_package {
            CfbKind::EncryptedOfficePackage
        } else if self.word {
            CfbKind::LegacyWord
        } else if self.excel {
            CfbKind::LegacyExcel
        } else if self.powerpoint {
            CfbKind::LegacyPowerPoint
        } else {
            CfbKind::Unrecognised
        }
    }
}

fn read_u16(bytes: &[u8], at: usize) -> Result<u16, CfbError> {
    bytes
        .get(at..at + 2)
        .and_then(|slice| slice.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or(CfbError::MalformedHeader)
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, CfbError> {
    bytes
        .get(at..at + 4)
        .and_then(|slice| slice.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(CfbError::MalformedHeader)
}

fn read_u64(bytes: &[u8], at: usize) -> Result<u64, CfbError> {
    bytes
        .get(at..at + 8)
        .and_then(|slice| slice.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or(CfbError::MalformedHeader)
}
