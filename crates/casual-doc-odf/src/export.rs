//! Deterministic bounded ODF 1.4 writing for the implemented ODT subset.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write};

use casual_doc_model::v1::{
    Alignment, BlockNode, BreakKind, Color, Definitions, Document, GroupChild, InlineNode,
    ParagraphProperties, RevisionKind, RunProperties,
};
use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

use crate::{
    CompatibilityEntry, CompatibilityReport, MANIFEST_PART, MIMETYPE_PART, ModelOutcome, ODT_MIME,
    OdfError, RetentionOutcome,
};

const CONTENT_HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4">"#;
const BODY_PREFIX: &str = "<office:body><office:text>";
const CONTENT_SUFFIX: &str = "</office:text></office:body></office:document-content>";
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.4"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text" manifest:version="1.4"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#;

/// Resource limits for deterministic ODT semantic export.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OdfExportLimits {
    /// Maximum generated `content.xml` bytes.
    pub max_content_bytes: usize,
    /// Maximum final ODT package bytes.
    pub max_package_bytes: usize,
    /// Maximum visited body blocks.
    pub max_blocks: usize,
    /// Maximum visited inline nodes.
    pub max_inline_nodes: usize,
    /// Maximum nested transparent-wrapper depth.
    pub max_recursion_depth: usize,
    /// Maximum aggregate source text bytes projected into XML.
    pub max_text_bytes: usize,
    /// Maximum distinct compatibility feature buckets before overflow folding.
    pub max_report_features: usize,
}

impl OdfExportLimits {
    /// Compiled maximum content XML bytes.
    pub const HARD_MAX_CONTENT_BYTES: usize = 512 * 1024 * 1024;
    /// Compiled maximum final package bytes.
    pub const HARD_MAX_PACKAGE_BYTES: usize = 1024 * 1024 * 1024;
    /// Compiled maximum visited blocks.
    pub const HARD_MAX_BLOCKS: usize = 4_000_000;
    /// Compiled maximum visited inline nodes.
    pub const HARD_MAX_INLINE_NODES: usize = 16_000_000;
    /// Compiled maximum wrapper depth.
    pub const HARD_MAX_RECURSION_DEPTH: usize = 256;
    /// Compiled maximum projected text bytes.
    pub const HARD_MAX_TEXT_BYTES: usize = 512 * 1024 * 1024;
    /// Compiled maximum report feature buckets.
    pub const HARD_MAX_REPORT_FEATURES: usize = 16_384;

    /// Validates configured limits against compiled safety ceilings.
    pub fn validate(self) -> Result<(), OdfError> {
        for (limit, value, hard_ceiling) in [
            (
                "odt_export_content_bytes",
                self.max_content_bytes,
                Self::HARD_MAX_CONTENT_BYTES,
            ),
            (
                "odt_export_package_bytes",
                self.max_package_bytes,
                Self::HARD_MAX_PACKAGE_BYTES,
            ),
            ("odt_export_blocks", self.max_blocks, Self::HARD_MAX_BLOCKS),
            (
                "odt_export_inline_nodes",
                self.max_inline_nodes,
                Self::HARD_MAX_INLINE_NODES,
            ),
            (
                "odt_export_recursion_depth",
                self.max_recursion_depth,
                Self::HARD_MAX_RECURSION_DEPTH,
            ),
            (
                "odt_export_text_bytes",
                self.max_text_bytes,
                Self::HARD_MAX_TEXT_BYTES,
            ),
            (
                "odt_export_report_features",
                self.max_report_features,
                Self::HARD_MAX_REPORT_FEATURES,
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

impl Default for OdfExportLimits {
    fn default() -> Self {
        Self {
            max_content_bytes: 128 * 1024 * 1024,
            max_package_bytes: 256 * 1024 * 1024,
            max_blocks: 500_000,
            max_inline_nodes: 4_000_000,
            max_recursion_depth: 64,
            max_text_bytes: 128 * 1024 * 1024,
            max_report_features: 4_096,
        }
    }
}

/// Successful deterministic semantic ODT export.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OdtExport {
    /// Complete ODF 1.4 package bytes.
    pub bytes: Vec<u8>,
    /// Explicit findings for model semantics not fully represented.
    pub report: CompatibilityReport,
}

#[derive(Debug)]
struct Reporter {
    counts: BTreeMap<(String, ModelOutcome), u32>,
    overflow: u32,
    max_features: usize,
}

impl Reporter {
    fn new(max_features: usize) -> Self {
        Self {
            counts: BTreeMap::new(),
            overflow: 0,
            max_features,
        }
    }

    fn record(&mut self, feature: &'static str, outcome: ModelOutcome) {
        let key = (feature.to_owned(), outcome);
        if let Some(count) = self.counts.get_mut(&key) {
            *count = count.saturating_add(1);
        } else if self.counts.len() < self.max_features {
            self.counts.insert(key, 1);
        } else {
            self.overflow = self.overflow.saturating_add(1);
        }
    }

    fn finish(self) -> CompatibilityReport {
        let mut entries = self
            .counts
            .into_iter()
            .map(
                |((feature, model_outcome), occurrences)| CompatibilityEntry {
                    feature,
                    occurrences,
                    model_outcome,
                    retention_outcome: RetentionOutcome::NotRetained,
                },
            )
            .collect::<Vec<_>>();
        if self.overflow != 0 {
            entries.push(CompatibilityEntry {
                feature: "odt.export.report.overflow".to_owned(),
                occurrences: self.overflow,
                model_outcome: ModelOutcome::Omitted,
                retention_outcome: RetentionOutcome::NotRetained,
            });
        }
        entries.sort_by(|left, right| {
            left.feature
                .cmp(&right.feature)
                .then_with(|| left.model_outcome.cmp(&right.model_outcome))
        });
        CompatibilityReport { entries }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum OdtParagraphAlignment {
    Start,
    End,
    Center,
    Justify,
}

impl OdtParagraphAlignment {
    const fn name(self) -> &'static str {
        match self {
            Self::Start => "P_start",
            Self::End => "P_end",
            Self::Center => "P_center",
            Self::Justify => "P_justify",
        }
    }

    const fn value(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
            Self::Center => "center",
            Self::Justify => "justify",
        }
    }
}

impl From<Alignment> for OdtParagraphAlignment {
    fn from(value: Alignment) -> Self {
        match value {
            Alignment::Start => Self::Start,
            Alignment::End => Self::End,
            Alignment::Center => Self::Center,
            Alignment::Justify => Self::Justify,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct OdtRunStyle {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strike: Option<bool>,
    color: Option<(u8, u8, u8)>,
    size_half_points: Option<u32>,
}

impl OdtRunStyle {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    fn name(&self) -> String {
        format!(
            "T_b{}_i{}_u{}_s{}_c{}_z{}",
            tri_state(self.bold),
            tri_state(self.italic),
            tri_state(self.underline),
            tri_state(self.strike),
            self.color
                .map(|(red, green, blue)| format!("{red:02x}{green:02x}{blue:02x}"))
                .unwrap_or_else(|| "n".to_owned()),
            self.size_half_points
                .map(|size| size.to_string())
                .unwrap_or_else(|| "n".to_owned()),
        )
    }
}

const fn tri_state(value: Option<bool>) -> &'static str {
    match value {
        None => "n",
        Some(false) => "0",
        Some(true) => "1",
    }
}

fn split_run_properties(properties: &RunProperties) -> (OdtRunStyle, RunProperties) {
    let mut remainder = properties.clone();
    let mut style = OdtRunStyle {
        bold: remainder.bold.take(),
        italic: remainder.italic.take(),
        underline: remainder.underline.take(),
        strike: remainder.strike.take(),
        size_half_points: remainder.size_half_points.take(),
        ..OdtRunStyle::default()
    };
    if let Some(Color::Rgb(color)) = remainder.color {
        style.color = Some((color.r, color.g, color.b));
        remainder.color = None;
    }
    (style, remainder)
}

struct Writer {
    xml: String,
    paragraph_styles: BTreeSet<OdtParagraphAlignment>,
    run_styles: BTreeSet<OdtRunStyle>,
    limits: OdfExportLimits,
    blocks: usize,
    inlines: usize,
    text_bytes: usize,
    paragraphs_written: usize,
    reporter: Reporter,
}

impl Writer {
    fn new(limits: OdfExportLimits) -> Result<Self, OdfError> {
        let mut writer = Self {
            xml: String::new(),
            paragraph_styles: BTreeSet::new(),
            run_styles: BTreeSet::new(),
            limits,
            blocks: 0,
            inlines: 0,
            text_bytes: 0,
            paragraphs_written: 0,
            reporter: Reporter::new(limits.max_report_features),
        };
        writer.push(BODY_PREFIX)?;
        Ok(writer)
    }

    fn push(&mut self, value: &str) -> Result<(), OdfError> {
        let observed = self
            .xml
            .len()
            .checked_add(value.len())
            .ok_or(OdfError::LimitExceeded {
                limit: "odt_export_content_bytes",
                observed: usize::MAX,
                allowed: self.limits.max_content_bytes,
            })?;
        enforce(
            "odt_export_content_bytes",
            observed,
            self.limits.max_content_bytes,
        )?;
        self.xml.push_str(value);
        Ok(())
    }

    fn visit_block(&mut self) -> Result<(), OdfError> {
        self.blocks = checked_add(self.blocks, 1, "odt_export_blocks", self.limits.max_blocks)?;
        Ok(())
    }

    fn visit_inline(&mut self) -> Result<(), OdfError> {
        self.inlines = checked_add(
            self.inlines,
            1,
            "odt_export_inline_nodes",
            self.limits.max_inline_nodes,
        )?;
        Ok(())
    }

    fn check_depth(&self, depth: usize) -> Result<(), OdfError> {
        enforce(
            "odt_export_recursion_depth",
            depth,
            self.limits.max_recursion_depth,
        )
    }

    fn write_blocks(&mut self, blocks: &[BlockNode], depth: usize) -> Result<(), OdfError> {
        self.check_depth(depth)?;
        for block in blocks {
            self.visit_block()?;
            match block {
                BlockNode::Paragraph(paragraph) => {
                    self.paragraphs_written = self.paragraphs_written.saturating_add(1);
                    let mut remainder = paragraph.properties.clone();
                    let outline = remainder.outline_level.take();
                    let alignment = remainder.alignment.take().map(OdtParagraphAlignment::from);
                    if remainder != ParagraphProperties::default() {
                        self.reporter
                            .record("odt.export.paragraph_properties", ModelOutcome::Omitted);
                    }
                    if let Some(level) = outline {
                        self.push("<text:h text:outline-level=\"")?;
                        self.push(&(u16::from(level) + 1).to_string())?;
                        if let Some(alignment) = alignment {
                            self.paragraph_styles.insert(alignment);
                            self.push("\" text:style-name=\"")?;
                            self.push(alignment.name())?;
                        }
                        self.push("\">")?;
                    } else {
                        self.push("<text:p")?;
                        if let Some(alignment) = alignment {
                            self.paragraph_styles.insert(alignment);
                            self.push(" text:style-name=\"")?;
                            self.push(alignment.name())?;
                            self.push("\"")?;
                        }
                        self.push(">")?;
                    }
                    self.write_inlines(&paragraph.inlines, depth + 1)?;
                    self.push(if outline.is_some() {
                        "</text:h>"
                    } else {
                        "</text:p>"
                    })?;
                }
                BlockNode::Sdt(sdt) => {
                    self.reporter
                        .record("odt.export.block_content_control", ModelOutcome::Degraded);
                    self.write_blocks(&sdt.blocks, depth + 1)?;
                }
                BlockNode::Table(_) => self
                    .reporter
                    .record("odt.export.table", ModelOutcome::Omitted),
                BlockNode::AltChunk(_) => self
                    .reporter
                    .record("odt.export.alt_chunk", ModelOutcome::Omitted),
            }
        }
        Ok(())
    }

    fn write_inlines(&mut self, inlines: &[InlineNode], depth: usize) -> Result<(), OdfError> {
        self.check_depth(depth)?;
        for inline in inlines {
            self.visit_inline()?;
            match inline {
                InlineNode::Run(run) => {
                    let (style, remainder) = split_run_properties(&run.properties);
                    if remainder != RunProperties::default() {
                        self.reporter
                            .record("odt.export.run_properties", ModelOutcome::Omitted);
                    }
                    let styled = !style.is_empty();
                    if styled {
                        let name = style.name();
                        self.run_styles.insert(style);
                        self.push("<text:span text:style-name=\"")?;
                        self.push(&name)?;
                        self.push("\">")?;
                    }
                    self.write_text(&run.text)?;
                    if styled {
                        self.push("</text:span>")?;
                    }
                }
                InlineNode::Tab(_) => self.push("<text:tab/>")?,
                InlineNode::Break(node) => {
                    if node.kind != BreakKind::Line {
                        self.reporter
                            .record("odt.export.page_or_column_break", ModelOutcome::Degraded);
                    }
                    self.push("<text:line-break/>")?;
                }
                InlineNode::Hyperlink(link) => {
                    self.reporter
                        .record("odt.export.hyperlink", ModelOutcome::Degraded);
                    self.write_inlines(&link.inlines, depth + 1)?;
                }
                InlineNode::Field(field) => {
                    self.reporter
                        .record("odt.export.field", ModelOutcome::Degraded);
                    self.write_inlines(&field.inlines, depth + 1)?;
                }
                InlineNode::Revision(revision) => {
                    self.reporter
                        .record("odt.export.revision", ModelOutcome::Degraded);
                    if matches!(
                        revision.kind,
                        RevisionKind::Insertion | RevisionKind::MoveTo
                    ) {
                        self.write_inlines(&revision.inlines, depth + 1)?;
                    }
                }
                InlineNode::Sdt(sdt) => {
                    self.reporter
                        .record("odt.export.inline_content_control", ModelOutcome::Degraded);
                    self.write_inlines(&sdt.inlines, depth + 1)?;
                }
                InlineNode::Drawing(drawing) => {
                    self.write_alt(drawing.descr.as_deref(), "odt.export.drawing")?
                }
                InlineNode::AnchoredDrawing(drawing) => {
                    self.write_alt(drawing.descr.as_deref(), "odt.export.anchored_drawing")?
                }
                InlineNode::Math(math) => {
                    self.reporter
                        .record("odt.export.math", ModelOutcome::Degraded);
                    self.write_text(&math.text)?;
                }
                InlineNode::Symbol(symbol) => {
                    self.reporter
                        .record("odt.export.symbol_font", ModelOutcome::Degraded);
                    if let Some(character) = char::from_u32(symbol.char) {
                        self.write_text(&character.to_string())?;
                    }
                }
                InlineNode::NoBreakHyphen(_) => self.write_text("\u{2011}")?,
                InlineNode::SoftHyphen(_) => self.write_text("\u{00ad}")?,
                InlineNode::PositionalTab(_) => {
                    self.reporter
                        .record("odt.export.positional_tab", ModelOutcome::Degraded);
                    self.push("<text:tab/>")?;
                }
                InlineNode::EmbeddedObject(_) => self
                    .reporter
                    .record("odt.export.embedded_object", ModelOutcome::Omitted),
                InlineNode::TextBox(_) => self
                    .reporter
                    .record("odt.export.text_box", ModelOutcome::Omitted),
                InlineNode::Group(group) => {
                    self.reporter
                        .record("odt.export.group", ModelOutcome::Omitted);
                    if group
                        .children
                        .iter()
                        .any(|child| matches!(child, GroupChild::TextBox(_) | GroupChild::Group(_)))
                    {
                        self.reporter
                            .record("odt.export.group_text", ModelOutcome::Omitted);
                    }
                }
                InlineNode::NoteReference(_) => self
                    .reporter
                    .record("odt.export.note_reference", ModelOutcome::Omitted),
                InlineNode::CommentReference(_)
                | InlineNode::CommentRangeStart(_)
                | InlineNode::CommentRangeEnd(_) => self
                    .reporter
                    .record("odt.export.comment", ModelOutcome::Omitted),
                InlineNode::BookmarkStart(_) | InlineNode::BookmarkEnd(_) => self
                    .reporter
                    .record("odt.export.bookmark", ModelOutcome::Omitted),
                InlineNode::MoveRangeStart(_) | InlineNode::MoveRangeEnd(_) => self
                    .reporter
                    .record("odt.export.move_range", ModelOutcome::Omitted),
                InlineNode::HorizontalRule(_) => self
                    .reporter
                    .record("odt.export.horizontal_rule", ModelOutcome::Omitted),
            }
        }
        Ok(())
    }

    fn write_alt(
        &mut self,
        description: Option<&str>,
        feature: &'static str,
    ) -> Result<(), OdfError> {
        if let Some(description) = description {
            self.reporter.record(feature, ModelOutcome::Degraded);
            self.write_text(description)
        } else {
            self.reporter.record(feature, ModelOutcome::Omitted);
            Ok(())
        }
    }

    fn write_text(&mut self, text: &str) -> Result<(), OdfError> {
        self.text_bytes = checked_add(
            self.text_bytes,
            text.len(),
            "odt_export_text_bytes",
            self.limits.max_text_bytes,
        )?;
        let mut plain = String::new();
        let mut characters = text.chars().peekable();
        while let Some(character) = characters.next() {
            match character {
                ' ' => {
                    self.flush_plain(&mut plain)?;
                    let mut count = 1_usize;
                    while characters.peek() == Some(&' ') {
                        characters.next();
                        count += 1;
                    }
                    if count == 1 {
                        self.push("<text:s/>")?;
                    } else {
                        self.push("<text:s text:c=\"")?;
                        self.push(&count.to_string())?;
                        self.push("\"/>")?;
                    }
                }
                '\t' => {
                    self.flush_plain(&mut plain)?;
                    self.push("<text:tab/>")?;
                }
                '\r' => {
                    self.flush_plain(&mut plain)?;
                    if characters.peek() == Some(&'\n') {
                        characters.next();
                    }
                    self.push("<text:line-break/>")?;
                }
                '\n' => {
                    self.flush_plain(&mut plain)?;
                    self.push("<text:line-break/>")?;
                }
                value if is_xml_character(value) => plain.push(value),
                _ => return Err(OdfError::InvalidXmlCharacter),
            }
        }
        self.flush_plain(&mut plain)
    }

    fn flush_plain(&mut self, plain: &mut String) -> Result<(), OdfError> {
        if plain.is_empty() {
            return Ok(());
        }
        let escaped = quick_xml::escape::escape(plain.as_str()).into_owned();
        plain.clear();
        self.push(&escaped)
    }
}

fn automatic_styles_xml(
    paragraph_styles: &BTreeSet<OdtParagraphAlignment>,
    run_styles: &BTreeSet<OdtRunStyle>,
    max_content_bytes: usize,
) -> Result<String, OdfError> {
    if paragraph_styles.is_empty() && run_styles.is_empty() {
        return Ok(String::new());
    }
    let mut xml = String::new();
    push_bounded(&mut xml, "<office:automatic-styles>", max_content_bytes)?;
    for alignment in paragraph_styles {
        push_bounded(&mut xml, "<style:style style:name=\"", max_content_bytes)?;
        push_bounded(&mut xml, alignment.name(), max_content_bytes)?;
        push_bounded(
            &mut xml,
            "\" style:family=\"paragraph\"><style:paragraph-properties fo:text-align=\"",
            max_content_bytes,
        )?;
        push_bounded(&mut xml, alignment.value(), max_content_bytes)?;
        push_bounded(&mut xml, "\"/></style:style>", max_content_bytes)?;
    }
    for style in run_styles {
        push_bounded(&mut xml, "<style:style style:name=\"", max_content_bytes)?;
        push_bounded(&mut xml, &style.name(), max_content_bytes)?;
        push_bounded(
            &mut xml,
            "\" style:family=\"text\"><style:text-properties",
            max_content_bytes,
        )?;
        if let Some(bold) = style.bold {
            push_bounded(
                &mut xml,
                if bold {
                    " fo:font-weight=\"bold\""
                } else {
                    " fo:font-weight=\"normal\""
                },
                max_content_bytes,
            )?;
        }
        if let Some(italic) = style.italic {
            push_bounded(
                &mut xml,
                if italic {
                    " fo:font-style=\"italic\""
                } else {
                    " fo:font-style=\"normal\""
                },
                max_content_bytes,
            )?;
        }
        if let Some(underline) = style.underline {
            push_bounded(
                &mut xml,
                if underline {
                    " style:text-underline-style=\"solid\""
                } else {
                    " style:text-underline-style=\"none\""
                },
                max_content_bytes,
            )?;
        }
        if let Some(strike) = style.strike {
            push_bounded(
                &mut xml,
                if strike {
                    " style:text-line-through-style=\"solid\""
                } else {
                    " style:text-line-through-style=\"none\""
                },
                max_content_bytes,
            )?;
        }
        if let Some((red, green, blue)) = style.color {
            push_bounded(&mut xml, " fo:color=\"#", max_content_bytes)?;
            push_bounded(
                &mut xml,
                &format!("{red:02x}{green:02x}{blue:02x}"),
                max_content_bytes,
            )?;
            push_bounded(&mut xml, "\"", max_content_bytes)?;
        }
        if let Some(size) = style.size_half_points {
            push_bounded(&mut xml, " fo:font-size=\"", max_content_bytes)?;
            push_bounded(&mut xml, &(size / 2).to_string(), max_content_bytes)?;
            if size % 2 != 0 {
                push_bounded(&mut xml, ".5", max_content_bytes)?;
            }
            push_bounded(&mut xml, "pt\"", max_content_bytes)?;
        }
        push_bounded(&mut xml, "/></style:style>", max_content_bytes)?;
    }
    push_bounded(&mut xml, "</office:automatic-styles>", max_content_bytes)?;
    Ok(xml)
}

fn push_bounded(output: &mut String, value: &str, allowed: usize) -> Result<(), OdfError> {
    let observed = output
        .len()
        .checked_add(value.len())
        .ok_or(OdfError::LimitExceeded {
            limit: "odt_export_content_bytes",
            observed: usize::MAX,
            allowed,
        })?;
    enforce("odt_export_content_bytes", observed, allowed)?;
    output.push_str(value);
    Ok(())
}

/// Writes a validated normalized document as a deterministic ODF 1.4 package.
pub fn write_odt(document: &Document, limits: OdfExportLimits) -> Result<OdtExport, OdfError> {
    limits.validate()?;
    document.validate().map_err(|_| OdfError::InvalidModel)?;
    let mut writer = Writer::new(limits)?;
    if document.definitions() != &Definitions::default() {
        writer
            .reporter
            .record("odt.export.definitions", ModelOutcome::Omitted);
    }
    if document.properties().is_some() {
        writer
            .reporter
            .record("odt.export.document_properties", ModelOutcome::Omitted);
    }
    if document.background().is_some() {
        writer
            .reporter
            .record("odt.export.background", ModelOutcome::Omitted);
    }
    writer.write_blocks(document.body(), 0)?;
    if writer.paragraphs_written == 0 {
        writer.push("<text:p/>")?;
    }
    writer.push(CONTENT_SUFFIX)?;
    let styles = automatic_styles_xml(
        &writer.paragraph_styles,
        &writer.run_styles,
        limits.max_content_bytes,
    )?;
    let content_len = CONTENT_HEADER
        .len()
        .checked_add(styles.len())
        .and_then(|value| value.checked_add(writer.xml.len()))
        .ok_or(OdfError::LimitExceeded {
            limit: "odt_export_content_bytes",
            observed: usize::MAX,
            allowed: limits.max_content_bytes,
        })?;
    enforce(
        "odt_export_content_bytes",
        content_len,
        limits.max_content_bytes,
    )?;
    let mut content = String::with_capacity(content_len);
    content.push_str(CONTENT_HEADER);
    content.push_str(&styles);
    content.push_str(&writer.xml);
    let content = content.into_bytes();
    let report = writer.reporter.finish();
    let bytes = package(&content, limits)?;
    Ok(OdtExport { bytes, report })
}

fn package(content: &[u8], limits: OdfExportLimits) -> Result<Vec<u8>, OdfError> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(
        MIMETYPE_PART,
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
    )
    .map_err(|_| OdfError::SerializationFailed)?;
    zip.write_all(ODT_MIME.as_bytes())
        .map_err(|_| OdfError::SerializationFailed)?;
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(crate::CONTENT_PART, deflated)
        .map_err(|_| OdfError::SerializationFailed)?;
    zip.write_all(content)
        .map_err(|_| OdfError::SerializationFailed)?;
    zip.start_file(MANIFEST_PART, deflated)
        .map_err(|_| OdfError::SerializationFailed)?;
    zip.write_all(MANIFEST.as_bytes())
        .map_err(|_| OdfError::SerializationFailed)?;
    let bytes = zip
        .finish()
        .map_err(|_| OdfError::SerializationFailed)?
        .into_inner();
    enforce(
        "odt_export_package_bytes",
        bytes.len(),
        limits.max_package_bytes,
    )?;
    Ok(bytes)
}

fn checked_add(
    value: usize,
    add: usize,
    limit: &'static str,
    allowed: usize,
) -> Result<usize, OdfError> {
    let observed = value.checked_add(add).ok_or(OdfError::LimitExceeded {
        limit,
        observed: usize::MAX,
        allowed,
    })?;
    enforce(limit, observed, allowed)?;
    Ok(observed)
}

fn enforce(limit: &'static str, observed: usize, allowed: usize) -> Result<(), OdfError> {
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

fn is_xml_character(character: char) -> bool {
    matches!(character as u32, 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x10ffff)
}

#[cfg(test)]
mod tests {
    use casual_doc_model::v1::{BlockNode, InlineNode, RgbColor};

    use super::*;
    use crate::{OdfImportLimits, OdfPackageLimits, OdfVersion, OdtPackage, import_content_xml};

    const CORE: &[u8] = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.4"><office:body><office:text><text:p>one<text:s text:c="2"/>two<text:tab/>three<text:line-break/>four</text:p><text:h text:outline-level="2">Title</text:h></office:text></office:body></office:document-content>"#;

    fn core_document() -> Document {
        import_content_xml(CORE, OdfVersion::V1_4, OdfImportLimits::default())
            .unwrap()
            .document
    }

    #[test]
    fn core_subset_is_deterministic_valid_and_semantically_stable() {
        let document = core_document();
        let first = write_odt(&document, OdfExportLimits::default()).unwrap();
        let second = write_odt(&document, OdfExportLimits::default()).unwrap();
        assert_eq!(first.bytes, second.bytes);
        assert!(first.report.entries.is_empty());
        let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
        let reopened = package.import_document(OdfImportLimits::default()).unwrap();
        assert_eq!(reopened.document, document);
    }

    #[test]
    fn supported_direct_formatting_uses_deterministic_automatic_styles() {
        let mut document = core_document();
        let BlockNode::Paragraph(paragraph) = &mut document.body_mut()[0] else {
            panic!("paragraph")
        };
        paragraph.properties.alignment = Some(Alignment::Center);
        let InlineNode::Run(run) = &mut paragraph.inlines[0] else {
            panic!("run")
        };
        run.properties.bold = Some(true);
        run.properties.italic = Some(false);
        run.properties.underline = Some(true);
        run.properties.strike = Some(false);
        run.properties.color = Some(Color::Rgb(RgbColor {
            r: 0x1a,
            g: 0x2b,
            b: 0x3c,
        }));
        run.properties.size_half_points = Some(21);

        let first = write_odt(&document, OdfExportLimits::default()).unwrap();
        let second = write_odt(&document, OdfExportLimits::default()).unwrap();
        assert_eq!(first.bytes, second.bytes);
        assert!(first.report.entries.is_empty());

        let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
        let content = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
        assert!(content.contains(
            "<style:style style:name=\"P_center\" style:family=\"paragraph\"><style:paragraph-properties fo:text-align=\"center\"/></style:style>"
        ));
        assert!(content.contains(
            "<style:style style:name=\"T_b1_i0_u1_s0_c1a2b3c_z21\" style:family=\"text\"><style:text-properties fo:font-weight=\"bold\" fo:font-style=\"normal\" style:text-underline-style=\"solid\" style:text-line-through-style=\"none\" fo:color=\"#1a2b3c\" fo:font-size=\"10.5pt\"/></style:style>"
        ));
        assert!(content.contains("<text:p text:style-name=\"P_center\">"));
        assert!(content.contains("<text:span text:style-name=\"T_b1_i0_u1_s0_c1a2b3c_z21\">"));
        assert!(content.contains("</text:span>"));

        let reopened = package.import_document(OdfImportLimits::default()).unwrap();
        assert!(reopened.report.entries.is_empty());
        let BlockNode::Paragraph(reopened_paragraph) = &reopened.document.body()[0] else {
            panic!("reopened paragraph")
        };
        assert_eq!(
            reopened_paragraph.properties.alignment,
            Some(Alignment::Center)
        );
        assert!(reopened_paragraph.inlines.iter().any(|inline| {
            matches!(inline, InlineNode::Run(run)
                if run.properties.bold == Some(true)
                    && run.properties.italic == Some(false)
                    && run.properties.underline == Some(true)
                    && run.properties.strike == Some(false)
                    && run.properties.color == Some(Color::Rgb(RgbColor {
                        r: 0x1a,
                        g: 0x2b,
                        b: 0x3c,
                    }))
                    && run.properties.size_half_points == Some(21))
        }));
        let reexported = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
        let mut package = OdtPackage::open(&reexported.bytes, OdfPackageLimits::default()).unwrap();
        let reopened_again = package.import_document(OdfImportLimits::default()).unwrap();
        assert_eq!(reopened_again.document, reopened.document);
    }

    #[test]
    fn unsupported_formatting_is_reported_and_limits_fail_atomically() {
        let mut document = core_document();
        let BlockNode::Paragraph(paragraph) = &mut document.body_mut()[0] else {
            panic!("paragraph")
        };
        let InlineNode::Run(run) = &mut paragraph.inlines[0] else {
            panic!("run")
        };
        run.properties.bold = Some(true);
        run.properties.all_caps = Some(true);
        let exported = write_odt(&document, OdfExportLimits::default()).unwrap();
        assert!(
            exported
                .report
                .entries
                .iter()
                .any(|entry| entry.feature == "odt.export.run_properties")
        );

        let error = write_odt(
            &document,
            OdfExportLimits {
                max_content_bytes: 8,
                ..OdfExportLimits::default()
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            OdfError::LimitExceeded {
                limit: "odt_export_content_bytes",
                ..
            }
        ));
        let invalid = write_odt(
            &document,
            OdfExportLimits {
                max_blocks: usize::MAX,
                ..OdfExportLimits::default()
            },
        )
        .unwrap_err();
        assert!(matches!(
            invalid,
            OdfError::InvalidLimitConfiguration {
                limit: "odt_export_blocks",
                ..
            }
        ));
    }
}
