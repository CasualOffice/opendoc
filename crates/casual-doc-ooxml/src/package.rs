//! Admitted DOCX package, part manifest, and on-demand part reads.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use zip::{CompressionMethod, ZipArchive};

use crate::archive::CentralDirectory;
use crate::contenttypes::ContentTypes;
use crate::discovery::{discover_main_document, resolve_main_document_relationships};
use crate::error::PackageError;
use crate::limits::{PackageLimits, enforce_expansion_ratio, enforce_limit, usize_to_u64};
use crate::path::{is_macro_part, normalize_package_path};
use crate::relationships::DocumentRelationship;

pub(crate) const CONTENT_TYPES_PART: &str = "[Content_Types].xml";
pub(crate) const ROOT_RELATIONSHIPS_PART: &str = "_rels/.rels";

/// Compression profile accepted for a DOCX part.
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
}

/// One admitted part in the deterministic source-package manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartManifestEntry {
    /// Normalized package-relative part name.
    pub part_name: String,
    /// Declared content type, if `[Content_Types].xml` resolves one.
    pub content_type: Option<String>,
    /// Compressed bytes declared by ZIP metadata.
    pub compressed_bytes: u64,
    /// Expanded bytes declared by ZIP metadata.
    pub expanded_bytes: u64,
    /// Accepted compression method.
    pub compression: PartCompression,
}

/// A deterministic, bounded snapshot of admitted source-package facts. This is
/// the Tier-1 provenance artifact (ADR-027 D5) and a component of the future
/// import bundle; it carries no decompressed document text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourcePackageSnapshot {
    /// Admitted parts ordered by normalized part name.
    pub parts: Vec<PartManifestEntry>,
    /// Normalized part name of the discovered main document.
    pub main_document_part: String,
    /// The main document's resolved relationships, ordered by id.
    pub main_document_relationships: Vec<DocumentRelationship>,
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

/// Admitted read-only DOCX package.
#[derive(Debug)]
pub struct DocxPackage<'a> {
    archive: ZipArchive<Cursor<&'a [u8]>>,
    entries: Vec<PackageEntry>,
    archive_indexes: BTreeMap<String, usize>,
    total_expanded_bytes: u64,
    main_document_part: String,
    content_types: ContentTypes,
    main_document_relationships: Vec<DocumentRelationship>,
}

impl<'a> DocxPackage<'a> {
    /// Validates package metadata without decompressing document parts.
    pub fn open(bytes: &'a [u8], limits: PackageLimits) -> Result<Self, PackageError> {
        Self::open_with_cancellation(bytes, limits, &CancellationToken::default())
    }

    /// Validates package metadata while honoring cooperative cancellation.
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
            if is_macro_part(&normalized) {
                return Err(PackageError::MacroPart);
            }
            if archive_indexes.insert(normalized.clone(), index).is_some() {
                return Err(PackageError::DuplicatePart);
            }
            entries.push(PackageEntry {
                part_name: normalized,
                compressed_bytes: file.compressed_size(),
                expanded_bytes: file.size(),
                compression,
            });
        }

        // OPC fixes only these two well-known names. The main document is not
        // required by a conventional path; it is discovered by relationship type
        // below. (ADR-027 / R1; `word/document.xml` is only a producer
        // convention.)
        for required in [CONTENT_TYPES_PART, ROOT_RELATIONSHIPS_PART] {
            if !archive_indexes.contains_key(required) {
                return Err(PackageError::MissingRequiredPart { part: required });
            }
        }

        entries.sort_by(|left, right| left.part_name.cmp(&right.part_name));

        // Discover the main document through the package `officeDocument`
        // relationship and bind it by content type. Fail-closed: any missing,
        // ambiguous, dangling, or wrong-typed main document is a typed error.
        let content_types_bytes = read_indexed(
            &mut archive,
            archive_indexes[CONTENT_TYPES_PART],
            cancellation,
        )?;
        let content_types = ContentTypes::parse(&content_types_bytes)?;
        let relationships_bytes = read_indexed(
            &mut archive,
            archive_indexes[ROOT_RELATIONSHIPS_PART],
            cancellation,
        )?;
        let main_document_part =
            discover_main_document(&relationships_bytes, &content_types, &archive_indexes)?;
        let main_document_relationships = resolve_main_document_relationships(
            &mut archive,
            &archive_indexes,
            &main_document_part,
            cancellation,
        )?;

        Ok(Self {
            archive,
            entries,
            archive_indexes,
            total_expanded_bytes,
            main_document_part,
            content_types,
            main_document_relationships,
        })
    }

    /// Returns the normalized part name of the discovered main document.
    #[must_use]
    pub fn main_document_part(&self) -> &str {
        &self.main_document_part
    }

    /// Returns the declared content type of a normalized package part, resolving
    /// `[Content_Types].xml` overrides before extension defaults.
    #[must_use]
    pub fn content_type(&self, part_name: &str) -> Option<&str> {
        self.content_types.content_type_of(part_name)
    }

    /// Returns the main document's resolved relationships, ordered by id.
    #[must_use]
    pub fn main_document_relationships(&self) -> &[DocumentRelationship] {
        &self.main_document_relationships
    }

    /// Builds the deterministic source-package snapshot (part manifest with
    /// content types, the main document, and its relationship graph).
    #[must_use]
    pub fn source_snapshot(&self) -> SourcePackageSnapshot {
        let parts = self
            .entries
            .iter()
            .map(|entry| PartManifestEntry {
                part_name: entry.part_name.clone(),
                content_type: self
                    .content_types
                    .content_type_of(&entry.part_name)
                    .map(str::to_owned),
                compressed_bytes: entry.compressed_bytes,
                expanded_bytes: entry.expanded_bytes,
                compression: entry.compression,
            })
            .collect();
        SourcePackageSnapshot {
            parts,
            main_document_part: self.main_document_part.clone(),
            main_document_relationships: self.main_document_relationships.clone(),
        }
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

/// Reads and verifies one admitted part by archive index into owned bytes.
pub(crate) fn read_indexed(
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
