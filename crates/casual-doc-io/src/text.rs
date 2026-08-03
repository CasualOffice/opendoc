//! Built-in deterministic UTF-8 plain-text adapter.

use std::collections::BTreeMap;

use casual_doc_model::v1::{
    BlockNode, BreakKind, Definitions, Document, GroupChild, InlineNode, Paragraph,
    ParagraphProperties, RevisionKind, Run, RunProperties, Tab,
};
use casual_doc_model::{IdGenerator, NodeId};

use crate::{
    AdapterError, CompatibilityEntry, CompatibilityReport, DocumentResources, ExportArtifact,
    ExportMode, ExportRequest, FeatureLocation, FormatDescriptor, FormatExporter, FormatId,
    FormatImporter, FormatProfile, ImportArtifact, ImportRequest, ModelOutcome, ProbeRequest,
    ProbeResult, RetentionOutcome, SourceEnvelope, formats,
};

const TEXT_MIME: &str = "text/plain";

/// Host-configurable plain-text limits with non-bypassable hard ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlainTextLimits {
    /// Maximum input bytes before UTF-8 decoding.
    pub max_input_bytes: usize,
    /// Maximum canonical exported UTF-8 bytes.
    pub max_output_bytes: usize,
    /// Maximum logical paragraphs after newline normalization.
    pub max_paragraphs: usize,
    /// Maximum Unicode scalar values after BOM/newline normalization.
    pub max_unicode_scalar_values: usize,
    /// Maximum UTF-8 bytes in one normalized logical line.
    pub max_line_bytes: usize,
}

impl PlainTextLimits {
    /// Hard maximum input bytes.
    pub const HARD_MAX_INPUT_BYTES: usize = 256 * 1024 * 1024;
    /// Hard maximum output bytes.
    pub const HARD_MAX_OUTPUT_BYTES: usize = 256 * 1024 * 1024;
    /// Hard maximum logical paragraphs.
    pub const HARD_MAX_PARAGRAPHS: usize = 8_000_000;
    /// Hard maximum Unicode scalar values.
    pub const HARD_MAX_UNICODE_SCALAR_VALUES: usize = 200_000_000;
    /// Hard maximum bytes in one line.
    pub const HARD_MAX_LINE_BYTES: usize = 64 * 1024 * 1024;

    fn validate(self) -> Result<(), AdapterError> {
        for (name, value, ceiling) in [
            (
                "text_input_bytes",
                self.max_input_bytes,
                Self::HARD_MAX_INPUT_BYTES,
            ),
            (
                "text_output_bytes",
                self.max_output_bytes,
                Self::HARD_MAX_OUTPUT_BYTES,
            ),
            (
                "text_paragraphs",
                self.max_paragraphs,
                Self::HARD_MAX_PARAGRAPHS,
            ),
            (
                "text_unicode_scalar_values",
                self.max_unicode_scalar_values,
                Self::HARD_MAX_UNICODE_SCALAR_VALUES,
            ),
            (
                "text_line_bytes",
                self.max_line_bytes,
                Self::HARD_MAX_LINE_BYTES,
            ),
        ] {
            if value > ceiling {
                return Err(AdapterError::new(format!(
                    "limit {name} value {value} exceeds hard ceiling {ceiling}"
                )));
            }
        }
        Ok(())
    }
}

impl Default for PlainTextLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024 * 1024,
            max_output_bytes: 64 * 1024 * 1024,
            max_paragraphs: 2_000_000,
            max_unicode_scalar_values: 50_000_000,
            max_line_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Debug)]
struct TextSourceState {
    original_bytes: Option<Vec<u8>>,
}

/// Built-in bounded UTF-8 plain-text adapter.
#[derive(Clone, Debug)]
pub struct PlainTextAdapter {
    descriptor: FormatDescriptor,
    limits: PlainTextLimits,
}

impl PlainTextAdapter {
    /// Creates an adapter with explicit plain-text limits.
    #[must_use]
    pub fn new(limits: PlainTextLimits) -> Self {
        Self {
            descriptor: FormatDescriptor {
                id: FormatId::new(formats::TEXT).expect("built-in text format id is valid"),
                display_name: "Plain Text".to_owned(),
                mime_types: vec![TEXT_MIME.to_owned()],
                extensions: vec!["txt".to_owned(), "text".to_owned()],
                can_import: true,
                can_export: true,
                exact_if_unchanged: true,
                preserve_when_safe: false,
            },
            limits,
        }
    }
}

impl Default for PlainTextAdapter {
    fn default() -> Self {
        Self::new(PlainTextLimits::default())
    }
}

impl FormatImporter for PlainTextAdapter {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn probe(&self, request: ProbeRequest<'_>) -> ProbeResult {
        match normalize_text(request.bytes, self.limits) {
            Ok(_) => ProbeResult::possible("text.valid-utf8"),
            Err(_) => ProbeResult::no_match("text.invalid-or-over-limit"),
        }
    }

    fn import(&self, request: ImportRequest<'_>) -> Result<ImportArtifact, AdapterError> {
        let normalized = normalize_text(request.bytes, self.limits)?;
        let document = document_from_text(&normalized)?;
        Ok(ImportArtifact {
            document,
            resources: DocumentResources::default(),
            source: SourceEnvelope::new(
                self.descriptor.id.clone(),
                env!("CARGO_PKG_VERSION").to_owned(),
                TextSourceState {
                    original_bytes: request.retain_source.then(|| request.bytes.to_vec()),
                },
            ),
            report: CompatibilityReport::default(),
            format: FormatProfile {
                format: self.descriptor.id.clone(),
                version: Some("utf-8".to_owned()),
            },
        })
    }
}

impl FormatExporter for PlainTextAdapter {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn export(&self, request: ExportRequest<'_>) -> Result<ExportArtifact, AdapterError> {
        self.limits.validate()?;
        request
            .document
            .validate()
            .map_err(|error| AdapterError::new(format!("normalized model: {error}")))?;
        let matching_source = request
            .source
            .filter(|source| source.format() == &self.descriptor.id)
            .and_then(SourceEnvelope::state::<TextSourceState>);

        let (bytes, report) = match request.mode {
            ExportMode::ExactIfUnchanged => {
                let bytes = matching_source
                    .filter(|_| request.source_unchanged)
                    .and_then(|source| source.original_bytes.clone())
                    .ok_or_else(|| {
                        AdapterError::new(
                            "exact export requires matching retained source and an unchanged document",
                        )
                    })?;
                enforce(
                    "text_output_bytes",
                    bytes.len(),
                    self.limits.max_output_bytes,
                )?;
                (bytes, CompatibilityReport::default())
            }
            ExportMode::Semantic | ExportMode::PreserveWhenSafe => {
                let mut output = TextOutput::new(self.limits.max_output_bytes);
                let mut losses = Losses::default();
                append_blocks(request.document.body(), &mut output, &mut losses)?;
                if request.document.definitions() != &Definitions::default() {
                    losses.record("plain_text.document_definitions", ModelOutcome::Omitted);
                }
                if request.document.properties().is_some() {
                    losses.record("plain_text.document_properties", ModelOutcome::Omitted);
                }
                if request.document.background().is_some() {
                    losses.record("plain_text.page_background", ModelOutcome::Omitted);
                }
                if !request.resources.is_empty() {
                    losses.record_many(
                        "plain_text.binary_resources",
                        ModelOutcome::Omitted,
                        request.resources.as_map().len(),
                    );
                }
                if request.source.is_some() && matching_source.is_none() {
                    losses.record("plain_text.source_envelope", ModelOutcome::Omitted);
                }
                (output.finish(), losses.finish())
            }
        };

        Ok(ExportArtifact {
            bytes,
            report,
            format: FormatProfile {
                format: self.descriptor.id.clone(),
                version: Some("utf-8".to_owned()),
            },
            mime_type: TEXT_MIME.to_owned(),
            suggested_extension: "txt".to_owned(),
        })
    }
}

fn normalize_text(bytes: &[u8], limits: PlainTextLimits) -> Result<String, AdapterError> {
    limits.validate()?;
    enforce("text_input_bytes", bytes.len(), limits.max_input_bytes)?;
    let text = std::str::from_utf8(bytes)
        .map_err(|_| AdapterError::new("plain text is not valid UTF-8"))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut normalized = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    let mut scalar_values = 0_usize;
    let mut paragraphs = 1_usize;
    let mut line_bytes = 0_usize;
    enforce("text_paragraphs", paragraphs, limits.max_paragraphs)?;
    while let Some(character) = characters.next() {
        if character < '\u{20}' && !matches!(character, '\t' | '\n' | '\r') {
            return Err(AdapterError::new(
                "plain text contains an unsupported C0 control character",
            ));
        }
        scalar_values = scalar_values.saturating_add(1);
        enforce(
            "text_unicode_scalar_values",
            scalar_values,
            limits.max_unicode_scalar_values,
        )?;
        if character == '\r' {
            if characters.peek() == Some(&'\n') {
                characters.next();
            }
            normalized.push('\n');
            paragraphs = paragraphs.saturating_add(1);
            line_bytes = 0;
        } else if character == '\n' {
            normalized.push('\n');
            paragraphs = paragraphs.saturating_add(1);
            line_bytes = 0;
        } else {
            normalized.push(character);
            line_bytes = line_bytes.saturating_add(character.len_utf8());
            enforce("text_line_bytes", line_bytes, limits.max_line_bytes)?;
        }
        enforce("text_paragraphs", paragraphs, limits.max_paragraphs)?;
    }
    Ok(normalized)
}

fn enforce(name: &'static str, observed: usize, allowed: usize) -> Result<(), AdapterError> {
    if observed > allowed {
        return Err(AdapterError::new(format!(
            "limit {name} observed {observed} exceeds allowed {allowed}"
        )));
    }
    Ok(())
}

fn document_from_text(text: &str) -> Result<Document, AdapterError> {
    let mut ids = IdGenerator::new(text_namespace(text));
    let document_id = next_id(&mut ids)?;
    let mut body = Vec::new();
    for line in text.split('\n') {
        let paragraph_id = next_id(&mut ids)?;
        let mut inlines = Vec::new();
        for (index, segment) in line.split('\t').enumerate() {
            if index != 0 {
                inlines.push(InlineNode::Tab(Tab {
                    id: next_id(&mut ids)?,
                }));
            }
            if !segment.is_empty() {
                inlines.push(InlineNode::Run(Run {
                    id: next_id(&mut ids)?,
                    properties: RunProperties::default(),
                    text: segment.to_owned(),
                }));
            }
        }
        body.push(BlockNode::Paragraph(Paragraph {
            id: paragraph_id,
            properties: ParagraphProperties::default(),
            inlines,
        }));
    }
    Document::new(document_id, body, Definitions::default())
        .map_err(|error| AdapterError::new(format!("plain-text model: {error}")))
}

fn next_id(ids: &mut IdGenerator) -> Result<NodeId, AdapterError> {
    ids.next_id()
        .map_err(|error| AdapterError::new(format!("plain-text identity: {error}")))
}

fn text_namespace(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

struct TextOutput {
    value: String,
    max_bytes: usize,
}

impl TextOutput {
    fn new(max_bytes: usize) -> Self {
        Self {
            value: String::new(),
            max_bytes,
        }
    }

    fn push_text(&mut self, value: &str) -> Result<(), AdapterError> {
        let mut characters = value.chars().peekable();
        while let Some(character) = characters.next() {
            if character < '\u{20}' && !matches!(character, '\t' | '\n' | '\r') {
                return Err(AdapterError::new(
                    "normalized text contains an unsupported C0 control character",
                ));
            }
            if character == '\r' {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                self.push('\n')?;
            } else {
                self.push(character)?;
            }
        }
        Ok(())
    }

    fn push(&mut self, value: char) -> Result<(), AdapterError> {
        let observed = self.value.len().saturating_add(value.len_utf8());
        enforce("text_output_bytes", observed, self.max_bytes)?;
        self.value.push(value);
        Ok(())
    }

    fn finish(self) -> Vec<u8> {
        self.value.into_bytes()
    }
}

#[derive(Default)]
struct Losses {
    entries: BTreeMap<&'static str, (u32, ModelOutcome)>,
}

impl Losses {
    fn record(&mut self, feature: &'static str, outcome: ModelOutcome) {
        self.record_many(feature, outcome, 1);
    }

    fn record_many(&mut self, feature: &'static str, outcome: ModelOutcome, count: usize) {
        let count = u32::try_from(count).unwrap_or(u32::MAX);
        self.entries
            .entry(feature)
            .and_modify(|entry| entry.0 = entry.0.saturating_add(count))
            .or_insert((count, outcome));
    }

    fn finish(self) -> CompatibilityReport {
        let mut report = CompatibilityReport {
            entries: self
                .entries
                .into_iter()
                .map(
                    |(feature, (occurrences, model_outcome))| CompatibilityEntry {
                        feature: feature.to_owned(),
                        occurrences,
                        location: FeatureLocation {
                            local_name: Some(feature.to_owned()),
                            ..FeatureLocation::default()
                        },
                        model_outcome,
                        retention_outcome: RetentionOutcome::NotRetained,
                    },
                )
                .collect(),
        };
        report.sort();
        report
    }
}

fn append_blocks(
    blocks: &[BlockNode],
    output: &mut TextOutput,
    losses: &mut Losses,
) -> Result<(), AdapterError> {
    for (index, block) in blocks.iter().enumerate() {
        if index != 0 {
            output.push('\n')?;
        }
        append_block(block, output, losses)?;
    }
    Ok(())
}

fn append_block(
    block: &BlockNode,
    output: &mut TextOutput,
    losses: &mut Losses,
) -> Result<(), AdapterError> {
    match block {
        BlockNode::Paragraph(paragraph) => {
            if paragraph.properties != ParagraphProperties::default() {
                losses.record("plain_text.paragraph_formatting", ModelOutcome::Omitted);
            }
            append_inlines(&paragraph.inlines, output, losses)
        }
        BlockNode::Table(table) => {
            losses.record("plain_text.table_structure", ModelOutcome::Degraded);
            for (row_index, row) in table.rows.iter().enumerate() {
                if row_index != 0 {
                    output.push('\n')?;
                }
                for (cell_index, cell) in row.cells.iter().enumerate() {
                    if cell_index != 0 {
                        output.push('\t')?;
                    }
                    append_blocks(&cell.blocks, output, losses)?;
                }
            }
            Ok(())
        }
        BlockNode::Sdt(sdt) => {
            losses.record("plain_text.content_control", ModelOutcome::Degraded);
            append_blocks(&sdt.blocks, output, losses)
        }
        BlockNode::AltChunk(_) => {
            losses.record("plain_text.alt_chunk", ModelOutcome::Omitted);
            Ok(())
        }
    }
}

fn append_inlines(
    inlines: &[InlineNode],
    output: &mut TextOutput,
    losses: &mut Losses,
) -> Result<(), AdapterError> {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => {
                if run.properties != RunProperties::default() {
                    losses.record("plain_text.run_formatting", ModelOutcome::Omitted);
                }
                output.push_text(&run.text)?;
            }
            InlineNode::Tab(_) => output.push('\t')?,
            InlineNode::Break(node) => {
                output.push('\n')?;
                if node.kind != BreakKind::Line {
                    losses.record("plain_text.page_or_column_break", ModelOutcome::Degraded);
                }
            }
            InlineNode::Drawing(drawing) => append_drawing_alt(
                drawing.descr.as_deref(),
                "plain_text.drawing",
                output,
                losses,
            )?,
            InlineNode::AnchoredDrawing(drawing) => append_drawing_alt(
                drawing.descr.as_deref(),
                "plain_text.anchored_drawing",
                output,
                losses,
            )?,
            InlineNode::EmbeddedObject(_) => {
                losses.record("plain_text.embedded_object", ModelOutcome::Omitted);
            }
            InlineNode::Hyperlink(link) => {
                losses.record("plain_text.hyperlink_target", ModelOutcome::Degraded);
                append_inlines(&link.inlines, output, losses)?;
            }
            InlineNode::Field(field) => {
                losses.record("plain_text.field_instruction", ModelOutcome::Degraded);
                append_inlines(&field.inlines, output, losses)?;
            }
            InlineNode::TextBox(text_box) => {
                losses.record("plain_text.text_box", ModelOutcome::Degraded);
                append_blocks(&text_box.blocks, output, losses)?;
            }
            InlineNode::Group(group) => append_group(group, output, losses)?,
            InlineNode::NoteReference(_) => {
                losses.record("plain_text.note_reference", ModelOutcome::Omitted);
            }
            InlineNode::CommentReference(_)
            | InlineNode::CommentRangeStart(_)
            | InlineNode::CommentRangeEnd(_)
            | InlineNode::BookmarkStart(_)
            | InlineNode::BookmarkEnd(_)
            | InlineNode::MoveRangeStart(_)
            | InlineNode::MoveRangeEnd(_) => {
                losses.record("plain_text.range_or_comment_marker", ModelOutcome::Omitted);
            }
            InlineNode::Revision(revision) => {
                losses.record("plain_text.revision", ModelOutcome::Degraded);
                if matches!(
                    revision.kind,
                    RevisionKind::Insertion | RevisionKind::MoveTo
                ) {
                    append_inlines(&revision.inlines, output, losses)?;
                }
            }
            InlineNode::Sdt(sdt) => {
                losses.record("plain_text.content_control", ModelOutcome::Degraded);
                append_inlines(&sdt.inlines, output, losses)?;
            }
            InlineNode::Math(math) => {
                losses.record("plain_text.math_markup", ModelOutcome::Degraded);
                output.push_text(&math.text)?;
            }
            InlineNode::Symbol(symbol) => {
                losses.record("plain_text.symbol_font", ModelOutcome::Degraded);
                if let Some(character) = char::from_u32(symbol.char) {
                    output.push(character)?;
                }
            }
            InlineNode::HorizontalRule(_) => {
                losses.record("plain_text.horizontal_rule", ModelOutcome::Omitted);
            }
            InlineNode::NoBreakHyphen(_) => output.push('\u{2011}')?,
            InlineNode::SoftHyphen(_) => output.push('\u{00ad}')?,
            InlineNode::PositionalTab(_) => {
                losses.record("plain_text.positional_tab", ModelOutcome::Degraded);
                output.push('\t')?;
            }
        }
    }
    Ok(())
}

fn append_drawing_alt(
    description: Option<&str>,
    feature: &'static str,
    output: &mut TextOutput,
    losses: &mut Losses,
) -> Result<(), AdapterError> {
    if let Some(description) = description {
        losses.record(feature, ModelOutcome::Degraded);
        output.push_text(description)
    } else {
        losses.record(feature, ModelOutcome::Omitted);
        Ok(())
    }
}

fn append_group(
    group: &casual_doc_model::v1::WordprocessingGroup,
    output: &mut TextOutput,
    losses: &mut Losses,
) -> Result<(), AdapterError> {
    losses.record("plain_text.group_structure", ModelOutcome::Degraded);
    let mut wrote_text = false;
    for child in &group.children {
        match child {
            GroupChild::TextBox(text_box) => {
                if wrote_text {
                    output.push('\n')?;
                }
                append_blocks(&text_box.blocks, output, losses)?;
                wrote_text = true;
            }
            GroupChild::Group(group) => {
                if wrote_text {
                    output.push('\n')?;
                }
                append_group(group, output, losses)?;
                wrote_text = true;
            }
            GroupChild::Picture(_) | GroupChild::Shape(_) => {
                losses.record("plain_text.group_non_text_child", ModelOutcome::Omitted);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DetectionRequest, FormatRegistry, FormatSelection, builtin_registry};

    #[test]
    fn import_normalizes_newlines_bom_and_tabs_with_stable_ids() {
        let adapter = PlainTextAdapter::default();
        let original = b"\xef\xbb\xbffirst\r\n\tsecond\rlast\n";
        let imported = adapter
            .import(ImportRequest {
                bytes: original,
                retain_source: true,
            })
            .unwrap();
        let canonical = adapter
            .import(ImportRequest {
                bytes: b"first\n\tsecond\nlast\n",
                retain_source: false,
            })
            .unwrap();
        assert_eq!(imported.document, canonical.document);
        assert_eq!(imported.document.body().len(), 4);

        let semantic = adapter
            .export(ExportRequest {
                document: &imported.document,
                resources: &imported.resources,
                source: Some(&imported.source),
                source_unchanged: true,
                mode: ExportMode::Semantic,
            })
            .unwrap();
        assert_eq!(semantic.bytes, b"first\n\tsecond\nlast\n");
        assert!(semantic.report.entries.is_empty());
        let exact = adapter
            .export(ExportRequest {
                document: &imported.document,
                resources: &imported.resources,
                source: Some(&imported.source),
                source_unchanged: true,
                mode: ExportMode::ExactIfUnchanged,
            })
            .unwrap();
        assert_eq!(exact.bytes, original);
    }

    #[test]
    fn invalid_utf8_controls_and_limits_fail_closed() {
        let adapter = PlainTextAdapter::default();
        for bytes in [b"bad\0text".as_slice(), &[0xff_u8][..]] {
            assert_eq!(
                adapter.probe(ProbeRequest { bytes }),
                ProbeResult::no_match("text.invalid-or-over-limit")
            );
            assert!(
                adapter
                    .import(ImportRequest {
                        bytes,
                        retain_source: false,
                    })
                    .is_err()
            );
        }

        let limited = PlainTextAdapter::new(PlainTextLimits {
            max_paragraphs: 1,
            ..PlainTextLimits::default()
        });
        assert!(
            limited
                .import(ImportRequest {
                    bytes: b"one\ntwo",
                    retain_source: false,
                })
                .is_err()
        );
        let invalid_configuration = PlainTextAdapter::new(PlainTextLimits {
            max_input_bytes: usize::MAX,
            ..PlainTextLimits::default()
        });
        assert!(
            invalid_configuration
                .import(ImportRequest {
                    bytes: b"text",
                    retain_source: false,
                })
                .is_err()
        );
    }

    #[test]
    fn valid_json_wins_over_the_possible_text_probe() {
        let registry = builtin_registry();
        let text = PlainTextAdapter::default()
            .import(ImportRequest {
                bytes: b"hello",
                retain_source: false,
            })
            .unwrap();
        let json = text.document.to_json().unwrap();
        let detected = registry
            .detect(DetectionRequest {
                bytes: &json,
                selection: FormatSelection::Auto,
                file_name_hint: Some("misleading.txt"),
                mime_hint: Some(TEXT_MIME),
            })
            .unwrap();
        assert_eq!(detected.as_str(), formats::NORMALIZED_JSON);
    }

    #[test]
    fn text_export_reports_formatting_structure_and_cross_format_loss() {
        let registry = builtin_registry();
        let docx = include_bytes!("../../../fixtures/corpus/real-producer-table-list.docx");
        let imported = registry
            .import(
                DetectionRequest {
                    bytes: docx,
                    selection: FormatSelection::Auto,
                    file_name_hint: None,
                    mime_hint: None,
                },
                true,
            )
            .unwrap();
        let exported = registry
            .export(
                &FormatId::new(formats::TEXT).unwrap(),
                ExportRequest {
                    document: &imported.document,
                    resources: &imported.resources,
                    source: Some(&imported.source),
                    source_unchanged: true,
                    mode: ExportMode::Semantic,
                },
            )
            .unwrap();
        assert!(std::str::from_utf8(&exported.bytes).is_ok());
        let features = exported
            .report
            .entries
            .iter()
            .map(|entry| entry.feature.as_str())
            .collect::<Vec<_>>();
        assert!(features.contains(&"plain_text.table_structure"));
        assert!(features.contains(&"plain_text.source_envelope"));
        assert!(features.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn output_limit_is_enforced_without_partial_artifact() {
        let importer = PlainTextAdapter::default();
        let imported = importer
            .import(ImportRequest {
                bytes: b"four",
                retain_source: true,
            })
            .unwrap();
        let exporter = PlainTextAdapter::new(PlainTextLimits {
            max_output_bytes: 3,
            ..PlainTextLimits::default()
        });
        assert!(
            exporter
                .export(ExportRequest {
                    document: &imported.document,
                    resources: &imported.resources,
                    source: None,
                    source_unchanged: false,
                    mode: ExportMode::Semantic,
                })
                .is_err()
        );
        assert!(
            exporter
                .export(ExportRequest {
                    document: &imported.document,
                    resources: &imported.resources,
                    source: Some(&imported.source),
                    source_unchanged: true,
                    mode: ExportMode::ExactIfUnchanged,
                })
                .is_err()
        );
    }

    #[test]
    fn semantic_export_canonicalizes_embedded_carriage_returns() {
        let adapter = PlainTextAdapter::default();
        let mut imported = adapter
            .import(ImportRequest {
                bytes: b"source",
                retain_source: false,
            })
            .unwrap();
        let BlockNode::Paragraph(paragraph) = &mut imported.document.body_mut()[0] else {
            panic!("plain text creates a paragraph");
        };
        let InlineNode::Run(run) = &mut paragraph.inlines[0] else {
            panic!("plain text creates a run");
        };
        run.text = "one\r\ntwo\rthree".to_owned();
        let exported = adapter
            .export(ExportRequest {
                document: &imported.document,
                resources: &imported.resources,
                source: None,
                source_unchanged: false,
                mode: ExportMode::Semantic,
            })
            .unwrap();
        assert_eq!(exported.bytes, b"one\ntwo\nthree");
    }

    #[test]
    fn builtins_are_capability_sorted_for_all_three_formats() {
        let registry: FormatRegistry = builtin_registry();
        assert_eq!(
            registry
                .export_formats()
                .into_iter()
                .map(FormatId::as_str)
                .collect::<Vec<_>>(),
            vec![formats::NORMALIZED_JSON, formats::DOCX, formats::TEXT]
        );
    }
}
