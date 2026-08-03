//! Admitted ZIP package, deterministic entry metadata, and bounded part reads.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use zip::{CompressionMethod, ZipArchive};

use crate::PackageError;
use crate::archive::CentralDirectory;
use crate::limits::{PackageLimits, enforce_expansion_ratio, enforce_limit, usize_to_u64};
use crate::path::normalize_package_path;

/// Compression profile accepted for a package part.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PartCompression {
    /// Bytes are stored without compression.
    Stored,
    /// Bytes use the ZIP Deflate method.
    Deflated,
}

/// Immutable metadata for one admitted package part.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageEntry {
    /// Normalized package-relative part name.
    pub part_name: String,
    /// Compressed bytes declared by ZIP metadata.
    pub compressed_bytes: u64,
    /// Expanded bytes declared by ZIP metadata.
    pub expanded_bytes: u64,
    /// Accepted compression method.
    pub compression: PartCompression,
    /// Local-file-header extra-field bytes.
    ///
    /// Container profiles such as ODF may require this to be zero for a
    /// well-known leading entry. The generic substrate records but does not
    /// assign format-specific meaning to it.
    pub local_extra_bytes: u64,
}

/// Thread-safe cancellation flag for package admission and part reads.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Requests cancellation for all clones of this token.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Returns whether cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn check(&self) -> Result<(), PackageError> {
        if self.is_cancelled() {
            Err(PackageError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Format-neutral admitted, read-only ZIP package.
#[derive(Debug)]
pub struct BoundedPackage<'a> {
    archive: ZipArchive<Cursor<&'a [u8]>>,
    entries: Vec<PackageEntry>,
    archive_indexes: BTreeMap<String, usize>,
    total_expanded_bytes: u64,
    package_bytes: u64,
}

impl<'a> BoundedPackage<'a> {
    /// Validates ZIP metadata without decompressing document parts.
    pub fn open(bytes: &'a [u8], limits: PackageLimits) -> Result<Self, PackageError> {
        Self::open_with_cancellation(bytes, limits, &CancellationToken::default())
    }

    /// Validates ZIP metadata while honoring cooperative cancellation.
    pub fn open_with_cancellation(
        bytes: &'a [u8],
        limits: PackageLimits,
        cancellation: &CancellationToken,
    ) -> Result<Self, PackageError> {
        cancellation.check()?;
        limits.validate()?;
        enforce_limit(
            "input_package_bytes",
            usize_to_u64(bytes.len()),
            usize_to_u64(limits.max_input_bytes),
        )?;

        let central = CentralDirectory::inspect(bytes, limits, cancellation)?;
        cancellation.check()?;
        let mut archive =
            ZipArchive::new(Cursor::new(bytes)).map_err(|_| PackageError::MalformedArchive)?;
        cancellation.check()?;
        if archive.len() != central.entries
            || usize::try_from(archive.central_directory_start()).ok() != Some(central.start)
        {
            return Err(PackageError::MalformedArchive);
        }
        if archive
            .has_overlapping_files()
            .map_err(|_| PackageError::MalformedArchive)?
        {
            return Err(PackageError::OverlappingEntries);
        }

        let mut entries = Vec::with_capacity(archive.len());
        let mut archive_indexes = BTreeMap::new();
        let mut total_expanded_bytes = 0_u64;
        for index in 0..archive.len() {
            cancellation.check()?;
            let file = archive
                .by_index_raw(index)
                .map_err(|_| PackageError::MalformedArchive)?;
            let normalized = normalize_package_path(file.name_raw(), limits.max_path_bytes)?;
            if file.is_dir() {
                if file.size() != 0 {
                    return Err(PackageError::MalformedArchive);
                }
                continue;
            }
            if file.is_symlink() || !file.is_file() {
                return Err(PackageError::SpecialEntry);
            }
            if file.encrypted() {
                return Err(PackageError::EncryptedEntry);
            }
            let compression = match file.compression() {
                CompressionMethod::Stored => PartCompression::Stored,
                CompressionMethod::Deflated => PartCompression::Deflated,
                _ => return Err(PackageError::UnsupportedCompression),
            };
            enforce_limit(
                "single_expanded_entry_bytes",
                file.size(),
                limits.max_single_expanded_bytes,
            )?;
            enforce_expansion_ratio(
                file.size(),
                file.compressed_size(),
                limits.max_expansion_ratio,
            )?;
            total_expanded_bytes = total_expanded_bytes.checked_add(file.size()).ok_or(
                PackageError::LimitExceeded {
                    limit: "total_expanded_bytes",
                    observed: u64::MAX,
                    allowed: limits.max_total_expanded_bytes,
                },
            )?;
            enforce_limit(
                "total_expanded_bytes",
                total_expanded_bytes,
                limits.max_total_expanded_bytes,
            )?;
            if archive_indexes.insert(normalized.clone(), index).is_some() {
                return Err(PackageError::DuplicatePart);
            }
            entries.push(PackageEntry {
                part_name: normalized,
                compressed_bytes: file.compressed_size(),
                expanded_bytes: file.size(),
                compression,
                local_extra_bytes: file
                    .extra_data()
                    .map_or(0, |bytes| usize_to_u64(bytes.len())),
            });
        }
        entries.sort_by(|left, right| left.part_name.cmp(&right.part_name));
        Ok(Self {
            archive,
            entries,
            archive_indexes,
            total_expanded_bytes,
            package_bytes: usize_to_u64(bytes.len()),
        })
    }

    /// Returns whether an admitted file part exists.
    #[must_use]
    pub fn contains_part(&self, part_name: &str) -> bool {
        self.archive_indexes.contains_key(part_name)
    }

    /// Returns an admitted part's original ZIP entry order.
    #[must_use]
    pub fn source_order(&self, part_name: &str) -> Option<usize> {
        self.archive_indexes.get(part_name).copied()
    }

    /// Returns deterministic part metadata ordered by normalized part name.
    #[must_use]
    pub fn entries(&self) -> &[PackageEntry] {
        &self.entries
    }

    /// Returns aggregate declared expanded bytes for admitted file parts.
    #[must_use]
    pub const fn total_expanded_bytes(&self) -> u64 {
        self.total_expanded_bytes
    }

    /// Returns the total byte size of the source package.
    #[must_use]
    pub const fn package_bytes(&self) -> u64 {
        self.package_bytes
    }

    /// Reads and verifies one admitted part into owned bytes.
    pub fn read_part(&mut self, part_name: &str) -> Result<Vec<u8>, PackageError> {
        self.read_part_with_cancellation(part_name, &CancellationToken::default())
    }

    /// Reads and verifies one admitted part while honoring cancellation.
    pub fn read_part_with_cancellation(
        &mut self,
        part_name: &str,
        cancellation: &CancellationToken,
    ) -> Result<Vec<u8>, PackageError> {
        cancellation.check()?;
        let index = self
            .archive_indexes
            .get(part_name)
            .copied()
            .ok_or(PackageError::PartNotFound)?;
        read_indexed(&mut self.archive, index, cancellation)
    }
}

fn read_indexed(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    index: usize,
    cancellation: &CancellationToken,
) -> Result<Vec<u8>, PackageError> {
    cancellation.check()?;
    let file = archive
        .by_index(index)
        .map_err(|_| PackageError::PartReadFailed)?;
    let declared_size = file.size();
    let capacity = usize::try_from(declared_size).map_err(|_| PackageError::PartReadFailed)?;
    let read_limit = declared_size
        .checked_add(1)
        .ok_or(PackageError::PartReadFailed)?;
    let mut bytes = Vec::with_capacity(capacity);
    let mut reader = file.take(read_limit);
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        cancellation.check()?;
        let read = reader
            .read(&mut chunk)
            .map_err(|_| PackageError::PartReadFailed)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    if usize_to_u64(bytes.len()) != declared_size {
        return Err(PackageError::PartReadFailed);
    }
    Ok(bytes)
}
