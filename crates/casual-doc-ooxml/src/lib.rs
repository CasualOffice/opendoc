//! Security-bounded DOCX package admission and on-demand part reads.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::io::{Cursor, Read};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use quick_xml::Reader;
use quick_xml::events::Event;
use zip::{CompressionMethod, ZipArchive};

const CONTENT_TYPES_PART: &str = "[Content_Types].xml";
const ROOT_RELATIONSHIPS_PART: &str = "_rels/.rels";

/// OPC `officeDocument` relationship type in the transitional namespace.
const OFFICE_DOCUMENT_REL_TRANSITIONAL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
/// OPC `officeDocument` relationship type in the ISO/IEC 29500 Strict namespace.
const OFFICE_DOCUMENT_REL_STRICT: &str =
    "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";
/// Accepted WordprocessingML main-document content types (document and template).
const MAIN_DOCUMENT_CONTENT_TYPES: [&str; 2] = [
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml",
];
/// Bound on element count for package-metadata XML (relationships, content types).
const MAX_METADATA_XML_ELEMENTS: u64 = 10_000;
/// Bound on element nesting depth for package-metadata XML.
const MAX_METADATA_XML_DEPTH: u64 = 64;

const LOCAL_FILE_SIGNATURE: &[u8; 4] = b"PK\x03\x04";
const CENTRAL_FILE_SIGNATURE: &[u8; 4] = b"PK\x01\x02";
const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
const ZIP64_EOCD_SIGNATURE: &[u8; 4] = b"PK\x06\x06";
const ZIP64_LOCATOR_SIGNATURE: &[u8; 4] = b"PK\x06\x07";

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

    fn validate(self) -> Result<(), PackageError> {
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

    fn check(&self) -> Result<(), PackageError> {
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

        Ok(Self {
            archive,
            entries,
            archive_indexes,
            total_expanded_bytes,
            main_document_part,
        })
    }

    /// Returns the normalized part name of the discovered main document.
    #[must_use]
    pub fn main_document_part(&self) -> &str {
        &self.main_document_part
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

/// One parsed OPC relationship (only the fields the reader needs).
#[derive(Debug)]
struct Relationship {
    rel_type: String,
    target: String,
    external: bool,
}

/// Parsed `[Content_Types].xml` default and override mappings.
#[derive(Debug, Default)]
struct ContentTypes {
    /// Lowercased extension to content type.
    defaults: BTreeMap<String, String>,
    /// Absolute part name (`/word/document.xml`) to content type.
    overrides: BTreeMap<String, String>,
}

impl ContentTypes {
    fn parse(bytes: &[u8]) -> Result<Self, PackageError> {
        let mut this = Self::default();
        for_each_metadata_element(bytes, CONTENT_TYPES_PART, |name, attribute| {
            match name {
                b"Default" => {
                    let mut extension = None;
                    let mut content_type = None;
                    attribute(&mut |key, value| match key {
                        b"Extension" => extension = Some(value.to_ascii_lowercase()),
                        b"ContentType" => content_type = Some(value.to_owned()),
                        _ => {}
                    })?;
                    if let (Some(extension), Some(content_type)) = (extension, content_type) {
                        this.defaults.insert(extension, content_type);
                    }
                }
                b"Override" => {
                    let mut part_name = None;
                    let mut content_type = None;
                    attribute(&mut |key, value| match key {
                        b"PartName" => part_name = Some(value.to_owned()),
                        b"ContentType" => content_type = Some(value.to_owned()),
                        _ => {}
                    })?;
                    if let (Some(part_name), Some(content_type)) = (part_name, content_type) {
                        this.overrides.insert(part_name, content_type);
                    }
                }
                _ => {}
            }
            Ok(())
        })?;
        Ok(this)
    }

    /// Resolves the content type of a normalized package part, if declared.
    fn content_type_of(&self, part_name: &str) -> Option<&str> {
        let absolute = format!("/{part_name}");
        if let Some(content_type) = self.overrides.get(&absolute) {
            return Some(content_type);
        }
        let extension = part_name.rsplit_once('.').map(|(_, ext)| ext)?;
        self.defaults
            .get(&extension.to_ascii_lowercase())
            .map(String::as_str)
    }
}

fn parse_relationships(bytes: &[u8]) -> Result<Vec<Relationship>, PackageError> {
    let mut out = Vec::new();
    for_each_metadata_element(bytes, ROOT_RELATIONSHIPS_PART, |name, attribute| {
        if name == b"Relationship" {
            let mut rel_type = None;
            let mut target = None;
            let mut external = false;
            attribute(&mut |key, value| match key {
                b"Type" => rel_type = Some(value.to_owned()),
                b"Target" => target = Some(value.to_owned()),
                b"TargetMode" => external = value.eq_ignore_ascii_case("External"),
                _ => {}
            })?;
            if let (Some(rel_type), Some(target)) = (rel_type, target) {
                out.push(Relationship {
                    rel_type,
                    target,
                    external,
                });
            }
        }
        Ok(())
    })?;
    Ok(out)
}

fn is_office_document_type(rel_type: &str) -> bool {
    rel_type == OFFICE_DOCUMENT_REL_TRANSITIONAL || rel_type == OFFICE_DOCUMENT_REL_STRICT
}

/// Resolves a root-relative OPC relationship target to a normalized part name,
/// rejecting any target that escapes the package root.
fn resolve_root_relative_target(target: &str) -> Option<String> {
    if target.is_empty() || target.contains('\\') || target.contains('\0') {
        return None;
    }
    let body = target.strip_prefix('/').unwrap_or(target);
    let mut segments: Vec<&str> = Vec::new();
    for segment in body.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }
    if segments.is_empty() {
        return None;
    }
    Some(segments.join("/"))
}

fn discover_main_document(
    relationships_bytes: &[u8],
    content_types: &ContentTypes,
    archive_indexes: &BTreeMap<String, usize>,
) -> Result<String, PackageError> {
    let relationships = parse_relationships(relationships_bytes)?;
    let office: Vec<&Relationship> = relationships
        .iter()
        .filter(|relationship| {
            !relationship.external && is_office_document_type(&relationship.rel_type)
        })
        .collect();
    match office.len() {
        0 => return Err(PackageError::MissingMainDocument),
        1 => {}
        _ => return Err(PackageError::AmbiguousMainDocument),
    }
    let resolved =
        resolve_root_relative_target(&office[0].target).ok_or(PackageError::UnsafePartName)?;
    if !archive_indexes.contains_key(&resolved) {
        return Err(PackageError::MissingMainDocument);
    }
    let content_type = content_types
        .content_type_of(&resolved)
        .ok_or(PackageError::UnsupportedMainDocumentType)?;
    if !MAIN_DOCUMENT_CONTENT_TYPES.contains(&content_type) {
        return Err(PackageError::UnsupportedMainDocumentType);
    }
    Ok(resolved)
}

/// Streams bounded, namespace-agnostic package-metadata XML, invoking `visit`
/// for each element with its local name and an attribute accessor. DTDs and any
/// non-predefined entity are rejected; depth and element counts are bounded.
fn for_each_metadata_element(
    bytes: &[u8],
    part: &'static str,
    mut visit: impl FnMut(
        &[u8],
        &mut dyn FnMut(&mut dyn FnMut(&[u8], &str)) -> Result<(), PackageError>,
    ) -> Result<(), PackageError>,
) -> Result<(), PackageError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut elements = 0_u64;
    let mut depth = 0_u64;
    let mut handle = |element: &quick_xml::events::BytesStart<'_>,
                      elements: &mut u64|
     -> Result<(), PackageError> {
        *elements += 1;
        if *elements > MAX_METADATA_XML_ELEMENTS {
            return Err(PackageError::LimitExceeded {
                limit: "metadata_xml_elements",
                observed: *elements,
                allowed: MAX_METADATA_XML_ELEMENTS,
            });
        }
        let local_name = element.local_name();
        let mut read_attributes = |sink: &mut dyn FnMut(&[u8], &str)| -> Result<(), PackageError> {
            for attribute in element.attributes() {
                let attribute =
                    attribute.map_err(|_| PackageError::MalformedPackageXml { part })?;
                // OPC relationship and content-type attribute values are plain
                // ASCII/UTF-8 with no character entities; a value that fails
                // UTF-8 or carries an unexpected entity fails closed downstream.
                let value = core::str::from_utf8(attribute.value.as_ref())
                    .map_err(|_| PackageError::MalformedPackageXml { part })?;
                sink(attribute.key.local_name().as_ref(), value);
            }
            Ok(())
        };
        visit(local_name.as_ref(), &mut read_attributes)
    };
    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| PackageError::MalformedPackageXml { part })?;
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err(PackageError::MalformedPackageXml { part }),
            Event::Start(element) => {
                depth += 1;
                if depth > MAX_METADATA_XML_DEPTH {
                    return Err(PackageError::MalformedPackageXml { part });
                }
                handle(&element, &mut elements)?;
            }
            Event::Empty(element) => {
                handle(&element, &mut elements)?;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(())
}

#[derive(Debug)]
struct CentralDirectory {
    start: usize,
    entries: usize,
}

impl CentralDirectory {
    fn inspect(
        bytes: &[u8],
        limits: PackageLimits,
        cancellation: &CancellationToken,
    ) -> Result<Self, PackageError> {
        if !bytes.starts_with(LOCAL_FILE_SIGNATURE) {
            return Err(PackageError::MalformedArchive);
        }
        let eocd = find_eocd(bytes)?;
        let disk = read_u16(bytes, eocd + 4)?;
        let central_disk = read_u16(bytes, eocd + 6)?;
        if disk != 0 || central_disk != 0 {
            return Err(PackageError::MalformedArchive);
        }

        let entries_on_disk = read_u16(bytes, eocd + 8)?;
        let entries = read_u16(bytes, eocd + 10)?;
        let central_size = read_u32(bytes, eocd + 12)?;
        let central_offset = read_u32(bytes, eocd + 16)?;
        let uses_zip64 = entries_on_disk == u16::MAX
            || entries == u16::MAX
            || central_size == u32::MAX
            || central_offset == u32::MAX;
        let (entry_count, central_size, central_offset, central_end) = if uses_zip64 {
            read_zip64_directory(bytes, eocd)?
        } else {
            if entries_on_disk != entries {
                return Err(PackageError::MalformedArchive);
            }
            (
                u64::from(entries),
                u64::from(central_size),
                u64::from(central_offset),
                eocd,
            )
        };

        enforce_limit("zip_entries", entry_count, usize_to_u64(limits.max_entries))?;
        let entries = usize::try_from(entry_count).map_err(|_| PackageError::MalformedArchive)?;
        let start = usize::try_from(central_offset).map_err(|_| PackageError::MalformedArchive)?;
        let size = usize::try_from(central_size).map_err(|_| PackageError::MalformedArchive)?;
        let end = start
            .checked_add(size)
            .ok_or(PackageError::MalformedArchive)?;
        if end != central_end || end > bytes.len() {
            return Err(PackageError::MalformedArchive);
        }

        let mut cursor = start;
        let mut names = BTreeSet::new();
        for _ in 0..entries {
            cancellation.check()?;
            let fixed_end = cursor
                .checked_add(46)
                .ok_or(PackageError::MalformedArchive)?;
            if fixed_end > end || bytes.get(cursor..cursor + 4) != Some(CENTRAL_FILE_SIGNATURE) {
                return Err(PackageError::MalformedArchive);
            }
            let flags = read_u16(bytes, cursor + 8)?;
            if flags & 1 != 0 {
                return Err(PackageError::EncryptedEntry);
            }
            match read_u16(bytes, cursor + 10)? {
                0 | 8 => {}
                _ => return Err(PackageError::UnsupportedCompression),
            }
            let name_length = usize::from(read_u16(bytes, cursor + 28)?);
            let extra_length = usize::from(read_u16(bytes, cursor + 30)?);
            let comment_length = usize::from(read_u16(bytes, cursor + 32)?);
            let record_end = fixed_end
                .checked_add(name_length)
                .and_then(|value| value.checked_add(extra_length))
                .and_then(|value| value.checked_add(comment_length))
                .ok_or(PackageError::MalformedArchive)?;
            if record_end > end {
                return Err(PackageError::MalformedArchive);
            }
            let name = normalize_package_path(
                &bytes[fixed_end..fixed_end + name_length],
                limits.max_path_bytes,
            )?;
            if !names.insert(name) {
                return Err(PackageError::DuplicatePart);
            }
            cursor = record_end;
        }
        if cursor != end {
            return Err(PackageError::MalformedArchive);
        }
        Ok(Self { start, entries })
    }
}

fn find_eocd(bytes: &[u8]) -> Result<usize, PackageError> {
    if bytes.len() < 22 {
        return Err(PackageError::MalformedArchive);
    }
    let minimum = bytes.len().saturating_sub(22 + usize::from(u16::MAX));
    for position in (minimum..=bytes.len() - 22).rev() {
        if bytes.get(position..position + 4) != Some(EOCD_SIGNATURE) {
            continue;
        }
        let comment_length = usize::from(read_u16(bytes, position + 20)?);
        if position
            .checked_add(22)
            .and_then(|value| value.checked_add(comment_length))
            == Some(bytes.len())
        {
            return Ok(position);
        }
    }
    Err(PackageError::MalformedArchive)
}

fn read_zip64_directory(bytes: &[u8], eocd: usize) -> Result<(u64, u64, u64, usize), PackageError> {
    let locator = eocd.checked_sub(20).ok_or(PackageError::MalformedArchive)?;
    if bytes.get(locator..locator + 4) != Some(ZIP64_LOCATOR_SIGNATURE)
        || read_u32(bytes, locator + 4)? != 0
        || read_u32(bytes, locator + 16)? != 1
    {
        return Err(PackageError::MalformedArchive);
    }
    let zip64_position = usize::try_from(read_u64(bytes, locator + 8)?)
        .map_err(|_| PackageError::MalformedArchive)?;
    if zip64_position > bytes.len().saturating_sub(56) {
        return Err(PackageError::MalformedArchive);
    }
    if bytes.get(zip64_position..zip64_position + 4) != Some(ZIP64_EOCD_SIGNATURE) {
        return Err(PackageError::MalformedArchive);
    }
    let record_size = usize::try_from(read_u64(bytes, zip64_position + 4)?)
        .map_err(|_| PackageError::MalformedArchive)?;
    if zip64_position
        .checked_add(12)
        .and_then(|value| value.checked_add(record_size))
        != Some(locator)
        || record_size < 44
        || read_u32(bytes, zip64_position + 16)? != 0
        || read_u32(bytes, zip64_position + 20)? != 0
    {
        return Err(PackageError::MalformedArchive);
    }
    let entries_on_disk = read_u64(bytes, zip64_position + 24)?;
    let entries = read_u64(bytes, zip64_position + 32)?;
    if entries_on_disk != entries {
        return Err(PackageError::MalformedArchive);
    }
    Ok((
        entries,
        read_u64(bytes, zip64_position + 40)?,
        read_u64(bytes, zip64_position + 48)?,
        zip64_position,
    ))
}

fn normalize_package_path(raw: &[u8], max_path_bytes: usize) -> Result<String, PackageError> {
    enforce_limit(
        "package_path_bytes",
        usize_to_u64(raw.len()),
        usize_to_u64(max_path_bytes),
    )?;
    let path = std::str::from_utf8(raw).map_err(|_| PackageError::UnsafePartName)?;
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return Err(PackageError::UnsafePartName);
    }
    let is_directory = path.ends_with('/');
    let body = path.strip_suffix('/').unwrap_or(path);
    if body.is_empty() {
        return Err(PackageError::UnsafePartName);
    }

    let mut normalized = String::with_capacity(path.len());
    for (index, segment) in body.split('/').enumerate() {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(PackageError::UnsafePartName);
        }
        if index == 0 {
            let bytes = segment.as_bytes();
            if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
                return Err(PackageError::UnsafePartName);
            }
        }
        if index != 0 {
            normalized.push('/');
        }
        normalize_segment(segment, &mut normalized)?;
    }
    if is_directory {
        normalized.push('/');
    }
    Ok(normalized)
}

fn normalize_segment(segment: &str, output: &mut String) -> Result<(), PackageError> {
    let bytes = segment.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = *bytes.get(index + 1).ok_or(PackageError::UnsafePartName)?;
            let low = *bytes.get(index + 2).ok_or(PackageError::UnsafePartName)?;
            let value = hex_value(high)
                .and_then(|high| hex_value(low).map(|low| (high << 4) | low))
                .ok_or(PackageError::UnsafePartName)?;
            if value == 0
                || value == b'/'
                || value == b'\\'
                || value == b'.'
                || is_ascii_unreserved(value)
            {
                return Err(PackageError::UnsafePartName);
            }
            output.push('%');
            output.push(hex_digit(value >> 4));
            output.push(hex_digit(value & 0x0f));
            index += 3;
        } else {
            let character = segment[index..]
                .chars()
                .next()
                .ok_or(PackageError::UnsafePartName)?;
            output.push(character);
            index += character.len_utf8();
        }
    }
    Ok(())
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

const fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + value - 10) as char,
    }
}

const fn is_ascii_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn is_macro_part(part_name: &str) -> bool {
    let lower = part_name.to_ascii_lowercase();
    lower.ends_with("/vbaproject.bin")
        || lower.ends_with("/vbaprojectsignature.bin")
        || lower.ends_with("/vbadata.xml")
}

fn enforce_expansion_ratio(
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

fn validate_limit(limit: &'static str, value: u64, hard: u64) -> Result<(), PackageError> {
    if value > hard {
        return Err(PackageError::InvalidLimitConfiguration {
            limit,
            value,
            hard_ceiling: hard,
        });
    }
    Ok(())
}

fn enforce_limit(limit: &'static str, observed: u64, allowed: u64) -> Result<(), PackageError> {
    if observed > allowed {
        return Err(PackageError::LimitExceeded {
            limit,
            observed,
            allowed,
        });
    }
    Ok(())
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, PackageError> {
    let end = offset
        .checked_add(2)
        .ok_or(PackageError::MalformedArchive)?;
    let value = bytes
        .get(offset..end)
        .ok_or(PackageError::MalformedArchive)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, PackageError> {
    let end = offset
        .checked_add(4)
        .ok_or(PackageError::MalformedArchive)?;
    let value = bytes
        .get(offset..end)
        .ok_or(PackageError::MalformedArchive)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, PackageError> {
    let end = offset
        .checked_add(8)
        .ok_or(PackageError::MalformedArchive)?;
    let value = bytes
        .get(offset..end)
        .ok_or(PackageError::MalformedArchive)?;
    Ok(u64::from_le_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}

/// DOCX package admission or part-read failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageError {
    /// A host limit exceeds its non-bypassable hard ceiling.
    InvalidLimitConfiguration {
        /// Stable limit name.
        limit: &'static str,
        /// Requested value.
        value: u64,
        /// Non-bypassable maximum.
        hard_ceiling: u64,
    },
    /// Package metadata exceeds an active resource limit.
    LimitExceeded {
        /// Stable limit name.
        limit: &'static str,
        /// Observed value.
        observed: u64,
        /// Active allowed value.
        allowed: u64,
    },
    /// ZIP records are malformed or inconsistent.
    MalformedArchive,
    /// Package work was cooperatively cancelled.
    Cancelled,
    /// A package path is unsafe or outside the accepted profile.
    UnsafePartName,
    /// Two records resolve to the same normalized package part.
    DuplicatePart,
    /// An encrypted ZIP entry is unsupported.
    EncryptedEntry,
    /// A ZIP entry uses a compression method outside the DOCX profile.
    UnsupportedCompression,
    /// Compressed data ranges overlap.
    OverlappingEntries,
    /// A symbolic link or other special entry is unsupported.
    SpecialEntry,
    /// A macro project part is unsupported.
    MacroPart,
    /// A minimal DOCX package part is missing.
    MissingRequiredPart {
        /// Required static part name.
        part: &'static str,
    },
    /// Package-metadata XML (relationships or content types) is malformed.
    MalformedPackageXml {
        /// Static part name of the offending metadata part.
        part: &'static str,
    },
    /// No `officeDocument` relationship resolves to an admitted main document.
    MissingMainDocument,
    /// More than one `officeDocument` relationship is present.
    AmbiguousMainDocument,
    /// The discovered main document does not carry a WordprocessingML type.
    UnsupportedMainDocumentType,
    /// A requested admitted part does not exist.
    PartNotFound,
    /// A part could not be fully decompressed and verified.
    PartReadFailed,
}

impl fmt::Display for PackageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimitConfiguration {
                limit,
                value,
                hard_ceiling,
            } => write!(
                formatter,
                "package limit {limit} value {value} exceeds hard ceiling {hard_ceiling}"
            ),
            Self::LimitExceeded {
                limit,
                observed,
                allowed,
            } => write!(
                formatter,
                "package limit {limit} exceeded: observed {observed}, allowed {allowed}"
            ),
            Self::MalformedArchive => formatter.write_str("DOCX ZIP structure is malformed"),
            Self::Cancelled => formatter.write_str("DOCX package operation was cancelled"),
            Self::UnsafePartName => formatter.write_str("DOCX package part name is unsafe"),
            Self::DuplicatePart => formatter.write_str("DOCX package contains a duplicate part"),
            Self::EncryptedEntry => formatter.write_str("encrypted DOCX entries are unsupported"),
            Self::UnsupportedCompression => {
                formatter.write_str("DOCX entry compression method is unsupported")
            }
            Self::OverlappingEntries => formatter.write_str("DOCX ZIP entry data ranges overlap"),
            Self::SpecialEntry => {
                formatter.write_str("DOCX package contains a special filesystem entry")
            }
            Self::MacroPart => formatter.write_str("DOCX macro project parts are unsupported"),
            Self::MissingRequiredPart { part } => {
                write!(formatter, "DOCX package is missing required part {part}")
            }
            Self::MalformedPackageXml { part } => {
                write!(formatter, "DOCX package metadata part {part} is malformed")
            }
            Self::MissingMainDocument => {
                formatter.write_str("DOCX package has no resolvable main document relationship")
            }
            Self::AmbiguousMainDocument => {
                formatter.write_str("DOCX package declares more than one main document")
            }
            Self::UnsupportedMainDocumentType => {
                formatter.write_str("DOCX main document content type is unsupported")
            }
            Self::PartNotFound => formatter.write_str("DOCX package part was not found"),
            Self::PartReadFailed => {
                formatter.write_str("DOCX package part could not be fully verified")
            }
        }
    }
}

impl Error for PackageError {}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;

    const DOCUMENT_PART: &str = "word/document.xml";
    const CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    const ROOT_RELATIONSHIPS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    const DOCUMENT: &[u8] = br#"<?xml version="1.0"?><w:document/>"#;
    const MIXED_UNICODE_DOCUMENT: &str = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
        "<w:body><w:p><w:r><w:t xml:space=\"preserve\">",
        "Cafe\u{0301} | \u{0939}\u{093f}\u{0928}\u{094d}\u{0926}\u{0940} | ",
        "\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064a}\u{0629} | ",
        "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}\u{200d}\u{1f466}",
        "</w:t></w:r></w:p></w:body></w:document>",
    );
    const UNKNOWN_SAFE_PART: &[u8] =
        br#"<custom xmlns="urn:opendoc:fixture"><value>preserve-me</value></custom>"#;

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

    fn minimal_entries() -> [(&'static str, &'static [u8], CompressionMethod); 3] {
        [
            (DOCUMENT_PART, DOCUMENT, CompressionMethod::Deflated),
            (CONTENT_TYPES_PART, CONTENT_TYPES, CompressionMethod::Stored),
            (
                ROOT_RELATIONSHIPS_PART,
                ROOT_RELATIONSHIPS,
                CompressionMethod::Stored,
            ),
        ]
    }

    fn minimal_package() -> Vec<u8> {
        package(&minimal_entries())
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
    fn minimal_package_metadata_and_part_reads_are_deterministic() {
        let bytes = minimal_package();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();

        assert_eq!(
            package
                .entries()
                .iter()
                .map(|entry| entry.part_name.as_str())
                .collect::<Vec<_>>(),
            vec![CONTENT_TYPES_PART, ROOT_RELATIONSHIPS_PART, DOCUMENT_PART]
        );
        assert_eq!(package.read_part(DOCUMENT_PART).unwrap(), DOCUMENT);
        assert_eq!(
            package.read_part("word/missing.xml"),
            Err(PackageError::PartNotFound)
        );
        assert_eq!(
            package.total_expanded_bytes(),
            usize_to_u64(CONTENT_TYPES.len() + ROOT_RELATIONSHIPS.len() + DOCUMENT.len())
        );
    }

    #[test]
    fn every_package_limit_accepts_its_boundary_and_rejects_above_it() {
        let bytes = minimal_package();
        let opened = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let total = opened.total_expanded_bytes();
        let single = opened
            .entries()
            .iter()
            .map(|entry| entry.expanded_bytes)
            .max()
            .unwrap();
        let path = minimal_entries()
            .iter()
            .map(|(name, _, _)| name.len())
            .max()
            .unwrap();

        let boundary = PackageLimits {
            max_input_bytes: bytes.len(),
            max_entries: 3,
            max_total_expanded_bytes: total,
            max_single_expanded_bytes: single,
            max_expansion_ratio: 1,
            max_path_bytes: path,
        };
        DocxPackage::open(&bytes, boundary).unwrap();

        let cases = [
            PackageLimits {
                max_input_bytes: bytes.len() - 1,
                ..boundary
            },
            PackageLimits {
                max_entries: 2,
                ..boundary
            },
            PackageLimits {
                max_total_expanded_bytes: total - 1,
                ..boundary
            },
            PackageLimits {
                max_single_expanded_bytes: single - 1,
                ..boundary
            },
            PackageLimits {
                max_expansion_ratio: 0,
                ..boundary
            },
            PackageLimits {
                max_path_bytes: path - 1,
                ..boundary
            },
        ];
        for limits in cases {
            assert!(matches!(
                DocxPackage::open(&bytes, limits),
                Err(PackageError::LimitExceeded { .. })
            ));
        }

        let invalid_configurations = [
            (
                "input_package_bytes",
                PackageLimits {
                    max_input_bytes: usize::MAX,
                    ..PackageLimits::default()
                },
            ),
            (
                "zip_entries",
                PackageLimits {
                    max_entries: usize::MAX,
                    ..PackageLimits::default()
                },
            ),
            (
                "total_expanded_bytes",
                PackageLimits {
                    max_total_expanded_bytes: u64::MAX,
                    ..PackageLimits::default()
                },
            ),
            (
                "single_expanded_entry_bytes",
                PackageLimits {
                    max_single_expanded_bytes: u64::MAX,
                    ..PackageLimits::default()
                },
            ),
            (
                "entry_expansion_ratio",
                PackageLimits {
                    max_expansion_ratio: u64::MAX,
                    ..PackageLimits::default()
                },
            ),
            (
                "package_path_bytes",
                PackageLimits {
                    max_path_bytes: usize::MAX,
                    ..PackageLimits::default()
                },
            ),
        ];
        for (expected_limit, invalid) in invalid_configurations {
            assert!(matches!(
                DocxPackage::open(&bytes, invalid),
                Err(PackageError::InvalidLimitConfiguration { limit, .. })
                    if limit == expected_limit
            ));
        }
    }

    #[test]
    fn traversal_ambiguous_escapes_and_duplicates_are_rejected() {
        for unsafe_name in [
            "../evil.xml",
            "/absolute.xml",
            "C:/drive.xml",
            "word\\evil.xml",
            "word/%2e%2e/evil.xml",
            "word/%64ocument.xml",
        ] {
            let mut entries = minimal_entries().to_vec();
            entries.push((unsafe_name, b"unsafe", CompressionMethod::Stored));
            assert_eq!(
                DocxPackage::open(&package(&entries), PackageLimits::default()).unwrap_err(),
                PackageError::UnsafePartName
            );
        }

        let mut entries = minimal_entries().to_vec();
        entries.push(("word/documenx.xml", b"duplicate", CompressionMethod::Stored));
        let mut duplicate = package(&entries);
        let central = central_record_positions(&duplicate);
        let duplicate_central = central[3];
        let duplicate_local =
            usize::try_from(read_u32(&duplicate, duplicate_central + 42).unwrap()).unwrap();
        duplicate[duplicate_central + 46..duplicate_central + 46 + DOCUMENT_PART.len()]
            .copy_from_slice(DOCUMENT_PART.as_bytes());
        duplicate[duplicate_local + 30..duplicate_local + 30 + DOCUMENT_PART.len()]
            .copy_from_slice(DOCUMENT_PART.as_bytes());
        assert_eq!(
            DocxPackage::open(&duplicate, PackageLimits::default()).unwrap_err(),
            PackageError::DuplicatePart
        );
    }

    #[test]
    fn high_expansion_malformed_missing_and_macro_packages_are_rejected() {
        let expanded = vec![b'A'; 64 * 1024];
        let entries = [
            (
                DOCUMENT_PART,
                expanded.as_slice(),
                CompressionMethod::Deflated,
            ),
            (CONTENT_TYPES_PART, CONTENT_TYPES, CompressionMethod::Stored),
            (
                ROOT_RELATIONSHIPS_PART,
                ROOT_RELATIONSHIPS,
                CompressionMethod::Stored,
            ),
        ];
        let limits = PackageLimits {
            max_expansion_ratio: 2,
            ..PackageLimits::default()
        };
        assert!(matches!(
            DocxPackage::open(&package(&entries), limits),
            Err(PackageError::LimitExceeded {
                limit: "entry_expansion_ratio",
                ..
            })
        ));
        assert_eq!(
            DocxPackage::open(b"not a zip", PackageLimits::default()).unwrap_err(),
            PackageError::MalformedArchive
        );
        assert!(matches!(
            DocxPackage::open(&package(&minimal_entries()[..2]), PackageLimits::default()),
            Err(PackageError::MissingRequiredPart { .. })
        ));

        let mut macro_entries = minimal_entries().to_vec();
        macro_entries.push(("word/vbaProject.bin", b"macro", CompressionMethod::Stored));
        assert_eq!(
            DocxPackage::open(&package(&macro_entries), PackageLimits::default()).unwrap_err(),
            PackageError::MacroPart
        );
    }

    #[test]
    fn encrypted_unsupported_symlink_and_overlapping_entries_are_rejected() {
        let original = minimal_package();
        let central = central_record_positions(&original);

        let mut encrypted = original.clone();
        encrypted[6] |= 1;
        encrypted[central[0] + 8] |= 1;
        assert_eq!(
            DocxPackage::open(&encrypted, PackageLimits::default()).unwrap_err(),
            PackageError::EncryptedEntry
        );

        let mut unsupported = original.clone();
        unsupported[8..10].copy_from_slice(&12_u16.to_le_bytes());
        unsupported[central[0] + 10..central[0] + 12].copy_from_slice(&12_u16.to_le_bytes());
        assert_eq!(
            DocxPackage::open(&unsupported, PackageLimits::default()).unwrap_err(),
            PackageError::UnsupportedCompression
        );

        let mut symlink = original.clone();
        symlink[central[0] + 5] = 3;
        symlink[central[0] + 38..central[0] + 42]
            .copy_from_slice(&(0o120777_u32 << 16).to_le_bytes());
        assert_eq!(
            DocxPackage::open(&symlink, PackageLimits::default()).unwrap_err(),
            PackageError::SpecialEntry
        );

        let mut overlapping = original;
        let first_offset = overlapping[central[0] + 42..central[0] + 46].to_vec();
        overlapping[central[1] + 42..central[1] + 46].copy_from_slice(&first_offset);
        assert_eq!(
            DocxPackage::open(&overlapping, PackageLimits::default()).unwrap_err(),
            PackageError::OverlappingEntries
        );
    }

    #[test]
    fn committed_package_fixtures_match_manifest_outcomes() {
        let minimal = include_bytes!("../../../fixtures/generated/minimal-valid.docx");
        let mut package = DocxPackage::open(minimal, PackageLimits::default()).unwrap();
        assert_eq!(package.read_part(DOCUMENT_PART).unwrap(), DOCUMENT);

        let mixed_unicode = include_bytes!("../../../fixtures/generated/mixed-unicode.docx");
        let mut package = DocxPackage::open(mixed_unicode, PackageLimits::default()).unwrap();
        assert_eq!(
            package.read_part(DOCUMENT_PART).unwrap(),
            MIXED_UNICODE_DOCUMENT.as_bytes()
        );

        let unknown_safe = include_bytes!("../../../fixtures/generated/unknown-safe-part.docx");
        let mut package = DocxPackage::open(unknown_safe, PackageLimits::default()).unwrap();
        assert!(
            package
                .entries()
                .iter()
                .any(|entry| entry.part_name == "customXml/item1.xml")
        );
        assert_eq!(
            package.read_part("customXml/item1.xml").unwrap(),
            UNKNOWN_SAFE_PART
        );

        let traversal = include_bytes!("../../../fixtures/generated/path-traversal.docx");
        assert_eq!(
            DocxPackage::open(traversal, PackageLimits::default()).unwrap_err(),
            PackageError::UnsafePartName
        );

        let high_expansion = include_bytes!("../../../fixtures/generated/high-expansion.docx");
        assert!(matches!(
            DocxPackage::open(
                high_expansion,
                PackageLimits {
                    max_expansion_ratio: 2,
                    ..PackageLimits::default()
                }
            ),
            Err(PackageError::LimitExceeded {
                limit: "entry_expansion_ratio",
                ..
            })
        ));

        let duplicate = include_bytes!("../../../fixtures/generated/duplicate-part.docx");
        assert_eq!(
            DocxPackage::open(duplicate, PackageLimits::default()).unwrap_err(),
            PackageError::DuplicatePart
        );

        let malformed = include_bytes!("../../../fixtures/generated/malformed-truncated.docx");
        assert_eq!(
            DocxPackage::open(malformed, PackageLimits::default()).unwrap_err(),
            PackageError::MalformedArchive
        );
    }

    #[test]
    fn cancellation_returns_no_package_or_partial_part() {
        let bytes = minimal_package();
        let cancellation = CancellationToken::default();
        cancellation.cancel();
        assert_eq!(
            DocxPackage::open_with_cancellation(&bytes, PackageLimits::default(), &cancellation)
                .unwrap_err(),
            PackageError::Cancelled
        );

        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        assert_eq!(
            package
                .read_part_with_cancellation(DOCUMENT_PART, &cancellation)
                .unwrap_err(),
            PackageError::Cancelled
        );
    }

    #[test]
    fn corrupt_part_data_returns_no_partial_bytes_or_document_text() {
        let mut bytes = minimal_package();
        let data_start = {
            let mut archive = ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
            usize::try_from(archive.by_index_raw(0).unwrap().data_start()).unwrap()
        };
        bytes[data_start] ^= 0xff;

        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let error = package.read_part(DOCUMENT_PART).unwrap_err();
        assert_eq!(error, PackageError::PartReadFailed);
        assert!(!error.to_string().contains("<w:document"));

        let malformed = PackageError::MalformedArchive.to_string();
        assert!(!malformed.contains("secret-document-text"));
    }

    const CONTENT_TYPES_HEAD: &str = concat!(
        "<?xml version=\"1.0\"?>",
        "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
        "<Default Extension=\"rels\" ",
        "ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
    );
    const MAIN_TYPE: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

    fn content_types_for(part: &str, content_type: &str) -> Vec<u8> {
        format!(
            "{CONTENT_TYPES_HEAD}<Override PartName=\"/{part}\" ContentType=\"{content_type}\"/></Types>"
        )
        .into_bytes()
    }

    fn relationships(inner: &str) -> Vec<u8> {
        format!(
            "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{inner}</Relationships>"
        )
        .into_bytes()
    }

    fn office_document_relationship(rel_type: &str, target: &str, external: bool) -> String {
        let mode = if external {
            " TargetMode=\"External\""
        } else {
            ""
        };
        format!("<Relationship Id=\"rId1\" Type=\"{rel_type}\" Target=\"{target}\"{mode}/>")
    }

    fn discovery_package(content_types: &[u8], rels: &[u8], main_part: &str) -> Vec<u8> {
        package(&[
            (CONTENT_TYPES_PART, content_types, CompressionMethod::Stored),
            (ROOT_RELATIONSHIPS_PART, rels, CompressionMethod::Stored),
            (main_part, DOCUMENT, CompressionMethod::Deflated),
        ])
    }

    #[test]
    fn main_document_is_discovered_at_a_non_conventional_path_without_word_document_xml() {
        let content_types = content_types_for("word/primary.xml", MAIN_TYPE);
        let rels = relationships(&office_document_relationship(
            OFFICE_DOCUMENT_REL_TRANSITIONAL,
            "word/primary.xml",
            false,
        ));
        let bytes = discovery_package(&content_types, &rels, "word/primary.xml");
        let package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        assert_eq!(package.main_document_part(), "word/primary.xml");
        assert!(
            package
                .entries()
                .iter()
                .all(|entry| entry.part_name != DOCUMENT_PART)
        );
    }

    #[test]
    fn strict_and_transitional_office_document_relationships_are_both_discovered() {
        for rel_type in [OFFICE_DOCUMENT_REL_TRANSITIONAL, OFFICE_DOCUMENT_REL_STRICT] {
            let content_types = content_types_for(DOCUMENT_PART, MAIN_TYPE);
            let rels = relationships(&office_document_relationship(
                rel_type,
                DOCUMENT_PART,
                false,
            ));
            let bytes = discovery_package(&content_types, &rels, DOCUMENT_PART);
            let package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
            assert_eq!(package.main_document_part(), DOCUMENT_PART);
        }
    }

    #[test]
    fn missing_ambiguous_external_and_mistyped_main_documents_are_rejected() {
        let content_types = content_types_for(DOCUMENT_PART, MAIN_TYPE);

        let empty = relationships("");
        assert_eq!(
            DocxPackage::open(
                &discovery_package(&content_types, &empty, DOCUMENT_PART),
                PackageLimits::default()
            )
            .unwrap_err(),
            PackageError::MissingMainDocument
        );

        let mut two =
            office_document_relationship(OFFICE_DOCUMENT_REL_TRANSITIONAL, DOCUMENT_PART, false);
        two.push_str(
            "<Relationship Id=\"rId2\" \
             Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" \
             Target=\"word/other.xml\"/>",
        );
        assert_eq!(
            DocxPackage::open(
                &discovery_package(&content_types, &relationships(&two), DOCUMENT_PART),
                PackageLimits::default()
            )
            .unwrap_err(),
            PackageError::AmbiguousMainDocument
        );

        let external = relationships(&office_document_relationship(
            OFFICE_DOCUMENT_REL_TRANSITIONAL,
            "https://example.invalid/document.xml",
            true,
        ));
        assert_eq!(
            DocxPackage::open(
                &discovery_package(&content_types, &external, DOCUMENT_PART),
                PackageLimits::default()
            )
            .unwrap_err(),
            PackageError::MissingMainDocument
        );

        let wrong_type = content_types_for(DOCUMENT_PART, "application/xml");
        let rels = relationships(&office_document_relationship(
            OFFICE_DOCUMENT_REL_TRANSITIONAL,
            DOCUMENT_PART,
            false,
        ));
        assert_eq!(
            DocxPackage::open(
                &discovery_package(&wrong_type, &rels, DOCUMENT_PART),
                PackageLimits::default()
            )
            .unwrap_err(),
            PackageError::UnsupportedMainDocumentType
        );
    }

    #[test]
    fn main_document_relationship_target_escaping_the_root_is_rejected() {
        let content_types = content_types_for(DOCUMENT_PART, MAIN_TYPE);
        let rels = relationships(&office_document_relationship(
            OFFICE_DOCUMENT_REL_TRANSITIONAL,
            "../escape.xml",
            false,
        ));
        assert_eq!(
            DocxPackage::open(
                &discovery_package(&content_types, &rels, DOCUMENT_PART),
                PackageLimits::default()
            )
            .unwrap_err(),
            PackageError::UnsafePartName
        );
    }

    #[test]
    fn malformed_relationships_xml_is_rejected_without_leaking_text() {
        let content_types = content_types_for(DOCUMENT_PART, MAIN_TYPE);
        let malformed = b"<Relationships><Relationship secret-token".to_vec();
        let error = DocxPackage::open(
            &discovery_package(&content_types, &malformed, DOCUMENT_PART),
            PackageLimits::default(),
        )
        .unwrap_err();
        assert_eq!(
            error,
            PackageError::MalformedPackageXml {
                part: ROOT_RELATIONSHIPS_PART
            }
        );
        assert!(!error.to_string().contains("secret-token"));
    }
}
