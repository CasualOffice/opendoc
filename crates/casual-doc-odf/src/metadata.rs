//! Bounded ODT `meta.xml` metadata mapping.

use std::collections::BTreeSet;

use casual_doc_model::v1::{
    AppProperties, CoreProperties, CustomProperty, CustomValue, DocumentProperties,
};
use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::{BytesStart, Event};

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
    let mut custom = Vec::new();
    let mut custom_names = BTreeSet::new();
    let mut elements = 0usize;
    let mut attributes = 0usize;
    let mut attribute_bytes = 0usize;
    let mut findings = Vec::new();
    let mut current: Option<(String, String)> = None;
    let mut custom_name: Option<(String, String)> = None;
    let mut depth = 0usize;
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|_| OdfError::MalformedContent)?
        {
            Event::Start(start) => {
                elements = elements.saturating_add(1);
                if elements > limits.max_xml_elements {
                    return Err(OdfError::LimitExceeded {
                        limit: "odf_metadata_xml_elements",
                        observed: elements,
                        allowed: limits.max_xml_elements,
                    });
                }
                for attr in start.attributes().flatten() {
                    attributes = attributes.saturating_add(1);
                    attribute_bytes = attribute_bytes.saturating_add(attr.value.len());
                }
                if attributes > limits.max_xml_attributes {
                    return Err(OdfError::LimitExceeded {
                        limit: "odf_metadata_xml_attributes",
                        observed: attributes,
                        allowed: limits.max_xml_attributes,
                    });
                }
                if attribute_bytes > limits.max_xml_attribute_bytes {
                    return Err(OdfError::LimitExceeded {
                        limit: "odf_metadata_xml_attribute_bytes",
                        observed: attribute_bytes,
                        allowed: limits.max_xml_attribute_bytes,
                    });
                }
                depth = depth.saturating_add(1);
                if depth > limits.max_xml_depth {
                    return Err(OdfError::LimitExceeded {
                        limit: "odf_xml_depth",
                        observed: depth,
                        allowed: limits.max_xml_depth,
                    });
                }
                let name = start.name();
                let (_, local) = split_name(name.as_ref());
                if local == b"document-statistic" {
                    read_statistics(&start, &mut app, &mut findings);
                }
                if local == b"user-defined" {
                    let mut name = None;
                    let mut kind = String::from("string");
                    for attr in start.attributes().flatten() {
                        let (_, attr_name) = split_name(attr.key.as_ref());
                        if attr_name == b"name" {
                            name = Some(String::from_utf8_lossy(attr.value.as_ref()).into_owned());
                        } else if attr_name == b"value-type" {
                            kind = String::from_utf8_lossy(attr.value.as_ref()).into_owned();
                        }
                    }
                    custom_name = name.map(|name| (name, kind));
                }
                if local == b"meta" || local == b"document-meta" || local == b"document-statistic" {
                    // Container elements are handled structurally.
                } else if matches!(
                    local,
                    b"title"
                        | b"subject"
                        | b"description"
                        | b"language"
                        | b"creator"
                        | b"initial-creator"
                        | b"creation-date"
                        | b"date"
                        | b"keyword"
                        | b"generator"
                        | b"editing-duration"
                        | b"editing-cycles"
                        | b"user-defined"
                ) {
                    current = Some((String::from_utf8_lossy(local).into_owned(), String::new()));
                }
            }
            Event::Empty(empty) => {
                elements = elements.saturating_add(1);
                if elements > limits.max_xml_elements {
                    return Err(OdfError::LimitExceeded {
                        limit: "odf_metadata_xml_elements",
                        observed: elements,
                        allowed: limits.max_xml_elements,
                    });
                }
                for attr in empty.attributes().flatten() {
                    attributes = attributes.saturating_add(1);
                    attribute_bytes = attribute_bytes.saturating_add(attr.value.len());
                }
                if attributes > limits.max_xml_attributes {
                    return Err(OdfError::LimitExceeded {
                        limit: "odf_metadata_xml_attributes",
                        observed: attributes,
                        allowed: limits.max_xml_attributes,
                    });
                }
                if attribute_bytes > limits.max_xml_attribute_bytes {
                    return Err(OdfError::LimitExceeded {
                        limit: "odf_metadata_xml_attribute_bytes",
                        observed: attribute_bytes,
                        allowed: limits.max_xml_attribute_bytes,
                    });
                }
                let name = empty.name();
                let (_, local) = split_name(name.as_ref());
                if local == b"document-statistic" {
                    read_statistics(&empty, &mut app, &mut findings);
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
                    if name == "editing-duration" {
                        match parse_duration_minutes(&value) {
                            Some(minutes) => app.total_time = Some(minutes),
                            None => findings.push((
                                "odf.metadata.editing-duration".to_owned(),
                                ModelOutcome::Degraded,
                                RetentionOutcome::NotRetained,
                            )),
                        }
                        depth = depth.saturating_sub(1);
                        continue;
                    }
                    if name == "editing-cycles" {
                        findings.push((
                            "odf.metadata.editing-cycles".to_owned(),
                            ModelOutcome::Omitted,
                            RetentionOutcome::NotRetained,
                        ));
                        depth = depth.saturating_sub(1);
                        continue;
                    }
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
                    } else if name == "user-defined" {
                        if let Some((name, kind)) = custom_name.take() {
                            if !name.is_empty() {
                                if custom_names.insert(name.clone()) {
                                    custom.push(CustomProperty {
                                        name,
                                        value: parse_custom_value(&kind, value, &mut findings),
                                    });
                                } else {
                                    findings.push((
                                        "odf.metadata.user-defined.duplicate".to_owned(),
                                        ModelOutcome::Degraded,
                                        RetentionOutcome::NotRetained,
                                    ));
                                }
                            } else {
                                findings.push((
                                    "odf.metadata.user-defined".to_owned(),
                                    ModelOutcome::Omitted,
                                    RetentionOutcome::NotRetained,
                                ));
                            }
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
    Ok((DocumentProperties { core, app, custom }, findings))
}

fn parse_duration_minutes(value: &str) -> Option<i64> {
    let body = value.strip_prefix("PT")?;
    let mut number = String::new();
    let mut total = 0i64;
    for character in body.chars() {
        if character.is_ascii_digit() {
            number.push(character);
            continue;
        }
        let parsed = number.parse::<i64>().ok()?;
        number.clear();
        total = total.checked_add(match character {
            'H' => parsed.checked_mul(60)?,
            'M' => parsed,
            'S' => parsed / 60,
            _ => return None,
        })?;
    }
    if number.is_empty() { Some(total) } else { None }
}

fn parse_custom_value(
    kind: &str,
    value: String,
    findings: &mut Vec<MetadataFinding>,
) -> CustomValue {
    match kind {
        "float" | "percentage" | "currency" => CustomValue::R8 { value },
        "boolean" => match value.trim() {
            "true" | "1" => CustomValue::Bool { value: true },
            "false" | "0" => CustomValue::Bool { value: false },
            _ => {
                findings.push((
                    "odf.metadata.user-defined.boolean".to_owned(),
                    ModelOutcome::Degraded,
                    RetentionOutcome::NotRetained,
                ));
                CustomValue::Text { value }
            }
        },
        "date" | "time" => CustomValue::FileTime { value },
        "long" | "short" | "int" => match value.trim().parse::<i32>() {
            Ok(value) => CustomValue::I4 { value },
            Err(_) => {
                findings.push((
                    "odf.metadata.user-defined.integer".to_owned(),
                    ModelOutcome::Degraded,
                    RetentionOutcome::NotRetained,
                ));
                CustomValue::Text { value }
            }
        },
        _ => CustomValue::Text { value },
    }
}

fn read_statistics(
    start: &BytesStart<'_>,
    app: &mut AppProperties,
    findings: &mut Vec<MetadataFinding>,
) {
    for attr in start.attributes().flatten() {
        let (_, attr_name) = split_name(attr.key.as_ref());
        let value = String::from_utf8_lossy(attr.value.as_ref())
            .parse::<i64>()
            .ok();
        let target = match attr_name {
            b"page-count" => Some(&mut app.pages),
            b"word-count" => Some(&mut app.words),
            b"character-count" => Some(&mut app.characters),
            b"paragraph-count" => Some(&mut app.paragraphs),
            _ => None,
        };
        if let Some(slot) = target {
            if let Some(value) = value {
                *slot = Some(value);
            } else {
                findings.push((
                    "odf.metadata.document-statistic".to_owned(),
                    ModelOutcome::Omitted,
                    RetentionOutcome::NotRetained,
                ));
            }
        } else {
            findings.push((
                format!(
                    "odf.metadata.document-statistic.{}",
                    String::from_utf8_lossy(attr_name)
                ),
                ModelOutcome::Omitted,
                RetentionOutcome::NotRetained,
            ));
        }
    }
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

    #[test]
    fn maps_statistics_and_user_defined_values() {
        let xml = br#"<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0"><office:meta><meta:document-statistic meta:page-count="3" meta:word-count="12"/><meta:user-defined meta:name="Build">ci</meta:user-defined></office:meta></office:document-meta>"#;
        let (properties, findings) = parse_metadata(xml, OdfImportLimits::default()).unwrap();
        assert_eq!(properties.app.pages, Some(3));
        assert_eq!(properties.app.words, Some(12));
        assert_eq!(properties.custom.len(), 1);
        assert!(findings.is_empty());
    }

    #[test]
    fn maps_typed_user_defined_values() {
        let xml = br#"<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0"><office:meta><meta:user-defined meta:name="Count" meta:value-type="long">7</meta:user-defined><meta:user-defined meta:name="Ready" meta:value-type="boolean">true</meta:user-defined></office:meta></office:document-meta>"#;
        let (properties, findings) = parse_metadata(xml, OdfImportLimits::default()).unwrap();
        assert!(findings.is_empty());
        assert!(matches!(
            properties.custom[0].value,
            CustomValue::I4 { value: 7 }
        ));
        assert!(matches!(
            properties.custom[1].value,
            CustomValue::Bool { value: true }
        ));
    }

    #[test]
    fn metadata_limits_and_duplicate_names_are_explicit() {
        let xml = br#"<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0"><office:meta><meta:user-defined meta:name="x">a</meta:user-defined><meta:user-defined meta:name="x">b</meta:user-defined></office:meta></office:document-meta>"#;
        let limits = OdfImportLimits {
            max_xml_elements: 1,
            ..OdfImportLimits::default()
        };
        assert!(parse_metadata(xml, limits).is_err());
        let (properties, findings) = parse_metadata(xml, OdfImportLimits::default()).unwrap();
        assert_eq!(properties.custom.len(), 1);
        assert!(
            findings
                .iter()
                .any(|(feature, _, _)| feature.ends_with("duplicate"))
        );
    }

    #[test]
    fn maps_editing_duration_to_minutes() {
        let xml = br#"<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0"><office:meta><meta:editing-duration>PT1H30M</meta:editing-duration></office:meta></office:document-meta>"#;
        let (properties, findings) = parse_metadata(xml, OdfImportLimits::default()).unwrap();
        assert_eq!(properties.app.total_time, Some(90));
        assert!(findings.is_empty());
    }

    #[test]
    fn metadata_mapping_is_independent_of_namespace_prefixes() {
        let xml = br#"<o:document-meta xmlns:o="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:d="http://purl.org/dc/elements/1.1/"><o:meta><d:title>Title</d:title></o:meta></o:document-meta>"#;
        let (properties, findings) = parse_metadata(xml, OdfImportLimits::default()).unwrap();
        assert_eq!(properties.core.title.as_deref(), Some("Title"));
        assert!(findings.is_empty());
    }
}
