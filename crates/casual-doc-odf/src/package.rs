//! ODF/ODT package profile admission over the generic bounded ZIP substrate.

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{
    BlockNode, Break, BreakKind, DocGrid, HeaderFooter, HeaderFooterId, HeaderFooterKind,
    HeaderFooterRef, InlineNode, LineNumbering, NoteProperties, PageBorders, PageNumbering,
    PaperSource, Paragraph, ParagraphProperties, Run, RunProperties, SectionBoundary,
    SectionColumns, SectionId, Tab,
};

use crate::master_page::{HeaderFooterInline, HeaderFooterRegion, MasterPageContent};
use casual_doc_package::{
    BoundedPackage, CancellationToken, PackageEntry, PackageLimits, PartCompression,
};

use crate::manifest::{Manifest, enforce, parse_manifest};
use crate::{ManifestEntry, OdfError, OdfImportLimits, OdtImport};

/// Required ODF MIME-type part.
pub const MIMETYPE_PART: &str = "mimetype";
/// Required ODF manifest part.
pub const MANIFEST_PART: &str = "META-INF/manifest.xml";
/// Required packaged document-content part.
pub const CONTENT_PART: &str = "content.xml";
/// Optional packaged named-style definitions.
pub const STYLES_PART: &str = "styles.xml";
/// Optional packaged document metadata.
pub const META_PART: &str = "meta.xml";
/// OpenDocument Text media type.
pub const ODT_MIME: &str = "application/vnd.oasis.opendocument.text";

/// Supported ODF document/profile versions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OdfVersion {
    /// OpenDocument 1.2.
    V1_2,
    /// OpenDocument 1.3.
    V1_3,
    /// OpenDocument 1.4.
    V1_4,
}

impl OdfVersion {
    pub(crate) fn parse(value: &str) -> Result<Self, OdfError> {
        match value {
            "1.2" => Ok(Self::V1_2),
            "1.3" => Ok(Self::V1_3),
            "1.4" => Ok(Self::V1_4),
            _ => Err(OdfError::UnsupportedVersion),
        }
    }

    /// Returns the stable ODF version string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V1_2 => "1.2",
            Self::V1_3 => "1.3",
            Self::V1_4 => "1.4",
        }
    }
}

/// ODF package-profile limits layered over generic ZIP limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OdfPackageLimits {
    /// Generic ZIP package limits.
    pub package: PackageLimits,
    /// Maximum expanded `META-INF/manifest.xml` bytes.
    pub max_manifest_bytes: usize,
    /// Maximum manifest XML element nesting depth.
    pub max_xml_depth: usize,
    /// Maximum manifest XML elements.
    pub max_xml_elements: usize,
    /// Maximum manifest XML attributes.
    pub max_xml_attributes: usize,
    /// Maximum aggregate raw manifest attribute-value bytes.
    pub max_xml_attribute_bytes: usize,
}

impl OdfPackageLimits {
    /// Hard maximum manifest bytes.
    pub const HARD_MAX_MANIFEST_BYTES: usize = 64 * 1024 * 1024;
    /// Hard maximum XML depth.
    pub const HARD_MAX_XML_DEPTH: usize = 256;
    /// Hard maximum XML elements.
    pub const HARD_MAX_XML_ELEMENTS: usize = 2_000_000;
    /// Hard maximum XML attributes.
    pub const HARD_MAX_XML_ATTRIBUTES: usize = 8_000_000;
    /// Hard maximum aggregate XML attribute bytes.
    pub const HARD_MAX_XML_ATTRIBUTE_BYTES: usize = 256 * 1024 * 1024;

    fn validate(self) -> Result<(), OdfError> {
        for (limit, value, hard_ceiling) in [
            (
                "odf_manifest_bytes",
                self.max_manifest_bytes,
                Self::HARD_MAX_MANIFEST_BYTES,
            ),
            (
                "odf_xml_depth",
                self.max_xml_depth,
                Self::HARD_MAX_XML_DEPTH,
            ),
            (
                "odf_xml_elements",
                self.max_xml_elements,
                Self::HARD_MAX_XML_ELEMENTS,
            ),
            (
                "odf_xml_attributes",
                self.max_xml_attributes,
                Self::HARD_MAX_XML_ATTRIBUTES,
            ),
            (
                "odf_xml_attribute_bytes",
                self.max_xml_attribute_bytes,
                Self::HARD_MAX_XML_ATTRIBUTE_BYTES,
            ),
        ] {
            if value > hard_ceiling {
                return Err(OdfError::InvalidLimitConfiguration {
                    limit,
                    value,
                    hard_ceiling,
                });
            }
        }
        Ok(())
    }
}

impl Default for OdfPackageLimits {
    fn default() -> Self {
        Self {
            package: PackageLimits::default(),
            max_manifest_bytes: 8 * 1024 * 1024,
            max_xml_depth: 64,
            max_xml_elements: 100_000,
            max_xml_attributes: 400_000,
            max_xml_attribute_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Admitted, read-only OpenDocument Text package.
#[derive(Debug)]
pub struct OdtPackage<'a> {
    package: BoundedPackage<'a>,
    version: OdfVersion,
    manifest_entries: Vec<ManifestEntry>,
    has_signatures: bool,
}

impl<'a> OdtPackage<'a> {
    /// Admits an ODT package under explicit ODF and generic ZIP limits.
    pub fn open(bytes: &'a [u8], limits: OdfPackageLimits) -> Result<Self, OdfError> {
        Self::open_with_cancellation(bytes, limits, &CancellationToken::default())
    }

    /// Admits an ODT package while honoring cooperative cancellation.
    pub fn open_with_cancellation(
        bytes: &'a [u8],
        limits: OdfPackageLimits,
        cancellation: &CancellationToken,
    ) -> Result<Self, OdfError> {
        limits.validate()?;
        let mut package =
            BoundedPackage::open_with_cancellation(bytes, limits.package, cancellation)?;
        for required in [MIMETYPE_PART, MANIFEST_PART, CONTENT_PART] {
            if !package.contains_part(required) {
                return Err(OdfError::MissingRequiredPart { part: required });
            }
        }

        let mimetype_entry = package
            .entries()
            .iter()
            .find(|entry| entry.part_name == MIMETYPE_PART)
            .ok_or(OdfError::MissingRequiredPart {
                part: MIMETYPE_PART,
            })?;
        if package.source_order(MIMETYPE_PART) != Some(0) {
            return Err(OdfError::MimetypeNotFirst);
        }
        if mimetype_entry.compression != PartCompression::Stored {
            return Err(OdfError::MimetypeCompressed);
        }
        if mimetype_entry.local_extra_bytes != 0 {
            return Err(OdfError::MimetypeExtraField);
        }
        if package.read_part_with_cancellation(MIMETYPE_PART, cancellation)? != ODT_MIME.as_bytes()
        {
            return Err(OdfError::InvalidMimetype);
        }

        let manifest_bytes = package.read_part_with_cancellation(MANIFEST_PART, cancellation)?;
        enforce(
            "odf_manifest_bytes",
            manifest_bytes.len(),
            limits.max_manifest_bytes,
        )?;
        let manifest = parse_manifest(&manifest_bytes, limits, cancellation)?;
        validate_manifest(&package, &manifest)?;
        if manifest.entries.values().any(|entry| entry.encrypted) {
            return Err(OdfError::EncryptedDocument);
        }
        if package
            .entries()
            .iter()
            .any(|entry| is_active_content_path(&entry.part_name))
        {
            return Err(OdfError::ActiveContent);
        }

        let has_signatures = package.entries().iter().any(|entry| {
            let lower = entry.part_name.to_ascii_lowercase();
            lower.starts_with("meta-inf/") && lower.ends_with("signatures.xml")
        });
        Ok(Self {
            package,
            version: manifest.version,
            manifest_entries: manifest.entries.into_values().collect(),
            has_signatures,
        })
    }

    /// Returns the admitted ODF version.
    #[must_use]
    pub const fn version(&self) -> OdfVersion {
        self.version
    }

    /// Returns manifest entries in ascending full-path order.
    #[must_use]
    pub fn manifest_entries(&self) -> &[ManifestEntry] {
        &self.manifest_entries
    }

    /// Returns whether an ODF signature file is present.
    ///
    /// This is only a preservation fact and never a signature-validity claim.
    #[must_use]
    pub const fn has_signatures(&self) -> bool {
        self.has_signatures
    }

    /// Returns deterministic package entry metadata.
    #[must_use]
    pub fn entries(&self) -> &[PackageEntry] {
        self.package.entries()
    }

    /// Reads and verifies one admitted ODF part.
    pub fn read_part(&mut self, part_name: &str) -> Result<Vec<u8>, OdfError> {
        self.package.read_part(part_name).map_err(OdfError::from)
    }

    /// Reads and verifies one admitted ODF part while honoring cancellation.
    pub fn read_part_with_cancellation(
        &mut self,
        part_name: &str,
        cancellation: &CancellationToken,
    ) -> Result<Vec<u8>, OdfError> {
        self.package
            .read_part_with_cancellation(part_name, cancellation)
            .map_err(OdfError::from)
    }

    /// Imports the admitted ODT's `content.xml` into the normalized v1 model.
    pub fn import_document(&mut self, limits: OdfImportLimits) -> Result<OdtImport, OdfError> {
        self.import_document_with_cancellation(limits, &CancellationToken::default())
    }

    /// Imports `content.xml` while honoring cooperative cancellation.
    pub fn import_document_with_cancellation(
        &mut self,
        limits: OdfImportLimits,
        cancellation: &CancellationToken,
    ) -> Result<OdtImport, OdfError> {
        let content = self.read_part_with_cancellation(CONTENT_PART, cancellation)?;
        let styles = if self
            .package
            .entries()
            .iter()
            .any(|entry| entry.part_name == STYLES_PART)
        {
            Some(self.read_part_with_cancellation(STYLES_PART, cancellation)?)
        } else {
            None
        };
        let mut imported = crate::content::import_content_xml_with_styles_and_cancellation(
            &content,
            styles.as_deref(),
            self.version,
            limits,
            cancellation,
        )?;
        if self
            .package
            .entries()
            .iter()
            .any(|entry| entry.part_name == META_PART)
        {
            let metadata = self.read_part_with_cancellation(META_PART, cancellation)?;
            let (properties, findings) = crate::metadata::parse_metadata(&metadata, limits)?;
            if !properties.is_empty() {
                imported.document = imported
                    .document
                    .with_properties(properties)
                    .map_err(|_| OdfError::InvalidModel)?;
            }
            imported.report.entries.extend(findings.into_iter().map(
                |(feature, model_outcome, retention_outcome)| crate::CompatibilityEntry {
                    feature,
                    occurrences: 1,
                    model_outcome,
                    retention_outcome,
                },
            ));
            imported.report.entries.sort_by(|a, b| {
                a.feature
                    .cmp(&b.feature)
                    .then(a.model_outcome.cmp(&b.model_outcome))
                    .then(a.retention_outcome.cmp(&b.retention_outcome))
            });
        }
        if let Some(geometry) = styles
            .as_deref()
            .and_then(crate::page_style::parse_page_layout)
        {
            let mut seed = 0x4f44_544c_4159_4f55_u64;
            for byte in geometry
                .size
                .width_twips
                .to_le_bytes()
                .into_iter()
                .chain(geometry.size.height_twips.to_le_bytes())
            {
                seed = seed.wrapping_mul(33).wrapping_add(byte as u64);
            }
            let mut ids = IdGenerator::new(seed);
            // Mint the section id first so its byte-stable value is unaffected by
            // any header/footer content sharing this generator.
            let section_id = SectionId::new(ids.next_id().map_err(|_| OdfError::InvalidModel)?);

            // Lift the first master-page's header/footer content (bounded).
            let master = match styles.as_deref() {
                Some(bytes) => crate::master_page::parse_master_page(bytes, limits)?,
                None => MasterPageContent::default(),
            };
            let mut header_defs: Vec<(HeaderFooterId, HeaderFooter)> = Vec::new();
            let mut footer_defs: Vec<(HeaderFooterId, HeaderFooter)> = Vec::new();
            let mut header_refs: Vec<HeaderFooterRef> = Vec::new();
            let mut footer_refs: Vec<HeaderFooterRef> = Vec::new();
            let mut even_present = false;
            // Deterministic region order: default header, even header, then the
            // matching footers. Every node id is drawn from `ids` so header/footer
            // block content stays globally unique against the body.
            let regions = [
                (master.default_header, HeaderFooterKind::Default, true),
                (master.even_header, HeaderFooterKind::Even, true),
                (master.default_footer, HeaderFooterKind::Default, false),
                (master.even_footer, HeaderFooterKind::Even, false),
            ];
            for (region, kind, is_header) in regions {
                let Some(region) = region else {
                    continue;
                };
                if matches!(kind, HeaderFooterKind::Even) {
                    even_present = true;
                }
                let reference =
                    HeaderFooterId::new(ids.next_id().map_err(|_| OdfError::InvalidModel)?);
                let blocks = build_header_footer_blocks(&region, &mut ids)?;
                if is_header {
                    header_defs.push((reference, HeaderFooter { blocks }));
                    header_refs.push(HeaderFooterRef { kind, reference });
                } else {
                    footer_defs.push((reference, HeaderFooter { blocks }));
                    footer_refs.push(HeaderFooterRef { kind, reference });
                }
            }
            let added_header_footer = !header_defs.is_empty() || !footer_defs.is_empty();

            {
                let definitions = imported.document.definitions_mut();
                for (id, header_footer) in header_defs {
                    definitions.headers.insert(id, header_footer);
                }
                for (id, header_footer) in footer_defs {
                    definitions.footers.insert(id, header_footer);
                }
                if even_present {
                    definitions.settings.even_and_odd_headers = true;
                }
                definitions.sections.push(SectionBoundary {
                    id: section_id,
                    page_size: geometry.size,
                    page_margins: geometry.margins,
                    columns: SectionColumns {
                        count: geometry.columns,
                        space_twips: geometry.column_gap_twips,
                        separator: geometry.column_separator,
                        equal_width: None,
                        columns: Vec::new(),
                    },
                    headers: header_refs,
                    footers: footer_refs,
                    section_type: None,
                    title_page: None,
                    vertical_alignment: None,
                    page_numbering: PageNumbering::default(),
                    doc_grid: DocGrid::default(),
                    orientation: geometry.orientation,
                    paper_source: PaperSource::default(),
                    page_borders: PageBorders::default(),
                    line_numbering: LineNumbering::default(),
                    footnote_props: NoteProperties::default(),
                    endnote_props: NoteProperties::default(),
                    text_direction: geometry.text_direction,
                    bidi: false,
                });
            }

            if !master.findings.is_empty() {
                imported
                    .report
                    .entries
                    .extend(master.findings.into_iter().map(
                        |(feature, model_outcome, retention_outcome)| crate::CompatibilityEntry {
                            feature,
                            occurrences: 1,
                            model_outcome,
                            retention_outcome,
                        },
                    ));
                imported.report.entries.sort_by(|a, b| {
                    a.feature
                        .cmp(&b.feature)
                        .then(a.model_outcome.cmp(&b.model_outcome))
                        .then(a.retention_outcome.cmp(&b.retention_outcome))
                });
            }

            // Header/footer block content is arbitrary text, so re-validate the
            // document to keep import atomic if any model invariant is violated.
            if added_header_footer {
                imported
                    .document
                    .validate()
                    .map_err(|_| OdfError::InvalidModel)?;
            }
        }
        Ok(imported)
    }
}

/// Builds bounded header/footer paragraphs into schema-v1 blocks, drawing every
/// node id from the shared section id generator so header/footer content stays
/// globally unique against body content. Text fragments are already maximal, so
/// no two adjacent runs are equivalent.
fn build_header_footer_blocks(
    region: &HeaderFooterRegion,
    ids: &mut IdGenerator,
) -> Result<Vec<BlockNode>, OdfError> {
    let mut blocks = Vec::with_capacity(region.paragraphs.len());
    for paragraph in &region.paragraphs {
        let id = ids.next_id().map_err(|_| OdfError::InvalidModel)?;
        let mut inlines = Vec::with_capacity(paragraph.inlines.len());
        for inline in &paragraph.inlines {
            let inline_id = ids.next_id().map_err(|_| OdfError::InvalidModel)?;
            inlines.push(match inline {
                HeaderFooterInline::Text(text) => InlineNode::Run(Run {
                    id: inline_id,
                    properties: RunProperties::default(),
                    text: text.clone(),
                }),
                HeaderFooterInline::Tab => InlineNode::Tab(Tab { id: inline_id }),
                HeaderFooterInline::LineBreak => InlineNode::Break(Break {
                    id: inline_id,
                    kind: BreakKind::Line,
                }),
            });
        }
        blocks.push(BlockNode::Paragraph(Paragraph {
            id,
            properties: ParagraphProperties::default(),
            inlines,
        }));
    }
    Ok(blocks)
}

fn validate_manifest(package: &BoundedPackage<'_>, manifest: &Manifest) -> Result<(), OdfError> {
    let root = manifest
        .entries
        .get("/")
        .ok_or(OdfError::ManifestMismatch)?;
    if root.media_type != ODT_MIME {
        return Err(OdfError::ManifestMismatch);
    }
    if let Some(version) = &root.version
        && OdfVersion::parse(version)? != manifest.version
    {
        return Err(OdfError::ManifestMismatch);
    }
    for forbidden in [MIMETYPE_PART, MANIFEST_PART] {
        if manifest.entries.contains_key(forbidden) {
            return Err(OdfError::ManifestMismatch);
        }
    }
    for entry in package.entries() {
        if entry.part_name == MIMETYPE_PART || entry.part_name.starts_with("META-INF/") {
            continue;
        }
        if !manifest.entries.contains_key(&entry.part_name) {
            return Err(OdfError::ManifestMismatch);
        }
    }
    for entry in manifest.entries.values() {
        if !is_safe_manifest_path(&entry.full_path) {
            return Err(OdfError::ManifestMismatch);
        }
        if entry.full_path == "/" || entry.full_path.ends_with('/') {
            continue;
        }
        if !package.contains_part(&entry.full_path) {
            return Err(OdfError::ManifestMismatch);
        }
    }
    if !manifest.entries.contains_key(CONTENT_PART) {
        return Err(OdfError::ManifestMismatch);
    }
    if manifest
        .entries
        .values()
        .any(|entry| is_active_content_media_type(&entry.media_type))
    {
        return Err(OdfError::ActiveContent);
    }
    Ok(())
}

fn is_safe_manifest_path(path: &str) -> bool {
    if path == "/" {
        return true;
    }
    if path.is_empty() || path.starts_with('/') || path.contains(['\\', '\0']) {
        return false;
    }
    let body = path.strip_suffix('/').unwrap_or(path);
    if body.is_empty() {
        return false;
    }
    for (index, segment) in body.split('/').enumerate() {
        if segment.is_empty() || segment == "." || segment == ".." {
            return false;
        }
        if index == 0 {
            let bytes = segment.as_bytes();
            if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
                return false;
            }
        }
        if has_ambiguous_percent_encoding(segment) {
            return false;
        }
    }
    true
}

fn has_ambiguous_percent_encoding(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }
        let Some(high) = bytes.get(index + 1).and_then(|byte| hex_value(*byte)) else {
            return true;
        };
        let Some(low) = bytes.get(index + 2).and_then(|byte| hex_value(*byte)) else {
            return true;
        };
        let value = (high << 4) | low;
        if value == 0
            || value == b'/'
            || value == b'\\'
            || value == b'.'
            || value.is_ascii_alphanumeric()
            || matches!(value, b'-' | b'_' | b'~')
        {
            return true;
        }
        index += 3;
    }
    false
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_active_content_path(part_name: &str) -> bool {
    let lower = part_name.to_ascii_lowercase();
    lower.starts_with("basic/")
        || lower.starts_with("scripts/")
        || lower == "meta-inf/scripts.xml"
        || lower.ends_with("/script-lb.xml")
        || lower.ends_with("/script-lc.xml")
}

fn is_active_content_media_type(media_type: &str) -> bool {
    matches!(
        media_type.to_ascii_lowercase().as_str(),
        "application/vnd.sun.star.basic-library" | "application/x-vnd.sun.star.script"
    )
}
