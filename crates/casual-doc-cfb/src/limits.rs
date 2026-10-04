//! Host-configurable compound-file limits and enforcement helpers.

use crate::CfbError;

/// Host-configurable MS-CFB limits with non-bypassable hard ceilings.
///
/// The limits that matter here are **not** the ZIP substrate's. A ZIP's hazards
/// are path traversal and compression ratio; a compound file's are a sector
/// **FAT**, which is a linked list, so the hazards are cycles, chains longer
/// than the file, and a directory that claims more entries than the input could
/// hold. Those are the four numbers below, and each one turns an unbounded read
/// into a typed refusal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CfbLimits {
    /// Maximum input container bytes.
    pub max_input_bytes: usize,
    /// Maximum directory entries enumerated, including the root and unallocated
    /// slots.
    pub max_directory_entries: u32,
    /// Maximum sectors followed along one FAT chain.
    ///
    /// This is the cycle guard: a FAT whose chain loops is an infinite read, and
    /// a ceiling is the only way to notice it without a second pass or a visited
    /// set the size of the FAT.
    pub max_chain_sectors: u32,
    /// Maximum DIFAT sectors followed.
    pub max_difat_sectors: u32,
}

impl CfbLimits {
    /// Hard maximum input container bytes.
    pub const HARD_MAX_INPUT_BYTES: usize = 1024 * 1024 * 1024;
    /// Hard maximum directory entries.
    pub const HARD_MAX_DIRECTORY_ENTRIES: u32 = 100_000;
    /// Hard maximum sectors per FAT chain.
    pub const HARD_MAX_CHAIN_SECTORS: u32 = 1_000_000;
    /// Hard maximum DIFAT sectors.
    pub const HARD_MAX_DIFAT_SECTORS: u32 = 10_000;

    pub(crate) fn validate(self) -> Result<(), CfbError> {
        validate_limit(
            "cfb_input_bytes",
            usize_to_u64(self.max_input_bytes),
            usize_to_u64(Self::HARD_MAX_INPUT_BYTES),
        )?;
        validate_limit(
            "cfb_directory_entries",
            u64::from(self.max_directory_entries),
            u64::from(Self::HARD_MAX_DIRECTORY_ENTRIES),
        )?;
        validate_limit(
            "cfb_chain_sectors",
            u64::from(self.max_chain_sectors),
            u64::from(Self::HARD_MAX_CHAIN_SECTORS),
        )?;
        validate_limit(
            "cfb_difat_sectors",
            u64::from(self.max_difat_sectors),
            u64::from(Self::HARD_MAX_DIFAT_SECTORS),
        )
    }
}

impl Default for CfbLimits {
    /// Limits sized for *recognition*, not for reading a document.
    ///
    /// They are deliberately far below the hard ceilings: the only thing phase 0
    /// does with a compound file is enumerate its directory to learn which
    /// well-known streams it holds, and that needs a few hundred entries at most.
    /// A real encrypted OOXML package has five or six.
    fn default() -> Self {
        Self {
            max_input_bytes: 256 * 1024 * 1024,
            max_directory_entries: 4_096,
            max_chain_sectors: 8_192,
            max_difat_sectors: 256,
        }
    }
}

fn validate_limit(limit: &'static str, value: u64, hard_ceiling: u64) -> Result<(), CfbError> {
    if value > hard_ceiling {
        return Err(CfbError::InvalidLimitConfiguration {
            limit,
            value,
            hard_ceiling,
        });
    }
    Ok(())
}

pub(crate) fn enforce_limit(
    limit: &'static str,
    observed: u64,
    allowed: u64,
) -> Result<(), CfbError> {
    if observed > allowed {
        return Err(CfbError::LimitExceeded {
            limit,
            observed,
            allowed,
        });
    }
    Ok(())
}

pub(crate) fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
