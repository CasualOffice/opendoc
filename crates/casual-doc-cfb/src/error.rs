//! Typed MS-CFB admission failures.

use std::error::Error;
use std::fmt;

/// Bounded MS-CFB container admission failure.
///
/// Every variant is a refusal with a *reason*. The design this implements
/// (`docs/163` §7.2) is explicit that a compound file we cannot read must be
/// refused with something a reader can act on rather than mis-parsed as some
/// other format, and the whole point of phase 0 is that the engine can say what
/// it found.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CfbError {
    /// A host limit exceeds its non-bypassable hard ceiling.
    InvalidLimitConfiguration {
        /// Stable limit name.
        limit: &'static str,
        /// Requested value.
        value: u64,
        /// Non-bypassable maximum.
        hard_ceiling: u64,
    },
    /// Container metadata exceeds an active resource limit.
    ///
    /// This is also what a FAT cycle becomes: a sector chain that is a loop is
    /// an unbounded read, and the only sound way to notice it in one pass is a
    /// ceiling on the chain's length.
    LimitExceeded {
        /// Stable limit name.
        limit: &'static str,
        /// Observed value.
        observed: u64,
        /// Active allowed value.
        allowed: u64,
    },
    /// The first eight bytes are not the MS-CFB header signature.
    NotCompoundFile,
    /// The 512-byte header is absent, truncated, or self-inconsistent.
    MalformedHeader,
    /// The header declares a major version or sector shift outside [MS-CFB].
    UnsupportedVersion {
        /// Declared major version.
        major: u16,
        /// Declared sector shift.
        sector_shift: u16,
    },
    /// A sector the structure refers to lies past the end of the input.
    TruncatedSector {
        /// The sector number that could not be read.
        sector: u32,
    },
    /// A directory entry is not a shape [MS-CFB] §2.6.1 permits.
    MalformedDirectoryEntry,
}

impl fmt::Display for CfbError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimitConfiguration {
                limit,
                value,
                hard_ceiling,
            } => write!(
                formatter,
                "compound-file limit {limit} value {value} exceeds hard ceiling {hard_ceiling}"
            ),
            Self::LimitExceeded {
                limit,
                observed,
                allowed,
            } => write!(
                formatter,
                "compound-file limit {limit} exceeded: observed {observed}, allowed {allowed}"
            ),
            Self::NotCompoundFile => formatter.write_str("input is not an OLE compound file"),
            Self::MalformedHeader => {
                formatter.write_str("compound-file header is malformed or truncated")
            }
            Self::UnsupportedVersion {
                major,
                sector_shift,
            } => write!(
                formatter,
                "compound-file major version {major} with sector shift {sector_shift} is unsupported"
            ),
            Self::TruncatedSector { sector } => {
                write!(
                    formatter,
                    "compound-file sector {sector} is past the end of the input"
                )
            }
            Self::MalformedDirectoryEntry => {
                formatter.write_str("compound-file directory entry is malformed")
            }
        }
    }
}

impl Error for CfbError {}
