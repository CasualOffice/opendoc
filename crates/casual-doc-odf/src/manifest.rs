//! Bounded namespace-aware ODF manifest parsing.

use std::collections::BTreeMap;

use casual_doc_package::CancellationToken;
use quick_xml::NsReader;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::{Namespace, ResolveResult};

use crate::OdfError;
use crate::package::{OdfPackageLimits, OdfVersion};

const MANIFEST_NS: &[u8] = b"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0";

/// One validated ODF manifest file entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestEntry {
    /// Package path, or `/` for the package root.
    pub full_path: String,
    /// Declared media type; empty when the producer supplied none.
    pub media_type: String,
    /// Entry-specific format version, when declared.
    pub version: Option<String>,
    /// Whether the entry has an ODF encryption-data declaration.
    pub encrypted: bool,
}

#[derive(Debug)]
pub(crate) struct Manifest {
    pub(crate) version: OdfVersion,
    pub(crate) entries: BTreeMap<String, ManifestEntry>,
}

pub(crate) fn parse_manifest(
    bytes: &[u8],
    limits: OdfPackageLimits,
    cancellation: &CancellationToken,
) -> Result<Manifest, OdfError> {
    enforce("odf_manifest_bytes", bytes.len(), limits.max_manifest_bytes)?;
    let mut reader = NsReader::from_reader(bytes);
    reader
        .resolver_mut()
        .set_max_declarations_per_element(limits.max_xml_attributes);
    let mut buffer = Vec::new();
    let mut depth = 0_usize;
    let mut elements = 0_usize;
    let mut attributes = 0_usize;
    let mut attribute_bytes = 0_usize;
    let mut root_seen = false;
    let mut root_closed = false;
    let mut version = None;
    let mut entries = BTreeMap::new();
    let mut current_entry: Option<(usize, ManifestEntry)> = None;

    loop {
        check_cancelled(cancellation)?;
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| OdfError::MalformedManifest)?;
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err(OdfError::MalformedManifest),
            Event::Start(element) => {
                depth = depth.saturating_add(1);
                enforce("odf_xml_depth", depth, limits.max_xml_depth)?;
                count_element(elements, limits.max_xml_elements).map(|value| elements = value)?;
                let is_root = !root_seen;
                if is_root {
                    if !is_manifest_name(&reader, &element, b"manifest") {
                        return Err(OdfError::MalformedManifest);
                    }
                    root_seen = true;
                    version = Some(read_version(
                        &reader,
                        &element,
                        &mut attributes,
                        &mut attribute_bytes,
                        limits,
                    )?);
                } else if root_closed {
                    return Err(OdfError::MalformedManifest);
                }

                if is_root {
                    // Root attributes were consumed by `read_version` above.
                } else if is_manifest_name(&reader, &element, b"file-entry") {
                    if current_entry.is_some() {
                        return Err(OdfError::MalformedManifest);
                    }
                    let entry = read_file_entry(
                        &reader,
                        &element,
                        &mut attributes,
                        &mut attribute_bytes,
                        limits,
                    )?;
                    current_entry = Some((depth, entry));
                } else {
                    count_attributes(
                        &reader,
                        &element,
                        &mut attributes,
                        &mut attribute_bytes,
                        limits,
                    )?;
                    if is_manifest_name(&reader, &element, b"encryption-data")
                        && let Some((_, entry)) = &mut current_entry
                    {
                        entry.encrypted = true;
                    }
                }
            }
            Event::Empty(element) => {
                count_element(elements, limits.max_xml_elements).map(|value| elements = value)?;
                if !root_seen || root_closed {
                    return Err(OdfError::MalformedManifest);
                }
                if is_manifest_name(&reader, &element, b"file-entry") {
                    let entry = read_file_entry(
                        &reader,
                        &element,
                        &mut attributes,
                        &mut attribute_bytes,
                        limits,
                    )?;
                    insert_entry(&mut entries, entry)?;
                } else {
                    count_attributes(
                        &reader,
                        &element,
                        &mut attributes,
                        &mut attribute_bytes,
                        limits,
                    )?;
                    if is_manifest_name(&reader, &element, b"encryption-data")
                        && let Some((_, entry)) = &mut current_entry
                    {
                        entry.encrypted = true;
                    }
                }
            }
            Event::End(element) => {
                if current_entry
                    .as_ref()
                    .is_some_and(|(entry_depth, _)| *entry_depth == depth)
                {
                    let (_, entry) = current_entry.take().ok_or(OdfError::MalformedManifest)?;
                    insert_entry(&mut entries, entry)?;
                }
                if depth == 1 {
                    if element.local_name().as_ref() != b"manifest" {
                        return Err(OdfError::MalformedManifest);
                    }
                    root_closed = true;
                }
                depth = depth.checked_sub(1).ok_or(OdfError::MalformedManifest)?;
            }
            Event::Text(text) if depth == 0 => {
                match text
                    .decode()
                    .map_err(|_| OdfError::MalformedManifest)?
                    .trim()
                    .is_empty()
                {
                    true => {}
                    false => return Err(OdfError::MalformedManifest),
                }
            }
            _ => {}
        }
        buffer.clear();
    }
    if !root_seen || !root_closed || depth != 0 || current_entry.is_some() {
        return Err(OdfError::MalformedManifest);
    }
    Ok(Manifest {
        version: version.ok_or(OdfError::UnsupportedVersion)?,
        entries,
    })
}

fn read_version(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    limits: OdfPackageLimits,
) -> Result<OdfVersion, OdfError> {
    let mut version = None;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedManifest)?;
        count_attribute(attribute.value.len(), attributes, attribute_bytes, limits)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        if is_bound(namespace, MANIFEST_NS) && local.as_ref() == b"version" {
            version = Some(decode_attribute(&attribute)?);
        }
    }
    OdfVersion::parse(version.as_deref().ok_or(OdfError::UnsupportedVersion)?)
}

fn read_file_entry(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    limits: OdfPackageLimits,
) -> Result<ManifestEntry, OdfError> {
    let mut full_path = None;
    let mut media_type = None;
    let mut version = None;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedManifest)?;
        count_attribute(attribute.value.len(), attributes, attribute_bytes, limits)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        if !is_bound(namespace, MANIFEST_NS) {
            continue;
        }
        match local.as_ref() {
            b"full-path" => full_path = Some(decode_attribute(&attribute)?),
            b"media-type" => media_type = Some(decode_attribute(&attribute)?),
            b"version" => version = Some(decode_attribute(&attribute)?),
            _ => {}
        }
    }
    Ok(ManifestEntry {
        full_path: full_path.ok_or(OdfError::MalformedManifest)?,
        media_type: media_type.unwrap_or_default(),
        version,
        encrypted: false,
    })
}

fn decode_attribute(
    attribute: &quick_xml::events::attributes::Attribute<'_>,
) -> Result<String, OdfError> {
    let raw =
        core::str::from_utf8(attribute.value.as_ref()).map_err(|_| OdfError::MalformedManifest)?;
    quick_xml::escape::unescape(raw)
        .map(|value| value.into_owned())
        .map_err(|_| OdfError::MalformedManifest)
}

fn count_attributes(
    _reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    limits: OdfPackageLimits,
) -> Result<(), OdfError> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedManifest)?;
        count_attribute(attribute.value.len(), attributes, attribute_bytes, limits)?;
    }
    Ok(())
}

fn count_attribute(
    bytes: usize,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    limits: OdfPackageLimits,
) -> Result<(), OdfError> {
    *attributes = attributes.saturating_add(1);
    *attribute_bytes = attribute_bytes.saturating_add(bytes);
    enforce("odf_xml_attributes", *attributes, limits.max_xml_attributes)?;
    enforce(
        "odf_xml_attribute_bytes",
        *attribute_bytes,
        limits.max_xml_attribute_bytes,
    )
}

fn count_element(current: usize, allowed: usize) -> Result<usize, OdfError> {
    let observed = current.saturating_add(1);
    enforce("odf_xml_elements", observed, allowed)?;
    Ok(observed)
}

fn insert_entry(
    entries: &mut BTreeMap<String, ManifestEntry>,
    entry: ManifestEntry,
) -> Result<(), OdfError> {
    if entry.full_path.is_empty() || entries.insert(entry.full_path.clone(), entry).is_some() {
        return Err(OdfError::ManifestMismatch);
    }
    Ok(())
}

fn is_manifest_name(reader: &NsReader<&[u8]>, element: &BytesStart<'_>, local: &[u8]) -> bool {
    let (namespace, actual_local) = reader.resolver().resolve_element(element.name());
    actual_local.as_ref() == local && is_bound(namespace, MANIFEST_NS)
}

fn is_bound(result: ResolveResult<'_>, expected: &[u8]) -> bool {
    matches!(result, ResolveResult::Bound(Namespace(actual)) if actual == expected)
}

fn check_cancelled(cancellation: &CancellationToken) -> Result<(), OdfError> {
    if cancellation.is_cancelled() {
        Err(OdfError::Cancelled)
    } else {
        Ok(())
    }
}

pub(crate) fn enforce(
    limit: &'static str,
    observed: usize,
    allowed: usize,
) -> Result<(), OdfError> {
    if observed > allowed {
        Err(OdfError::LimitExceeded {
            limit,
            observed,
            allowed,
        })
    } else {
        Ok(())
    }
}
