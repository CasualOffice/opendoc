//! Admitted DOCX package, part manifest, and on-demand part reads.

use casual_doc_package::{
    BoundedPackage, CancellationToken, PackageEntry, PackageLimits, PartCompression,
};

use crate::contenttypes::ContentTypes;
use crate::discovery::{discover_main_document, resolve_part_relationships};
use crate::error::PackageError;
use crate::path::is_macro_part;
use crate::relationships::DocumentRelationship;

pub(crate) const CONTENT_TYPES_PART: &str = "[Content_Types].xml";
pub(crate) const ROOT_RELATIONSHIPS_PART: &str = "_rels/.rels";

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

/// Admitted read-only DOCX package.
#[derive(Debug)]
pub struct DocxPackage<'a> {
    package: BoundedPackage<'a>,
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
        let mut package = BoundedPackage::open_with_cancellation(bytes, limits, cancellation)
            .map_err(PackageError::from)?;

        if package
            .entries()
            .iter()
            .any(|entry| is_macro_part(&entry.part_name))
        {
            return Err(PackageError::MacroPart);
        }

        // OPC fixes only these two well-known names. The main document is not
        // required by a conventional path; it is discovered by relationship type.
        for required in [CONTENT_TYPES_PART, ROOT_RELATIONSHIPS_PART] {
            if !package.contains_part(required) {
                return Err(PackageError::MissingRequiredPart { part: required });
            }
        }

        let content_types_bytes = package
            .read_part_with_cancellation(CONTENT_TYPES_PART, cancellation)
            .map_err(PackageError::from)?;
        let content_types = ContentTypes::parse(&content_types_bytes)?;
        let relationships_bytes = package
            .read_part_with_cancellation(ROOT_RELATIONSHIPS_PART, cancellation)
            .map_err(PackageError::from)?;
        let main_document_part =
            discover_main_document(&relationships_bytes, &content_types, &package)?;
        let main_document_relationships =
            resolve_part_relationships(&mut package, &main_document_part, cancellation)?;

        Ok(Self {
            package,
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

    /// Resolves an arbitrary admitted part's own relationships on demand, ordered
    /// by id. Targets resolve relative to the part's directory, so an extra part
    /// (header, footer, footnotes) resolves its own image and hyperlink
    /// references. A part with no `_rels` companion has no relationships.
    pub fn part_relationships(
        &mut self,
        part_name: &str,
    ) -> Result<Vec<DocumentRelationship>, PackageError> {
        self.part_relationships_with_cancellation(part_name, &CancellationToken::default())
    }

    /// Resolves an arbitrary admitted part's relationships while honoring
    /// cancellation.
    pub fn part_relationships_with_cancellation(
        &mut self,
        part_name: &str,
        cancellation: &CancellationToken,
    ) -> Result<Vec<DocumentRelationship>, PackageError> {
        resolve_part_relationships(&mut self.package, part_name, cancellation)
    }

    /// Builds the deterministic source-package snapshot (part manifest with
    /// content types, the main document, and its relationship graph).
    #[must_use]
    pub fn source_snapshot(&self) -> SourcePackageSnapshot {
        let parts = self
            .package
            .entries()
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
        self.package.entries()
    }

    /// Returns aggregate declared expanded bytes for admitted file parts.
    #[must_use]
    pub const fn total_expanded_bytes(&self) -> u64 {
        self.package.total_expanded_bytes()
    }

    /// Returns the total byte size of the source package (the input `.docx`
    /// bytes). This is an envelope/filesystem fact, not document metadata.
    #[must_use]
    pub const fn package_bytes(&self) -> u64 {
        self.package.package_bytes()
    }

    /// Reads and verifies one admitted part into owned bytes.
    pub fn read_part(&mut self, part_name: &str) -> Result<Vec<u8>, PackageError> {
        self.package
            .read_part(part_name)
            .map_err(PackageError::from)
    }

    /// Reads and verifies one admitted part while honoring cancellation.
    pub fn read_part_with_cancellation(
        &mut self,
        part_name: &str,
        cancellation: &CancellationToken,
    ) -> Result<Vec<u8>, PackageError> {
        self.package
            .read_part_with_cancellation(part_name, cancellation)
            .map_err(PackageError::from)
    }
}
