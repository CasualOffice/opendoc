//! Bounded ODT `meta.xml` metadata mapping.

use casual_doc_model::v1::{AppProperties, CoreProperties, DocumentProperties};
use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::Event;

use crate::{ModelOutcome, OdfError, OdfImportLimits, RetentionOutcome};

type MetadataFinding = (String, ModelOutcome, RetentionOutcome);

/// Parses the supported ODT metadata fields and returns explicit findings for
/// metadata that has no schema-v1 representation.
pub(crate) fn parse_metadata(
    bytes: &[u8],
    limits: OdfImportLimits,
) -> Result<(DocumentProperties, Vec<MetadataFinding>), OdfError> {
    if bytes.len() > limits.max_content_bytes {
        return Err(OdfError::LimitExceeded {
            limit: "odf_metadata_bytes",
            observed: bytes.len(),
            allowed: limits.max_content_bytes,
        });
    }
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut core = CoreProperties::default();
    let mut app = AppProperties::default();
    let mut findings = Vec::new();
    let mut current: Option<(String, String)> = None;
    let mut depth = 0usize;
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|_| OdfError::MalformedContent)?
        {
            Event::Start(start) => {
                depth = depth.saturating_add(1);
                if depth > limits.max_xml_depth {
                    return Err(OdfError::LimitExceeded {
                        limit: "odf_xml_depth",
                        observed: depth,
                        allowed: limits.max_xml_depth,
                    });
                }
                let name = start.name();
                let (prefix, local) = split_name(name.as_ref());
                if local == b"meta" || local == b"document-meta" || local == b"document-statistic" {
                    // Container elements are handled structurally.
                } else if matches!(prefix, b"dc" | b"dcterms" | b"meta") {
                    current = Some((String::from_utf8_lossy(local).into_owned(), String::new()));
                }
            }
            Event::Text(text) => {
                if let Some((_, value)) = current.as_mut() {
                    value.push_str(
                        &unescape(&String::from_utf8_lossy(text.as_ref()))
                            .map_err(|_| OdfError::MalformedContent)?,
                    );
                    if value.len() > 4096 {
                        return Err(OdfError::LimitExceeded {
                            limit: "odf_metadata_text_bytes",
                            observed: value.len(),
                            allowed: 4096,
                        });
                    }
                }
            }
            Event::End(end) => {
                let name_ref = end.name();
                let (_, local) = split_name(name_ref.as_ref());
                if let Some((name, value)) = current.take() {
                    let target = match name.as_str() {
                        "title" => Some(&mut core.title),
                        "subject" => Some(&mut core.subject),
                        "description" => Some(&mut core.description),
                        "language" => Some(&mut core.language),
                        "creator" | "initial-creator" => Some(&mut core.creator),
                        "creation-date" => Some(&mut core.created),
                        "date" => Some(&mut core.modified),
                        "keyword" => Some(&mut core.keywords),
                        "generator" => Some(&mut app.application),
                        _ => None,
                    };
                    if let Some(slot) = target {
                        if slot.is_none() {
                            *slot = Some(value);
                        } else {
                            findings.push((
                                format!("odf.metadata.{name}"),
                                ModelOutcome::Degraded,
                                RetentionOutcome::NotRetained,
                            ));
                        }
                    } else if local != b"meta" {
                        findings.push((
                            format!("odf.metadata.{name}"),
                            ModelOutcome::Omitted,
                            RetentionOutcome::NotRetained,
                        ));
                    }
                }
                depth = depth.saturating_sub(1);
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok((
        DocumentProperties {
            core,
            app,
            ..DocumentProperties::default()
        },
        findings,
    ))
}

fn split_name(name: &[u8]) -> (&[u8], &[u8]) {
    name.iter()
        .position(|byte| *byte == b':')
        .map_or((b"", name), |index| (&name[..index], &name[index + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_core_and_generator_metadata() {
        let xml = br#"<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0"><office:meta><dc:title>Title</dc:title><dc:creator>Ada</dc:creator><meta:generator>OpenDoc</meta:generator></office:meta></office:document-meta>"#;
        let (properties, findings) = parse_metadata(xml, OdfImportLimits::default()).unwrap();
        assert_eq!(properties.core.title.as_deref(), Some("Title"));
        assert_eq!(properties.core.creator.as_deref(), Some("Ada"));
        assert_eq!(properties.app.application.as_deref(), Some("OpenDoc"));
        assert!(findings.is_empty());
    }
}
