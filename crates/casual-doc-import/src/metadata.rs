//! Document-property parsing: `docProps/{core,app,custom}.xml` -> the v1
//! `DocumentProperties` groups.
//!
//! Each part is streamed with the bounded XML reader (depth and element counts
//! bounded, DTDs rejected) used by the other definition parsers. Elements are
//! matched by local name (namespace-agnostic), so the `dc:`/`cp:`/`dcterms:`/
//! `vt:` prefixes are irrelevant. Text and date values are captured verbatim
//! (unescaped) so they round-trip through the writer without loss. Every
//! recognized field is mapped; an unrecognized leaf field is reported so nothing
//! is dropped silently.

use casual_doc_model::v1::{
    AppProperties, CoreProperties, CustomProperty, CustomValue, DocumentProperties, HeadingPair,
};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, BytesText, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::attribute_value;
use crate::report::Reporter;

/// The three optional `docProps` source parts.
#[derive(Default)]
pub(crate) struct DocPropsSources {
    pub core: Option<Vec<u8>>,
    pub app: Option<Vec<u8>>,
    pub custom: Option<Vec<u8>>,
}

impl DocPropsSources {
    /// Whether none of the parts were discovered.
    pub(crate) fn is_empty(&self) -> bool {
        self.core.is_none() && self.app.is_none() && self.custom.is_none()
    }
}

/// Parses whatever `docProps` parts were discovered into a `DocumentProperties`,
/// returning `None` when nothing maps to a non-empty group. Unrecognized leaf
/// fields are reported on `reporter`.
pub(crate) fn parse(
    sources: &DocPropsSources,
    config: ImportConfig,
    reporter: &mut Reporter,
) -> Result<Option<DocumentProperties>, ImportError> {
    let core = match &sources.core {
        Some(xml) => parse_core(xml, config, reporter)?,
        None => CoreProperties::default(),
    };
    let app = match &sources.app {
        Some(xml) => parse_app(xml, config, reporter)?,
        None => AppProperties::default(),
    };
    let custom = match &sources.custom {
        Some(xml) => parse_custom(xml, config)?,
        None => Vec::new(),
    };
    let properties = DocumentProperties { core, app, custom };
    Ok((!properties.is_empty()).then_some(properties))
}

/// Decodes a text event to an unescaped owned string (mirrors the body parser).
fn decode(text: &BytesText<'_>) -> Result<String, ImportError> {
    let raw = std::str::from_utf8(text.as_ref()).map_err(|_| ImportError::MalformedXml)?;
    Ok(quick_xml::escape::unescape(raw)
        .map_err(|_| ImportError::MalformedXml)?
        .into_owned())
}

fn bump(count: &mut u64, max: u64) -> Result<(), ImportError> {
    *count += 1;
    if *count > max {
        return Err(ImportError::LimitExceeded {
            limit: "xml_elements",
        });
    }
    Ok(())
}

/// Parses `docProps/core.xml`.
fn parse_core(
    xml: &[u8],
    config: ImportConfig,
    reporter: &mut Reporter,
) -> Result<CoreProperties, ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut core = CoreProperties::default();
    let mut elements = 0_u64;
    let mut depth = 0_u64;
    // The local name of the field currently being captured (a direct child of
    // the root), plus its accumulating text.
    let mut current: Option<Vec<u8>> = None;
    let mut text = String::new();

    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| ImportError::MalformedXml)?;
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err(ImportError::MalformedXml),
            Event::Start(element) => {
                depth += 1;
                if depth > config.max_depth {
                    return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                }
                bump(&mut elements, config.max_elements)?;
                if depth == 2 {
                    current = Some(element.local_name().as_ref().to_vec());
                    text.clear();
                }
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                // An empty-valued field (`<dc:title/>`) at the root level.
                if depth == 1
                    && !assign_core(&mut core, element.local_name().as_ref(), String::new())
                {
                    reporter.report(element.local_name().as_ref());
                }
            }
            Event::Text(chunk) if current.is_some() && depth == 2 => {
                text.push_str(&decode(&chunk)?);
            }
            Event::End(_) => {
                if depth == 2
                    && let Some(name) = current.take()
                    && !assign_core(&mut core, &name, std::mem::take(&mut text))
                {
                    reporter.report(&name);
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(core)
}

/// Assigns one recognized core field; returns whether the local name matched.
fn assign_core(core: &mut CoreProperties, local: &[u8], value: String) -> bool {
    let slot = match local {
        b"title" => &mut core.title,
        b"subject" => &mut core.subject,
        b"creator" => &mut core.creator,
        b"keywords" => &mut core.keywords,
        b"description" => &mut core.description,
        b"lastModifiedBy" => &mut core.last_modified_by,
        b"revision" => &mut core.revision,
        b"created" => &mut core.created,
        b"modified" => &mut core.modified,
        b"lastPrinted" => &mut core.last_printed,
        b"category" => &mut core.category,
        b"contentStatus" => &mut core.content_status,
        b"language" => &mut core.language,
        b"version" => &mut core.version,
        _ => return false,
    };
    *slot = Some(value);
    true
}

/// The nested `vt:vector` group currently open in `app.xml`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Titles,
    Headings,
}

/// Parses `docProps/app.xml`.
fn parse_app(
    xml: &[u8],
    config: ImportConfig,
    reporter: &mut Reporter,
) -> Result<AppProperties, ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut app = AppProperties::default();
    let mut elements = 0_u64;
    let mut depth = 0_u64;
    let mut section = Section::None;
    // A scalar field open at the root level, or a `vt:` leaf open inside a
    // vector section — never both at once.
    let mut scalar: Option<Vec<u8>> = None;
    let mut vt: Option<Vec<u8>> = None;
    let mut pending_heading: Option<String> = None;
    let mut text = String::new();

    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| ImportError::MalformedXml)?;
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err(ImportError::MalformedXml),
            Event::Start(element) => {
                depth += 1;
                if depth > config.max_depth {
                    return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                }
                bump(&mut elements, config.max_elements)?;
                let local = element.local_name();
                match local.as_ref() {
                    b"TitlesOfParts" => section = Section::Titles,
                    b"HeadingPairs" => {
                        section = Section::Headings;
                        pending_heading = None;
                    }
                    b"lpstr" | b"lpwstr" | b"i4" if section != Section::None => {
                        vt = Some(local.as_ref().to_vec());
                        text.clear();
                    }
                    _ if section == Section::None && depth == 2 => {
                        scalar = Some(local.as_ref().to_vec());
                        text.clear();
                    }
                    _ => {}
                }
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                let local = element.local_name();
                if section != Section::None {
                    // An empty vector leaf (`<vt:lpstr/>`).
                    commit_vt(
                        local.as_ref(),
                        String::new(),
                        section,
                        &mut app,
                        &mut pending_heading,
                    );
                } else if depth == 1 && !assign_app_scalar(&mut app, local.as_ref(), String::new())
                {
                    reporter.report(local.as_ref());
                }
            }
            Event::Text(chunk) if vt.is_some() || scalar.is_some() => {
                text.push_str(&decode(&chunk)?);
            }
            Event::End(element) => {
                let local = element.local_name();
                match local.as_ref() {
                    b"TitlesOfParts" | b"HeadingPairs" => section = Section::None,
                    _ => {
                        if vt.take().is_some() {
                            commit_vt(
                                local.as_ref(),
                                std::mem::take(&mut text),
                                section,
                                &mut app,
                                &mut pending_heading,
                            );
                        } else if depth == 2
                            && let Some(name) = scalar.take()
                            && !assign_app_scalar(&mut app, &name, std::mem::take(&mut text))
                        {
                            reporter.report(&name);
                        }
                    }
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(app)
}

/// Records a closed `vt:` leaf into the open vector section.
fn commit_vt(
    local: &[u8],
    value: String,
    section: Section,
    app: &mut AppProperties,
    pending_heading: &mut Option<String>,
) {
    match section {
        Section::Titles => {
            if matches!(local, b"lpstr" | b"lpwstr") {
                app.titles_of_parts.push(value);
            }
        }
        Section::Headings => match local {
            b"lpstr" | b"lpwstr" => *pending_heading = Some(value),
            b"i4" => {
                if let (Some(name), Ok(count)) = (pending_heading.take(), value.parse::<i32>()) {
                    app.heading_pairs.push(HeadingPair { name, count });
                }
            }
            _ => {}
        },
        Section::None => {}
    }
}

/// Assigns one recognized scalar app field; returns whether the name matched. A
/// recognized field with an unparseable numeric/boolean value is left unset (it
/// still counts as recognized, so it is not reported as unmapped).
fn assign_app_scalar(app: &mut AppProperties, local: &[u8], value: String) -> bool {
    match local {
        b"Application" => app.application = Some(value),
        b"AppVersion" => app.app_version = Some(value),
        b"Company" => app.company = Some(value),
        b"Manager" => app.manager = Some(value),
        b"Template" => app.template = Some(value),
        b"HyperlinkBase" => app.hyperlink_base = Some(value),
        b"TotalTime" => app.total_time = value.parse().ok(),
        b"Pages" => app.pages = value.parse().ok(),
        b"Words" => app.words = value.parse().ok(),
        b"Characters" => app.characters = value.parse().ok(),
        b"CharactersWithSpaces" => app.characters_with_spaces = value.parse().ok(),
        b"Lines" => app.lines = value.parse().ok(),
        b"Paragraphs" => app.paragraphs = value.parse().ok(),
        b"DocSecurity" => app.doc_security = value.parse().ok(),
        b"ScaleCrop" => app.scale_crop = Some(parse_bool(&value)),
        b"LinksUpToDate" => app.links_up_to_date = Some(parse_bool(&value)),
        b"SharedDoc" => app.shared_doc = Some(parse_bool(&value)),
        _ => return false,
    }
    true
}

/// Reads a docProps boolean: true unless an explicit falsey token.
fn parse_bool(value: &str) -> bool {
    !matches!(value.trim(), "false" | "0" | "off" | "")
}

/// Parses `docProps/custom.xml` into the ordered custom-property list. The OPC
/// `fmtid`/`pid` bookkeeping is ignored (regenerated on write).
fn parse_custom(xml: &[u8], config: ImportConfig) -> Result<Vec<CustomProperty>, ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut out = Vec::new();
    let mut elements = 0_u64;
    let mut depth = 0_u64;
    let mut pending_name: Option<String> = None;
    let mut pending_value: Option<CustomValue> = None;
    let mut vt: Option<Vec<u8>> = None;
    let mut text = String::new();

    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| ImportError::MalformedXml)?;
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err(ImportError::MalformedXml),
            Event::Start(element) => {
                depth += 1;
                if depth > config.max_depth {
                    return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                }
                bump(&mut elements, config.max_elements)?;
                if element.local_name().as_ref() == b"property" {
                    pending_name = property_name(&element);
                    pending_value = None;
                } else if pending_name.is_some() {
                    vt = Some(element.local_name().as_ref().to_vec());
                    text.clear();
                }
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                if element.local_name().as_ref() == b"property" {
                    // A value-less property is skipped (nothing to model).
                    pending_name = None;
                    pending_value = None;
                } else if pending_name.is_some() {
                    pending_value =
                        Some(custom_value(element.local_name().as_ref(), String::new()));
                }
            }
            Event::Text(chunk) if vt.is_some() => {
                text.push_str(&decode(&chunk)?);
            }
            Event::End(element) => {
                if element.local_name().as_ref() == b"property" {
                    if let (Some(name), Some(value)) = (pending_name.take(), pending_value.take())
                        && !name.is_empty()
                    {
                        out.push(CustomProperty { name, value });
                    }
                } else if let Some(kind) = vt.take() {
                    pending_value = Some(custom_value(&kind, std::mem::take(&mut text)));
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(out)
}

/// Reads a custom property's `name` attribute.
fn property_name(element: &BytesStart<'_>) -> Option<String> {
    attribute_value(element, b"name")
}

/// Maps a `vt:` leaf's local name and text to a typed custom value.
fn custom_value(local: &[u8], value: String) -> CustomValue {
    match local {
        b"lpwstr" | b"lpstr" => CustomValue::Text { value },
        b"i4" => match value.parse::<i32>() {
            Ok(number) => CustomValue::I4 { value: number },
            Err(_) => CustomValue::Other {
                kind: "i4".to_owned(),
                value,
            },
        },
        b"r8" => CustomValue::R8 { value },
        b"bool" => match value.trim() {
            "true" | "1" => CustomValue::Bool { value: true },
            "false" | "0" => CustomValue::Bool { value: false },
            _ => CustomValue::Other {
                kind: "bool".to_owned(),
                value,
            },
        },
        b"filetime" => CustomValue::FileTime { value },
        other => CustomValue::Other {
            kind: String::from_utf8_lossy(other).into_owned(),
            value,
        },
    }
}
