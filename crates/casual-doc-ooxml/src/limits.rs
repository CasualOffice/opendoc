//! Host-configurable package limits and their enforcement helpers.

use crate::error::PackageError;

/// Host-configurable DOCX package limits with non-bypassable hard ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageLimits {
    /// Maximum input package bytes.
    pub max_input_bytes: usize,
    /// Maximum central-directory entries, including directories.
    pub max_entries: usize,
    /// Maximum aggregate declared expanded bytes.
    pub max_total_expanded_bytes: u64,
    /// Maximum declared expanded bytes for one entry.
    pub max_single_expanded_bytes: u64,
    /// Maximum expanded-to-compressed ratio for one entry.
    pub max_expansion_ratio: u64,
    /// Maximum raw entry-name bytes.
    pub max_path_bytes: usize,
}

impl PackageLimits {
    /// Hard maximum input package bytes.
    pub const HARD_MAX_INPUT_BYTES: usize = 1024 * 1024 * 1024;
    /// Hard maximum central-directory entries.
    pub const HARD_MAX_ENTRIES: usize = 50_000;
    /// Hard maximum aggregate declared expanded bytes.
    pub const HARD_MAX_TOTAL_EXPANDED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
    /// Hard maximum declared expanded bytes for one entry.
    pub const HARD_MAX_SINGLE_EXPANDED_BYTES: u64 = 1024 * 1024 * 1024;
    /// Hard maximum per-entry expansion ratio.
    pub const HARD_MAX_EXPANSION_RATIO: u64 = 1_000;
    /// Hard maximum raw entry-name bytes.
    pub const HARD_MAX_PATH_BYTES: usize = 4_096;

    pub(crate) fn validate(self) -> Result<(), PackageError> {
        validate_limit(
            "input_package_bytes",
            usize_to_u64(self.max_input_bytes),
            usize_to_u64(Self::HARD_MAX_INPUT_BYTES),
        )?;
        validate_limit(
            "zip_entries",
            usize_to_u64(self.max_entries),
            usize_to_u64(Self::HARD_MAX_ENTRIES),
        )?;
        validate_limit(
            "total_expanded_bytes",
            self.max_total_expanded_bytes,
            Self::HARD_MAX_TOTAL_EXPANDED_BYTES,
        )?;
        validate_limit(
            "single_expanded_entry_bytes",
            self.max_single_expanded_bytes,
            Self::HARD_MAX_SINGLE_EXPANDED_BYTES,
        )?;
        validate_limit(
            "entry_expansion_ratio",
            self.max_expansion_ratio,
            Self::HARD_MAX_EXPANSION_RATIO,
        )?;
        validate_limit(
            "package_path_bytes",
            usize_to_u64(self.max_path_bytes),
            usize_to_u64(Self::HARD_MAX_PATH_BYTES),
        )
    }
}

impl Default for PackageLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 256 * 1024 * 1024,
            max_entries: 10_000,
            max_total_expanded_bytes: 1024 * 1024 * 1024,
            max_single_expanded_bytes: 256 * 1024 * 1024,
            max_expansion_ratio: 200,
            max_path_bytes: 1_024,
        }
    }
}

pub(crate) fn validate_limit(
    limit: &'static str,
    value: u64,
    hard: u64,
) -> Result<(), PackageError> {
    if value > hard {
        return Err(PackageError::InvalidLimitConfiguration {
            limit,
            value,
            hard_ceiling: hard,
        });
    }
    Ok(())
}

pub(crate) fn enforce_limit(
    limit: &'static str,
    observed: u64,
    allowed: u64,
) -> Result<(), PackageError> {
    if observed > allowed {
        return Err(PackageError::LimitExceeded {
            limit,
            observed,
            allowed,
        });
    }
    Ok(())
}

pub(crate) fn enforce_expansion_ratio(
    expanded: u64,
    compressed: u64,
    allowed: u64,
) -> Result<(), PackageError> {
    let exceeded = if compressed == 0 {
        expanded != 0
    } else {
        u128::from(expanded) > u128::from(compressed) * u128::from(allowed)
    };
    if exceeded {
        return Err(PackageError::LimitExceeded {
            limit: "entry_expansion_ratio",
            observed: expanded
                .saturating_add(compressed.saturating_sub(1))
                .checked_div(compressed)
                .unwrap_or(u64::MAX),
            allowed,
        });
    }
    Ok(())
}

pub(crate) fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
