//! Semantic WordprocessingML import into the normalized schema v1 model.
//!
//! This first slice maps the main document body — paragraphs, runs, text,
//! explicit tabs and breaks, and direct run properties (bold, italic,
//! underline, strike, size, RGB color) — into a deterministic `v1::Document`.
//! Every traversed construct that is not modeled is recorded in a bounded,
//! deterministic compatibility report under the dual-axis disposition taxonomy
//! (`35-DISPOSITION-TAXONOMY.md`); nothing is dropped silently. Styles,
//! numbering, sections, tables (as structure), media, fields, and tracked
//! changes are reported, not yet modeled.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use casual_doc_model::v1::{
    BlockNode, Break, BreakKind, Color, Definitions, Document, InlineNode, Paragraph,
    ParagraphProperties, RgbColor, Run, RunProperties, Tab,
};
use casual_doc_model::{IdGenerator, ModelError, NodeId};
use casual_doc_ooxml::{DocxPackage, PackageError};
use quick_xml::Reader;
use quick_xml::events::Event;

/// Host-configurable import options with bounded, non-bypassable ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImportConfig {
    /// Non-zero namespace used to derive deterministic model IDs.
    pub id_namespace: u64,
    /// Maximum XML elements traversed.
    pub max_elements: u64,
    /// Maximum XML nesting depth.
    pub max_depth: u64,
    /// Maximum aggregate text bytes mapped into runs.
    pub max_text_bytes: usize,
}

impl ImportConfig {
    const HARD_MAX_ELEMENTS: u64 = 50_000_000;
    const HARD_MAX_DEPTH: u64 = 4_096;
    const HARD_MAX_TEXT_BYTES: usize = 256 * 1024 * 1024;

    fn validate(self) -> Result<(), ImportError> {
        if self.id_namespace == 0
            || self.max_elements > Self::HARD_MAX_ELEMENTS
            || self.max_depth > Self::HARD_MAX_DEPTH
            || self.max_text_bytes > Self::HARD_MAX_TEXT_BYTES
        {
            return Err(ImportError::InvalidConfig);
        }
        Ok(())
    }
}

impl Default for ImportConfig {
    fn default() -> Self {
        Self {
            id_namespace: 1,
            max_elements: 5_000_000,
            max_depth: 512,
            max_text_bytes: 64 * 1024 * 1024,
        }
    }
}

/// How a construct was represented in the model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelOutcome {
    /// Fully represented.
    Mapped,
    /// Partially represented.
    Degraded,
    /// Not represented.
    Omitted,
}

/// What happened to source detail the model did not consume.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetentionOutcome {
    /// Retained in a validated preservation record.
    Preserved,
    /// Intentionally and reportably dropped (no record).
    NotRetained,
    /// Retention refused by policy.
    Blocked,
    /// Structurally invalid or over-limit.
    Rejected,
    /// No unconsumed remainder.
    NotApplicable,
}

/// One compatibility-report entry, aggregated by feature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilityEntry {
    /// Feature (WordprocessingML local element name).
    pub feature: String,
    /// Bounded occurrence count.
    pub occurrences: u32,
    /// Model outcome.
    pub model_outcome: ModelOutcome,
    /// Retention outcome.
    pub retention_outcome: RetentionOutcome,
}

/// A deterministic compatibility report ordered by feature name.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompatibilityReport {
    /// Entries ordered by feature name.
    pub entries: Vec<CompatibilityEntry>,
}

/// The result of importing a main document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Import {
    /// The normalized v1 document.
    pub document: Document,
    /// The compatibility report.
    pub report: CompatibilityReport,
}

/// A WordprocessingML import failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImportError {
    /// The import configuration exceeded a hard ceiling.
    InvalidConfig,
    /// The package could not provide the main document part.
    Package(PackageError),
    /// The main document XML was malformed or DTD-bearing.
    MalformedXml,
    /// A configured import bound was exceeded.
    LimitExceeded {
        /// Stable limit name.
        limit: &'static str,
    },
    /// The constructed model violated a v1 invariant.
    Model(ModelError),
}

impl fmt::Display for ImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig => {
                formatter.write_str("import configuration exceeds a hard ceiling")
            }
            Self::Package(error) => write!(formatter, "package error: {error}"),
            Self::MalformedXml => formatter.write_str("main document XML is malformed"),
            Self::LimitExceeded { limit } => {
                write!(formatter, "import limit {limit} exceeded")
            }
            Self::Model(error) => write!(formatter, "imported model is invalid: {error}"),
        }
    }
}

impl Error for ImportError {}

/// Imports the main document of an admitted DOCX package into a v1 document.
pub fn import_package(
    package: &mut DocxPackage<'_>,
    config: ImportConfig,
) -> Result<Import, ImportError> {
    let main_part = package.main_document_part().to_owned();
    let bytes = package
        .read_part(&main_part)
        .map_err(ImportError::Package)?;
    import_main_document_xml(&bytes, config)
}

/// Imports main-document WordprocessingML bytes into a v1 document.
pub fn import_main_document_xml(xml: &[u8], config: ImportConfig) -> Result<Import, ImportError> {
    config.validate()?;
    let mut builder = Builder::new(config);
    builder.run(xml)?;
    builder.finish()
}

/// A run/tab/break segment before ids and normalization are assigned.
enum Segment {
    Run {
        properties: RunProperties,
        text: String,
    },
    Tab,
    Break(BreakKind),
}

const HANDLED: &[&[u8]] = &[
    b"document",
    b"body",
    b"p",
    b"r",
    b"rPr",
    b"t",
    b"tab",
    b"br",
    b"b",
    b"i",
    b"u",
    b"strike",
    b"sz",
    b"color",
];

struct Builder {
    ids: IdGenerator,
    max_elements: u64,
    max_depth: u64,
    max_text_bytes: usize,
    elements: u64,
    depth: u64,
    text_bytes: usize,
    document_id: Option<NodeId>,
    in_body: bool,
    paragraph_open: bool,
    paragraph_id: Option<NodeId>,
    segments: Vec<Segment>,
    run_open: bool,
    run_properties: RunProperties,
    in_run_properties: bool,
    in_text: bool,
    text_buffer: String,
    paragraphs: Vec<Paragraph>,
    unsupported: BTreeMap<String, u32>,
}

impl Builder {
    fn new(config: ImportConfig) -> Self {
        Self {
            ids: IdGenerator::new(config.id_namespace),
            max_elements: config.max_elements,
            max_depth: config.max_depth,
            max_text_bytes: config.max_text_bytes,
            elements: 0,
            depth: 0,
            text_bytes: 0,
            document_id: None,
            in_body: false,
            paragraph_open: false,
            paragraph_id: None,
            segments: Vec::new(),
            run_open: false,
            run_properties: RunProperties::default(),
            in_run_properties: false,
            in_text: false,
            text_buffer: String::new(),
            paragraphs: Vec::new(),
            unsupported: BTreeMap::new(),
        }
    }

    fn next_id(&mut self) -> Result<NodeId, ImportError> {
        self.ids
            .next_id()
            .map_err(|_| ImportError::LimitExceeded { limit: "node_ids" })
    }

    fn run(&mut self, xml: &[u8]) -> Result<(), ImportError> {
        let document_id = self.next_id()?;
        self.document_id = Some(document_id);

        let mut reader = Reader::from_reader(xml);
        let mut buffer = Vec::new();
        loop {
            let event = reader
                .read_event_into(&mut buffer)
                .map_err(|_| ImportError::MalformedXml)?;
            match event {
                Event::Eof => break,
                Event::DocType(_) => return Err(ImportError::MalformedXml),
                Event::Start(element) => {
                    self.depth += 1;
                    if self.depth > self.max_depth {
                        return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                    }
                    self.on_start(element.local_name().as_ref(), &element)?;
                }
                Event::Empty(element) => {
                    self.on_start(element.local_name().as_ref(), &element)?;
                    self.on_end(element.local_name().as_ref())?;
                }
                Event::End(element) => {
                    self.on_end(element.local_name().as_ref())?;
                    self.depth = self.depth.saturating_sub(1);
                }
                Event::Text(text) if self.in_text => {
                    let raw = text.into_inner();
                    let raw =
                        std::str::from_utf8(raw.as_ref()).map_err(|_| ImportError::MalformedXml)?;
                    let decoded =
                        quick_xml::escape::unescape(raw).map_err(|_| ImportError::MalformedXml)?;
                    self.text_bytes = self.text_bytes.saturating_add(decoded.len());
                    if self.text_bytes > self.max_text_bytes {
                        return Err(ImportError::LimitExceeded {
                            limit: "text_bytes",
                        });
                    }
                    self.text_buffer.push_str(decoded.as_ref());
                }
                _ => {}
            }
            buffer.clear();
        }
        Ok(())
    }

    fn on_start(
        &mut self,
        local: &[u8],
        element: &quick_xml::events::BytesStart<'_>,
    ) -> Result<(), ImportError> {
        self.elements += 1;
        if self.elements > self.max_elements {
            return Err(ImportError::LimitExceeded {
                limit: "xml_elements",
            });
        }
        if self.in_body && !HANDLED.contains(&local) {
            self.report(local);
        }
        match local {
            b"body" => self.in_body = true,
            b"p" if self.in_body => {
                self.paragraph_open = true;
                self.paragraph_id = Some(self.next_id()?);
                self.segments.clear();
                self.run_open = false;
            }
            b"r" if self.paragraph_open => {
                self.run_open = true;
                self.run_properties = RunProperties::default();
            }
            b"rPr" if self.run_open => self.in_run_properties = true,
            b"t" if self.run_open => {
                self.in_text = true;
                self.text_buffer.clear();
            }
            b"tab" if self.run_open => self.segments.push(Segment::Tab),
            b"br" if self.run_open => {
                self.segments.push(Segment::Break(break_kind(element)));
            }
            _ if self.in_run_properties => self.apply_run_property(local, element),
            _ => {}
        }
        Ok(())
    }

    fn on_end(&mut self, local: &[u8]) -> Result<(), ImportError> {
        match local {
            b"body" => self.in_body = false,
            b"p" if self.paragraph_open => self.finish_paragraph()?,
            b"r" => self.run_open = false,
            b"rPr" => self.in_run_properties = false,
            b"t" if self.in_text => {
                self.in_text = false;
                let text = std::mem::take(&mut self.text_buffer);
                if !text.is_empty() {
                    self.segments.push(Segment::Run {
                        properties: self.run_properties.clone(),
                        text,
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_run_property(&mut self, local: &[u8], element: &quick_xml::events::BytesStart<'_>) {
        let value = attribute_value(element, b"val");
        match local {
            b"b" => self.run_properties.bold = Some(is_true(value.as_deref())),
            b"i" => self.run_properties.italic = Some(is_true(value.as_deref())),
            b"u" => {
                self.run_properties.underline = Some(value.as_deref() != Some("none"));
            }
            b"strike" => self.run_properties.strike = Some(is_true(value.as_deref())),
            b"sz" => {
                if let Some(size) = value.as_deref().and_then(|value| value.parse::<u32>().ok()) {
                    self.run_properties.size_half_points = Some(size);
                }
            }
            b"color" => {
                if let Some(rgb) = value.as_deref().and_then(parse_rgb) {
                    self.run_properties.color = Some(Color::Rgb(rgb));
                }
            }
            _ => {}
        }
    }

    fn finish_paragraph(&mut self) -> Result<(), ImportError> {
        self.paragraph_open = false;
        let paragraph_id = self
            .paragraph_id
            .take()
            .expect("paragraph id was allocated");
        let segments = std::mem::take(&mut self.segments);
        let normalized = normalize_segments(segments);
        let mut inlines = Vec::with_capacity(normalized.len());
        for segment in normalized {
            let id = self.next_id()?;
            inlines.push(match segment {
                Segment::Run { properties, text } => InlineNode::Run(Run {
                    id,
                    properties,
                    text,
                }),
                Segment::Tab => InlineNode::Tab(Tab { id }),
                Segment::Break(kind) => InlineNode::Break(Break { id, kind }),
            });
        }
        self.paragraphs.push(Paragraph {
            id: paragraph_id,
            properties: ParagraphProperties::default(),
            inlines,
        });
        Ok(())
    }

    fn report(&mut self, local: &[u8]) {
        let feature = String::from_utf8_lossy(local).into_owned();
        let counter = self.unsupported.entry(feature).or_insert(0);
        *counter = counter.saturating_add(1);
    }

    fn finish(mut self) -> Result<Import, ImportError> {
        let document_id = self.document_id.expect("document id was allocated");
        if self.paragraphs.is_empty() {
            // A body with no paragraphs yields a single empty paragraph so the
            // v1 document has a non-empty body.
            let id = self.next_id()?;
            self.paragraphs.push(Paragraph {
                id,
                properties: ParagraphProperties::default(),
                inlines: Vec::new(),
            });
        }
        let body = self
            .paragraphs
            .into_iter()
            .map(BlockNode::Paragraph)
            .collect();
        let document =
            Document::new(document_id, body, Definitions::default()).map_err(ImportError::Model)?;

        let entries = self
            .unsupported
            .into_iter()
            .map(|(feature, occurrences)| CompatibilityEntry {
                feature,
                occurrences,
                model_outcome: ModelOutcome::Omitted,
                retention_outcome: RetentionOutcome::NotRetained,
            })
            .collect();
        Ok(Import {
            document,
            report: CompatibilityReport { entries },
        })
    }
}

fn normalize_segments(segments: Vec<Segment>) -> Vec<Segment> {
    let mut normalized: Vec<Segment> = Vec::with_capacity(segments.len());
    for segment in segments {
        match segment {
            Segment::Run { text, .. } if text.is_empty() => {}
            Segment::Run { properties, text } => {
                if let Some(Segment::Run {
                    properties: previous_properties,
                    text: previous_text,
                }) = normalized.last_mut()
                {
                    if *previous_properties == properties {
                        previous_text.push_str(&text);
                        continue;
                    }
                }
                normalized.push(Segment::Run { properties, text });
            }
            other => normalized.push(other),
        }
    }
    normalized
}

fn break_kind(element: &quick_xml::events::BytesStart<'_>) -> BreakKind {
    match attribute_value(element, b"type").as_deref() {
        Some("page") => BreakKind::Page,
        Some("column") => BreakKind::Column,
        _ => BreakKind::Line,
    }
}

fn attribute_value(element: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
    for attribute in element.attributes() {
        let attribute = attribute.ok()?;
        if attribute.key.local_name().as_ref() == name {
            return std::str::from_utf8(attribute.value.as_ref())
                .ok()
                .map(str::to_owned);
        }
    }
    None
}

fn is_true(value: Option<&str>) -> bool {
    !matches!(value, Some("0") | Some("false") | Some("off"))
}

fn parse_rgb(value: &str) -> Option<RgbColor> {
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&value[range], 16).ok();
    Some(RgbColor {
        r: channel(0..2)?,
        g: channel(2..4)?,
        b: channel(4..6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import(xml: &[u8]) -> Import {
        import_main_document_xml(xml, ImportConfig::default()).unwrap()
    }

    fn paragraph(import: &Import, index: usize) -> &Paragraph {
        let BlockNode::Paragraph(paragraph) = &import.document.body()[index];
        paragraph
    }

    #[test]
    fn paragraphs_runs_and_run_properties_are_mapped() {
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Hello</w:t></w:r>
                 <w:r><w:t xml:space="preserve"> world</w:t></w:r></w:p>
            <w:p><w:r><w:t>Second</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let import = import(xml);
        assert_eq!(import.document.body().len(), 2);

        let first = paragraph(&import, 0);
        assert_eq!(first.inlines.len(), 2);
        let InlineNode::Run(bold) = &first.inlines[0] else {
            panic!("expected run");
        };
        assert_eq!(bold.text, "Hello");
        assert_eq!(bold.properties.bold, Some(true));
        let InlineNode::Run(plain) = &first.inlines[1] else {
            panic!("expected run");
        };
        assert_eq!(plain.text, " world");
        assert_eq!(plain.properties.bold, None);

        assert_eq!(paragraph(&import, 1).inlines.len(), 1);
    }

    #[test]
    fn adjacent_equal_property_runs_are_merged() {
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:t>a</w:t></w:r><w:r><w:t>b</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let import = import(xml);
        let para = paragraph(&import, 0);
        assert_eq!(para.inlines.len(), 1);
        let InlineNode::Run(run) = &para.inlines[0] else {
            panic!("expected run");
        };
        assert_eq!(run.text, "ab");
    }

    #[test]
    fn tabs_breaks_and_color_are_mapped() {
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:rPr><w:color w:val="FF0000"/></w:rPr><w:t>a</w:t><w:tab/><w:t>b</w:t>
                 <w:br w:type="page"/></w:r></w:p>
        </w:body></w:document>"#;
        let import = import(xml);
        let para = paragraph(&import, 0);
        assert_eq!(para.inlines.len(), 4);
        assert!(matches!(para.inlines[0], InlineNode::Run(_)));
        assert!(matches!(para.inlines[1], InlineNode::Tab(_)));
        assert!(matches!(para.inlines[2], InlineNode::Run(_)));
        assert!(matches!(
            para.inlines[3],
            InlineNode::Break(Break {
                kind: BreakKind::Page,
                ..
            })
        ));
        let InlineNode::Run(run) = &para.inlines[0] else {
            panic!();
        };
        assert_eq!(
            run.properties.color,
            Some(Color::Rgb(RgbColor { r: 255, g: 0, b: 0 }))
        );
    }

    #[test]
    fn unsupported_constructs_are_dispositioned_and_cell_text_is_flattened() {
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:sectPr/>
            <w:tbl><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
        </w:body></w:document>"#;
        let import = import(xml);
        // The table cell paragraph is flattened into the body (R4).
        assert_eq!(import.document.body().len(), 1);
        let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
            panic!("expected run");
        };
        assert_eq!(run.text, "cell");
        // Table/section structure is reported, ordered by feature name.
        let features: Vec<&str> = import
            .report
            .entries
            .iter()
            .map(|entry| entry.feature.as_str())
            .collect();
        assert!(features.contains(&"sectPr"));
        assert!(features.contains(&"tbl"));
        assert!(features.windows(2).all(|pair| pair[0] < pair[1]));
        for entry in &import.report.entries {
            assert_eq!(entry.model_outcome, ModelOutcome::Omitted);
            assert_eq!(entry.retention_outcome, RetentionOutcome::NotRetained);
        }
    }

    #[test]
    fn empty_body_yields_a_single_empty_paragraph() {
        let import = import(br#"<w:document xmlns:w="urn:w"><w:body/></w:document>"#);
        assert_eq!(import.document.body().len(), 1);
        assert!(paragraph(&import, 0).inlines.is_empty());
    }

    #[test]
    fn import_is_deterministic() {
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let first = import(xml).document.to_json().unwrap();
        let second = import(xml).document.to_json().unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn dtd_bearing_xml_is_rejected() {
        let xml = br#"<!DOCTYPE w:document><w:document xmlns:w="urn:w"><w:body/></w:document>"#;
        assert_eq!(
            import_main_document_xml(xml, ImportConfig::default()),
            Err(ImportError::MalformedXml)
        );
    }

    #[test]
    fn end_to_end_from_admitted_package() {
        use std::io::{Cursor, Write};
        use zip::write::SimpleFileOptions;
        use zip::{CompressionMethod, ZipWriter};

        let content_types = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
        let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>Hello DOCX</w:t></w:r></w:p></w:body></w:document>"#;

        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in [
            ("[Content_Types].xml", content_types.as_slice()),
            ("_rels/.rels", rels.as_slice()),
            ("word/document.xml", document.as_slice()),
        ] {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        let package_bytes = writer.finish().unwrap().into_inner();

        let mut package =
            DocxPackage::open(&package_bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
        let import = import_package(&mut package, ImportConfig::default()).unwrap();
        let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
            panic!("expected run");
        };
        assert_eq!(run.text, "Hello DOCX");
    }
}
