//! Bounded, namespace-aware semantic import of ODT `content.xml`.

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{
    AbstractNumbering, AbstractNumberingId, Alignment, BlockNode, Bookmark, BookmarkEnd,
    BookmarkId, BookmarkStart, Break, BreakKind, Color, Definitions, Document, ExternalTarget,
    GridColumn, Hyperlink, HyperlinkTarget, InlineNode, InternalTarget, LevelJustification,
    LevelSuffix, MAX_TABLE_DEPTH, Note, NoteId, NoteKind, NoteReference, NumberFormat,
    NumberingInstance, NumberingInstanceId, NumberingLevel, NumberingRef, Paragraph,
    ParagraphProperties, RgbColor, Run, RunProperties, Tab, Table, TableCell, TableCellProperties,
    TableProperties, TableRow, TableRowProperties, VerticalMerge,
};
use casual_doc_package::CancellationToken;
use quick_xml::NsReader;
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::name::{Namespace, ResolveResult};

use crate::{OdfError, OdfVersion};

const OFFICE_NS: &[u8] = b"urn:oasis:names:tc:opendocument:xmlns:office:1.0";
const TEXT_NS: &[u8] = b"urn:oasis:names:tc:opendocument:xmlns:text:1.0";
const SCRIPT_NS: &[u8] = b"urn:oasis:names:tc:opendocument:xmlns:script:1.0";
const XLINK_NS: &[u8] = b"http://www.w3.org/1999/xlink";
const STYLE_NS: &[u8] = b"urn:oasis:names:tc:opendocument:xmlns:style:1.0";
const FO_NS: &[u8] = b"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0";
const TABLE_NS: &[u8] = b"urn:oasis:names:tc:opendocument:xmlns:table:1.0";
const MAX_NAMESPACE_DECLARATIONS_PER_ELEMENT: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NamespaceKind {
    Office,
    Text,
    Script,
    Xlink,
    Style,
    Fo,
    Table,
    Foreign,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedName {
    namespace: NamespaceKind,
    local: Vec<u8>,
}

/// How a source construct was represented in the normalized model.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ModelOutcome {
    /// The source construct was fully represented.
    Mapped,
    /// Some, but not all, source semantics were represented.
    Degraded,
    /// The source construct was not represented.
    Omitted,
}

/// What happened to source detail not represented by the model.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RetentionOutcome {
    /// Unconsumed source detail was retained in validated sidecar state.
    Preserved,
    /// The semantic-only content entry point did not retain source detail.
    NotRetained,
    /// Source detail was refused by security or host policy.
    Blocked,
    /// Source detail was invalid or over limit.
    Rejected,
    /// The construct had no unconsumed remainder.
    NotApplicable,
}

/// One aggregated, deterministic ODT compatibility finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilityEntry {
    /// Stable feature identifier using canonical namespace labels.
    pub feature: String,
    /// Saturating occurrence count.
    pub occurrences: u32,
    /// Semantic mapping result.
    pub model_outcome: ModelOutcome,
    /// Preservation result for unconsumed source detail.
    pub retention_outcome: RetentionOutcome,
}

/// Deterministically ordered compatibility findings.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompatibilityReport {
    /// Findings sorted by feature and outcome.
    pub entries: Vec<CompatibilityEntry>,
}

/// Successful atomic ODT semantic import.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OdtImport {
    /// Validated normalized schema-v1 document.
    pub document: Document,
    /// Explicit findings for deferred or unrepresented source constructs.
    pub report: CompatibilityReport,
}

/// Resource limits for ODT semantic content import.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OdfImportLimits {
    /// Maximum expanded `content.xml` bytes.
    pub max_content_bytes: usize,
    /// Maximum expanded optional `styles.xml` bytes.
    pub max_styles_bytes: usize,
    /// Maximum XML element nesting depth.
    pub max_xml_depth: usize,
    /// Maximum XML elements.
    pub max_xml_elements: usize,
    /// Maximum XML attributes.
    pub max_xml_attributes: usize,
    /// Maximum aggregate raw attribute-value bytes.
    pub max_xml_attribute_bytes: usize,
    /// Maximum bytes in one XML qualified name.
    pub max_xml_name_bytes: usize,
    /// Maximum normalized paragraphs.
    pub max_paragraphs: usize,
    /// Maximum normalized inline nodes.
    pub max_inline_nodes: usize,
    /// Maximum source list elements.
    pub max_lists: usize,
    /// Maximum nested source list depth.
    pub max_list_depth: usize,
    /// Maximum normalized tables, including nested tables.
    pub max_tables: usize,
    /// Maximum expanded table rows across all tables.
    pub max_table_rows: usize,
    /// Maximum expanded table cells, including covered cells.
    pub max_table_cells: usize,
    /// Maximum nested table depth.
    pub max_table_depth: usize,
    /// Maximum footnote and endnote definitions.
    pub max_notes: usize,
    /// Maximum aggregate normalized text bytes.
    pub max_text_bytes: usize,
    /// Maximum spaces emitted by one `text:s` element.
    pub max_space_repeat: usize,
    /// Maximum distinct source-feature buckets before folding into one overflow entry.
    pub max_report_features: usize,
}

impl OdfImportLimits {
    /// Compiled maximum expanded `content.xml` bytes.
    pub const HARD_MAX_CONTENT_BYTES: usize = 512 * 1024 * 1024;
    /// Compiled maximum expanded `styles.xml` bytes.
    pub const HARD_MAX_STYLES_BYTES: usize = 512 * 1024 * 1024;
    /// Compiled maximum XML nesting depth.
    pub const HARD_MAX_XML_DEPTH: usize = 256;
    /// Compiled maximum XML element count.
    pub const HARD_MAX_XML_ELEMENTS: usize = 8_000_000;
    /// Compiled maximum XML attribute count.
    pub const HARD_MAX_XML_ATTRIBUTES: usize = 24_000_000;
    /// Compiled maximum aggregate raw attribute-value bytes.
    pub const HARD_MAX_XML_ATTRIBUTE_BYTES: usize = 512 * 1024 * 1024;
    /// Compiled maximum XML qualified-name bytes.
    pub const HARD_MAX_XML_NAME_BYTES: usize = 4_096;
    /// Compiled maximum paragraph count.
    pub const HARD_MAX_PARAGRAPHS: usize = 2_000_000;
    /// Compiled maximum inline-node count.
    pub const HARD_MAX_INLINE_NODES: usize = 16_000_000;
    /// Compiled maximum source list count.
    pub const HARD_MAX_LISTS: usize = 2_000_000;
    /// Compiled maximum nested source list depth.
    pub const HARD_MAX_LIST_DEPTH: usize = 256;
    /// Compiled maximum table count.
    pub const HARD_MAX_TABLES: usize = 1_000_000;
    /// Compiled maximum expanded table-row count.
    pub const HARD_MAX_TABLE_ROWS: usize = 2_000_000;
    /// Compiled maximum expanded table-cell count.
    pub const HARD_MAX_TABLE_CELLS: usize = 16_000_000;
    /// Compiled maximum nested table depth, aligned with the normalized model.
    pub const HARD_MAX_TABLE_DEPTH: usize = MAX_TABLE_DEPTH as usize;
    /// Compiled maximum footnote and endnote count.
    pub const HARD_MAX_NOTES: usize = 2_000_000;
    /// Compiled maximum aggregate normalized text bytes.
    pub const HARD_MAX_TEXT_BYTES: usize = 512 * 1024 * 1024;
    /// Compiled maximum one-element space expansion.
    pub const HARD_MAX_SPACE_REPEAT: usize = 1_000_000;
    /// Compiled maximum distinct source-feature buckets before overflow folding.
    pub const HARD_MAX_REPORT_FEATURES: usize = 16_384;

    fn validate(self) -> Result<(), OdfError> {
        for (limit, value, hard_ceiling) in [
            (
                "odf_content_bytes",
                self.max_content_bytes,
                Self::HARD_MAX_CONTENT_BYTES,
            ),
            (
                "odf_styles_bytes",
                self.max_styles_bytes,
                Self::HARD_MAX_STYLES_BYTES,
            ),
            (
                "odf_content_xml_depth",
                self.max_xml_depth,
                Self::HARD_MAX_XML_DEPTH,
            ),
            (
                "odf_content_xml_elements",
                self.max_xml_elements,
                Self::HARD_MAX_XML_ELEMENTS,
            ),
            (
                "odf_content_xml_attributes",
                self.max_xml_attributes,
                Self::HARD_MAX_XML_ATTRIBUTES,
            ),
            (
                "odf_content_xml_attribute_bytes",
                self.max_xml_attribute_bytes,
                Self::HARD_MAX_XML_ATTRIBUTE_BYTES,
            ),
            (
                "odf_content_xml_name_bytes",
                self.max_xml_name_bytes,
                Self::HARD_MAX_XML_NAME_BYTES,
            ),
            (
                "odf_content_paragraphs",
                self.max_paragraphs,
                Self::HARD_MAX_PARAGRAPHS,
            ),
            (
                "odf_content_inline_nodes",
                self.max_inline_nodes,
                Self::HARD_MAX_INLINE_NODES,
            ),
            ("odf_content_lists", self.max_lists, Self::HARD_MAX_LISTS),
            (
                "odf_content_list_depth",
                self.max_list_depth,
                Self::HARD_MAX_LIST_DEPTH,
            ),
            ("odf_content_tables", self.max_tables, Self::HARD_MAX_TABLES),
            (
                "odf_content_table_rows",
                self.max_table_rows,
                Self::HARD_MAX_TABLE_ROWS,
            ),
            (
                "odf_content_table_cells",
                self.max_table_cells,
                Self::HARD_MAX_TABLE_CELLS,
            ),
            (
                "odf_content_table_depth",
                self.max_table_depth,
                Self::HARD_MAX_TABLE_DEPTH,
            ),
            ("odf_content_notes", self.max_notes, Self::HARD_MAX_NOTES),
            (
                "odf_content_text_bytes",
                self.max_text_bytes,
                Self::HARD_MAX_TEXT_BYTES,
            ),
            (
                "odf_content_space_repeat",
                self.max_space_repeat,
                Self::HARD_MAX_SPACE_REPEAT,
            ),
            (
                "odf_content_report_features",
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

impl Default for OdfImportLimits {
    fn default() -> Self {
        Self {
            max_content_bytes: 128 * 1024 * 1024,
            max_styles_bytes: 64 * 1024 * 1024,
            max_xml_depth: 96,
            max_xml_elements: 1_000_000,
            max_xml_attributes: 3_000_000,
            max_xml_attribute_bytes: 128 * 1024 * 1024,
            max_xml_name_bytes: 1_024,
            max_paragraphs: 250_000,
            max_inline_nodes: 2_000_000,
            max_lists: 250_000,
            max_list_depth: 64,
            max_tables: 100_000,
            max_table_rows: 1_000_000,
            max_table_cells: 4_000_000,
            max_table_depth: 16,
            max_notes: 250_000,
            max_text_bytes: 128 * 1024 * 1024,
            max_space_repeat: 65_536,
            max_report_features: 4_096,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum InlineDraft {
    Text {
        text: String,
        properties: Box<RunProperties>,
    },
    Tab,
    LineBreak,
    Hyperlink {
        target: HyperlinkTarget,
        inlines: Vec<InlineDraft>,
    },
    BookmarkStart(usize),
    BookmarkEnd(usize),
    NoteReference {
        index: usize,
        kind: NoteKind,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LinkDraft {
    depth: usize,
    target: Option<HyperlinkTarget>,
    inlines: Vec<InlineDraft>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BookmarkDraft {
    name: String,
    paired: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParagraphDraft {
    depth: usize,
    outline_level: Option<u8>,
    alignment: Option<Alignment>,
    numbering: Option<ListParagraphDraft>,
    inlines: Vec<InlineDraft>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum BlockDraft {
    Paragraph(usize),
    Table(TableDraft),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TableDraft {
    columns: usize,
    rows: Vec<TableRowDraft>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TableRowDraft {
    header: bool,
    slots: Vec<TableSlotDraft>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TableSlotDraft {
    Cell(TableCellDraft),
    Covered,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TableCellDraft {
    column_span: u32,
    row_span: u32,
    blocks: Vec<BlockDraft>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NoteDraft {
    kind: NoteKind,
    blocks: Vec<BlockDraft>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SuspendedInlineContext {
    paragraph: ParagraphDraft,
    active_link: Option<LinkDraft>,
    active_run_properties: RunProperties,
    open_spans: Vec<OpenSpan>,
    open_lists: Vec<OpenList>,
    open_list_items: Vec<OpenListItem>,
    open_bookmarks: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpenNote {
    depth: usize,
    body_depth: Option<usize>,
    body_seen: bool,
    citation_depth: Option<usize>,
    citation_has_text: bool,
    kind: NoteKind,
    blocks: Vec<BlockDraft>,
    table_stack_len: usize,
    outer: SuspendedInlineContext,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpenTable {
    depth: usize,
    columns: usize,
    rows: Vec<TableRowDraft>,
    row_container_depth: Option<usize>,
    row_container_header: bool,
    current_row: Option<OpenTableRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpenTableRow {
    depth: usize,
    repeat: usize,
    header: bool,
    slots: Vec<TableSlotDraft>,
    current_cell: Option<OpenTableCell>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpenTableCell {
    depth: usize,
    repeat: usize,
    column_span: u32,
    row_span: u32,
    blocks: Vec<BlockDraft>,
}

type TableCoordinate = (usize, usize);
type TableOwners = BTreeMap<TableCoordinate, TableCoordinate>;

#[derive(Clone, Debug, Eq, PartialEq)]
struct ListParagraphDraft {
    instance: usize,
    level: u8,
    style_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpenList {
    depth: usize,
    instance: usize,
    level: u8,
    style_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpenListItem {
    depth: usize,
    first_paragraph: bool,
}

impl ParagraphDraft {
    fn push_text(&mut self, value: &str, properties: &RunProperties) {
        push_text_draft(&mut self.inlines, value, properties);
    }
}

fn push_text_draft(inlines: &mut Vec<InlineDraft>, value: &str, properties: &RunProperties) {
    if value.is_empty() {
        return;
    }
    if let Some(InlineDraft::Text {
        text: previous,
        properties: previous_properties,
    }) = inlines.last_mut()
        && previous_properties.as_ref() == properties
    {
        previous.push_str(value);
    } else {
        inlines.push(InlineDraft::Text {
            text: value.to_owned(),
            properties: Box::new(properties.clone()),
        });
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum StyleFamily {
    Paragraph,
    Text,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct OdfStyle {
    family: Option<StyleFamily>,
    parent: Option<String>,
    alignment: Option<Alignment>,
    run_properties: RunProperties,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpenStyle {
    depth: usize,
    name: String,
    style: OdfStyle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OdfListLevel {
    level: u8,
    num_fmt: NumberFormat,
    lvl_text: String,
    start: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpenListStyle {
    depth: usize,
    name: String,
    levels: BTreeMap<u8, OdfListLevel>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpenSpan {
    depth: usize,
    previous_properties: RunProperties,
}

#[derive(Debug)]
struct Reporter {
    counts: BTreeMap<(String, ModelOutcome, RetentionOutcome), u32>,
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

    fn report(&mut self, feature: String, outcome: ModelOutcome) {
        self.report_with_retention(feature, outcome, RetentionOutcome::NotRetained);
    }

    fn report_with_retention(
        &mut self,
        feature: String,
        outcome: ModelOutcome,
        retention: RetentionOutcome,
    ) {
        let key = (feature, outcome, retention);
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
                |((feature, model_outcome, retention_outcome), occurrences)| CompatibilityEntry {
                    feature,
                    occurrences,
                    model_outcome,
                    retention_outcome,
                },
            )
            .collect::<Vec<_>>();
        if self.overflow != 0 {
            entries.push(CompatibilityEntry {
                feature: "odf.report.overflow".to_owned(),
                occurrences: self.overflow,
                model_outcome: ModelOutcome::Omitted,
                retention_outcome: RetentionOutcome::NotRetained,
            });
        }
        entries.sort_by(|left, right| {
            left.feature
                .cmp(&right.feature)
                .then_with(|| left.model_outcome.cmp(&right.model_outcome))
                .then_with(|| left.retention_outcome.cmp(&right.retention_outcome))
        });
        CompatibilityReport { entries }
    }
}

type AutomaticStyles = BTreeMap<(StyleFamily, String), OdfStyle>;
type ListStyles = BTreeMap<String, BTreeMap<u8, OdfListLevel>>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct StyleCatalog {
    automatic: AutomaticStyles,
    lists: ListStyles,
}

#[allow(clippy::too_many_arguments)]
fn parse_style_catalog(
    content: &[u8],
    styles_part: Option<&[u8]>,
    expected_version: OdfVersion,
    limits: OdfImportLimits,
    cancellation: &CancellationToken,
    reporter: &mut Reporter,
) -> Result<StyleCatalog, OdfError> {
    let mut catalog = if let Some(styles_part) = styles_part {
        parse_style_document(
            styles_part,
            b"document-styles",
            true,
            expected_version,
            limits,
            cancellation,
            reporter,
        )?
    } else {
        StyleCatalog::default()
    };
    let content_styles = parse_style_document(
        content,
        b"document-content",
        false,
        expected_version,
        limits,
        cancellation,
        reporter,
    )?;
    for (key, style) in content_styles.automatic {
        if catalog.automatic.insert(key, style).is_some() {
            reporter.report("odf.style.shadowed".to_owned(), ModelOutcome::Degraded);
        }
    }
    for (name, list_style) in content_styles.lists {
        if catalog.lists.insert(name, list_style).is_some() {
            reporter.report("odf.list-style.shadowed".to_owned(), ModelOutcome::Degraded);
        }
    }
    catalog.automatic = resolve_style_inheritance(&catalog.automatic, limits, reporter)?;
    Ok(catalog)
}

#[allow(clippy::too_many_arguments)]
fn parse_style_document(
    bytes: &[u8],
    expected_root: &[u8],
    include_common_styles: bool,
    expected_version: OdfVersion,
    limits: OdfImportLimits,
    cancellation: &CancellationToken,
    reporter: &mut Reporter,
) -> Result<StyleCatalog, OdfError> {
    let mut reader = NsReader::from_reader(bytes);
    reader
        .resolver_mut()
        .set_max_declarations_per_element(MAX_NAMESPACE_DECLARATIONS_PER_ELEMENT);
    let mut buffer = Vec::new();
    let mut depth = 0_usize;
    let mut elements = 0_usize;
    let mut attributes = 0_usize;
    let mut attribute_bytes = 0_usize;
    let mut style_container_depth = None;
    let mut automatic_seen = false;
    let mut common_seen = false;
    let mut root_seen = false;
    let mut root_closed = false;
    let mut open_style: Option<OpenStyle> = None;
    let mut open_list_style: Option<OpenListStyle> = None;
    let mut styles = AutomaticStyles::new();
    let mut list_styles = ListStyles::new();

    loop {
        check_cancelled(cancellation)?;
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| OdfError::MalformedContent)?;
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err(OdfError::MalformedContent),
            Event::Start(element) => {
                depth = depth.checked_add(1).ok_or(OdfError::MalformedContent)?;
                enforce("odf_content_xml_depth", depth, limits.max_xml_depth)?;
                elements = checked_increment(elements)?;
                enforce(
                    "odf_content_xml_elements",
                    elements,
                    limits.max_xml_elements,
                )?;
                validate_name(&element, limits)?;
                let name = resolved_name(&reader, &element);
                if is_active(&name) {
                    return Err(OdfError::ActiveContent);
                }
                if depth == 1 {
                    if root_seen
                        || root_closed
                        || !is_name(&name, NamespaceKind::Office, expected_root)
                    {
                        return Err(OdfError::MalformedContent);
                    }
                    root_seen = true;
                    if read_version(
                        &reader,
                        &element,
                        &mut attributes,
                        &mut attribute_bytes,
                        limits,
                        reporter,
                    )? != expected_version
                    {
                        return Err(OdfError::ManifestMismatch);
                    }
                } else if depth == 2 && is_name(&name, NamespaceKind::Office, b"automatic-styles") {
                    if automatic_seen {
                        return Err(OdfError::MalformedContent);
                    }
                    automatic_seen = true;
                    style_container_depth = Some(depth);
                    count_attributes_only(&element, &mut attributes, &mut attribute_bytes, limits)?;
                } else if depth == 2
                    && include_common_styles
                    && is_name(&name, NamespaceKind::Office, b"styles")
                {
                    if common_seen {
                        return Err(OdfError::MalformedContent);
                    }
                    common_seen = true;
                    style_container_depth = Some(depth);
                    count_attributes_only(&element, &mut attributes, &mut attribute_bytes, limits)?;
                } else if style_container_depth.is_some() {
                    process_style_element(
                        &reader,
                        &element,
                        &name,
                        depth,
                        limits,
                        &mut attributes,
                        &mut attribute_bytes,
                        &mut open_style,
                        &mut styles,
                        &mut open_list_style,
                        &mut list_styles,
                        false,
                        reporter,
                    )?;
                } else {
                    count_attributes_only(&element, &mut attributes, &mut attribute_bytes, limits)?;
                }
            }
            Event::Empty(element) => {
                elements = checked_increment(elements)?;
                enforce(
                    "odf_content_xml_elements",
                    elements,
                    limits.max_xml_elements,
                )?;
                validate_name(&element, limits)?;
                let name = resolved_name(&reader, &element);
                if is_active(&name) {
                    return Err(OdfError::ActiveContent);
                }
                let event_depth = depth.checked_add(1).ok_or(OdfError::MalformedContent)?;
                if event_depth == 1 {
                    return Err(OdfError::MalformedContent);
                } else if event_depth == 2
                    && is_name(&name, NamespaceKind::Office, b"automatic-styles")
                {
                    if automatic_seen {
                        return Err(OdfError::MalformedContent);
                    }
                    automatic_seen = true;
                    count_attributes_only(&element, &mut attributes, &mut attribute_bytes, limits)?;
                } else if event_depth == 2
                    && include_common_styles
                    && is_name(&name, NamespaceKind::Office, b"styles")
                {
                    if common_seen {
                        return Err(OdfError::MalformedContent);
                    }
                    common_seen = true;
                    count_attributes_only(&element, &mut attributes, &mut attribute_bytes, limits)?;
                } else if style_container_depth.is_some() {
                    process_style_element(
                        &reader,
                        &element,
                        &name,
                        event_depth,
                        limits,
                        &mut attributes,
                        &mut attribute_bytes,
                        &mut open_style,
                        &mut styles,
                        &mut open_list_style,
                        &mut list_styles,
                        true,
                        reporter,
                    )?;
                } else {
                    count_attributes_only(&element, &mut attributes, &mut attribute_bytes, limits)?;
                }
            }
            Event::End(element) => {
                if open_style
                    .as_ref()
                    .is_some_and(|style: &OpenStyle| style.depth == depth)
                {
                    finish_style(&mut open_style, &mut styles)?;
                }
                if open_list_style
                    .as_ref()
                    .is_some_and(|style: &OpenListStyle| style.depth == depth)
                {
                    finish_list_style(&mut open_list_style, &mut list_styles)?;
                }
                if style_container_depth == Some(depth) {
                    style_container_depth = None;
                }
                if depth == 1 {
                    if element.local_name().as_ref() != expected_root {
                        return Err(OdfError::MalformedContent);
                    }
                    root_closed = true;
                }
                depth = depth.checked_sub(1).ok_or(OdfError::MalformedContent)?;
            }
            Event::Text(text) if style_container_depth.is_some() => {
                let decoded = text.decode().map_err(|_| OdfError::MalformedContent)?;
                let value = quick_xml::escape::unescape(&decoded)
                    .map_err(|_| OdfError::MalformedContent)?;
                if !value.trim().is_empty() {
                    return Err(OdfError::MalformedContent);
                }
            }
            Event::CData(text) => {
                let non_whitespace = !text
                    .decode()
                    .map_err(|_| OdfError::MalformedContent)?
                    .trim()
                    .is_empty();
                if style_container_depth.is_some() && non_whitespace {
                    return Err(OdfError::MalformedContent);
                }
            }
            _ => {}
        }
        buffer.clear();
    }
    if !root_seen
        || !root_closed
        || depth != 0
        || style_container_depth.is_some()
        || open_style.is_some()
        || open_list_style.is_some()
    {
        return Err(OdfError::MalformedContent);
    }
    Ok(StyleCatalog {
        automatic: styles,
        lists: list_styles,
    })
}

fn resolve_style_inheritance(
    raw: &AutomaticStyles,
    limits: OdfImportLimits,
    reporter: &mut Reporter,
) -> Result<AutomaticStyles, OdfError> {
    let mut resolved = AutomaticStyles::new();
    let mut visiting = BTreeSet::new();
    for key in raw.keys() {
        resolve_style(key, raw, &mut resolved, &mut visiting, 0, limits, reporter)?;
    }
    Ok(resolved)
}

#[allow(clippy::too_many_arguments)]
fn resolve_style(
    key: &(StyleFamily, String),
    raw: &AutomaticStyles,
    resolved: &mut AutomaticStyles,
    visiting: &mut BTreeSet<(StyleFamily, String)>,
    depth: usize,
    limits: OdfImportLimits,
    reporter: &mut Reporter,
) -> Result<OdfStyle, OdfError> {
    if let Some(style) = resolved.get(key) {
        return Ok(style.clone());
    }
    enforce("odf_style_inheritance_depth", depth, limits.max_xml_depth)?;
    let mut style = raw.get(key).cloned().ok_or(OdfError::MalformedContent)?;
    if !visiting.insert(key.clone()) {
        reporter.report(
            "odf.style.inheritance-cycle".to_owned(),
            ModelOutcome::Degraded,
        );
        style.parent = None;
        return Ok(style);
    }
    if let Some(parent_name) = style.parent.clone() {
        let parent_key = (key.0, parent_name);
        if raw.contains_key(&parent_key) {
            let mut inherited = resolve_style(
                &parent_key,
                raw,
                resolved,
                visiting,
                depth + 1,
                limits,
                reporter,
            )?;
            if style.alignment.is_some() {
                inherited.alignment = style.alignment;
            }
            merge_run_properties(&mut inherited.run_properties, &style.run_properties);
            inherited.family = style.family;
            inherited.parent = None;
            style = inherited;
        } else {
            reporter.report(
                "odf.style.unresolved-parent".to_owned(),
                ModelOutcome::Degraded,
            );
            style.parent = None;
        }
    }
    visiting.remove(key);
    resolved.insert(key.clone(), style.clone());
    Ok(style)
}

#[allow(clippy::too_many_arguments)]
fn process_style_element(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    name: &ResolvedName,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    open_style: &mut Option<OpenStyle>,
    styles: &mut AutomaticStyles,
    open_list_style: &mut Option<OpenListStyle>,
    list_styles: &mut ListStyles,
    empty: bool,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    if depth == 3 && is_name(name, NamespaceKind::Style, b"style") {
        if open_style.is_some() || open_list_style.is_some() {
            return Err(OdfError::MalformedContent);
        }
        *open_style = Some(read_style_header(
            reader,
            element,
            depth,
            limits,
            attributes,
            attribute_bytes,
            reporter,
        )?);
        if empty {
            finish_style(open_style, styles)?;
        }
    } else if depth == 3 && is_name(name, NamespaceKind::Text, b"list-style") {
        if open_style.is_some() || open_list_style.is_some() {
            return Err(OdfError::MalformedContent);
        }
        *open_list_style = Some(read_list_style_header(
            reader,
            element,
            depth,
            limits,
            attributes,
            attribute_bytes,
            reporter,
        )?);
        if empty {
            finish_list_style(open_list_style, list_styles)?;
        }
    } else if let Some(list_style) = open_list_style {
        if depth == list_style.depth + 1
            && (is_name(name, NamespaceKind::Text, b"list-level-style-bullet")
                || is_name(name, NamespaceKind::Text, b"list-level-style-number"))
        {
            let level = read_list_level(
                reader,
                element,
                name,
                limits,
                attributes,
                attribute_bytes,
                reporter,
            )?;
            if list_style.levels.insert(level.level, level).is_some() {
                return Err(OdfError::MalformedContent);
            }
        } else {
            count_and_report_attributes(
                reader,
                element,
                attributes,
                attribute_bytes,
                limits,
                reporter,
            )?;
            reporter.report(feature("element", name), ModelOutcome::Degraded);
        }
    } else if let Some(style) = open_style {
        if depth != style.depth + 1 {
            return Err(OdfError::MalformedContent);
        }
        if is_name(name, NamespaceKind::Style, b"text-properties") {
            read_text_style_properties(
                reader,
                element,
                limits,
                attributes,
                attribute_bytes,
                style,
                reporter,
            )?;
        } else if is_name(name, NamespaceKind::Style, b"paragraph-properties") {
            read_paragraph_style_properties(
                reader,
                element,
                limits,
                attributes,
                attribute_bytes,
                style,
                reporter,
            )?;
        } else {
            count_and_report_attributes(
                reader,
                element,
                attributes,
                attribute_bytes,
                limits,
                reporter,
            )?;
            reporter.report(feature("element", name), ModelOutcome::Degraded);
        }
    } else {
        count_and_report_attributes(
            reader,
            element,
            attributes,
            attribute_bytes,
            limits,
            reporter,
        )?;
        reporter.report(feature("element", name), ModelOutcome::Degraded);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn read_list_style_header(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    reporter: &mut Reporter,
) -> Result<OpenListStyle, OdfError> {
    let mut style_name = None;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        if namespace_kind(&namespace) == NamespaceKind::Style && local.as_ref() == b"name" {
            if style_name.is_some() {
                return Err(OdfError::MalformedContent);
            }
            style_name = Some(decode_attribute(&attribute)?);
        } else if !is_namespace_declaration(&attribute) {
            reporter.report(
                attribute_feature(reader, &attribute),
                ModelOutcome::Degraded,
            );
        }
    }
    let name = style_name.ok_or(OdfError::MalformedContent)?;
    if name.is_empty() {
        return Err(OdfError::MalformedContent);
    }
    Ok(OpenListStyle {
        depth,
        name,
        levels: BTreeMap::new(),
    })
}

#[allow(clippy::too_many_arguments)]
fn read_list_level(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    element_name: &ResolvedName,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    reporter: &mut Reporter,
) -> Result<OdfListLevel, OdfError> {
    let bullet = element_name.local == b"list-level-style-bullet";
    let mut source_level = None;
    let mut bullet_char = None;
    let mut num_format = None;
    let mut prefix = String::new();
    let mut suffix = String::new();
    let mut start = 1_u16;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        let namespace = namespace_kind(&namespace);
        match (namespace, local.as_ref()) {
            (NamespaceKind::Text, b"level") => {
                if source_level.is_some() {
                    return Err(OdfError::MalformedContent);
                }
                source_level = Some(
                    decode_attribute(&attribute)?
                        .parse::<u16>()
                        .map_err(|_| OdfError::MalformedContent)?,
                );
            }
            (NamespaceKind::Text, b"bullet-char") if bullet => {
                bullet_char = Some(decode_attribute(&attribute)?);
            }
            (NamespaceKind::Style, b"num-format") if !bullet => {
                num_format = Some(decode_attribute(&attribute)?);
            }
            (NamespaceKind::Style, b"num-prefix") if !bullet => {
                prefix = decode_attribute(&attribute)?;
            }
            (NamespaceKind::Style, b"num-suffix") if !bullet => {
                suffix = decode_attribute(&attribute)?;
            }
            (NamespaceKind::Text, b"start-value") if !bullet => {
                let value = decode_attribute(&attribute)?;
                start = value
                    .parse::<u16>()
                    .map_err(|_| OdfError::MalformedContent)?
                    .min(32_767);
            }
            _ if !is_namespace_declaration(&attribute) => reporter.report(
                attribute_feature(reader, &attribute),
                ModelOutcome::Degraded,
            ),
            _ => {}
        }
    }
    let source_level = source_level.ok_or(OdfError::MalformedContent)?;
    if source_level == 0 || source_level > u16::from(u8::MAX) + 1 {
        return Err(OdfError::MalformedContent);
    }
    let level = u8::try_from(source_level - 1).map_err(|_| OdfError::MalformedContent)?;
    let (num_fmt, lvl_text) = if bullet {
        let glyph = bullet_char.ok_or(OdfError::MalformedContent)?;
        if glyph.is_empty() || glyph.len() > 255 {
            return Err(OdfError::MalformedContent);
        }
        (NumberFormat::Bullet, glyph)
    } else {
        let format = match num_format.as_deref().unwrap_or("1") {
            "1" => NumberFormat::Decimal,
            "a" => NumberFormat::LowerLetter,
            "A" => NumberFormat::UpperLetter,
            "i" => NumberFormat::LowerRoman,
            "I" => NumberFormat::UpperRoman,
            _ => {
                reporter.report(
                    "odf.list-style.number-format".to_owned(),
                    ModelOutcome::Degraded,
                );
                NumberFormat::Decimal
            }
        };
        let template = format!("{prefix}%{}{suffix}", u16::from(level) + 1);
        if template.len() > 255 {
            return Err(OdfError::MalformedContent);
        }
        (format, template)
    };
    Ok(OdfListLevel {
        level,
        num_fmt,
        lvl_text,
        start,
    })
}

fn finish_list_style(
    open_list_style: &mut Option<OpenListStyle>,
    list_styles: &mut ListStyles,
) -> Result<(), OdfError> {
    let style = open_list_style.take().ok_or(OdfError::MalformedContent)?;
    if list_styles.insert(style.name, style.levels).is_some() {
        return Err(OdfError::MalformedContent);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn read_style_header(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    reporter: &mut Reporter,
) -> Result<OpenStyle, OdfError> {
    let mut name = None;
    let mut family = None;
    let mut family_seen = false;
    let mut parent = None;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        if namespace_kind(&namespace) == NamespaceKind::Style && local.as_ref() == b"name" {
            if name.is_some() {
                return Err(OdfError::MalformedContent);
            }
            name = Some(decode_attribute(&attribute)?);
        } else if namespace_kind(&namespace) == NamespaceKind::Style && local.as_ref() == b"family"
        {
            if family_seen {
                return Err(OdfError::MalformedContent);
            }
            family_seen = true;
            family = match decode_attribute(&attribute)?.as_str() {
                "paragraph" => Some(StyleFamily::Paragraph),
                "text" => Some(StyleFamily::Text),
                _ => {
                    reporter.report(
                        "odf.style.unsupported-family".to_owned(),
                        ModelOutcome::Omitted,
                    );
                    None
                }
            };
        } else if namespace_kind(&namespace) == NamespaceKind::Style
            && local.as_ref() == b"parent-style-name"
        {
            if parent.is_some() {
                return Err(OdfError::MalformedContent);
            }
            parent = Some(decode_attribute(&attribute)?);
        } else if !is_namespace_declaration(&attribute) {
            reporter.report(
                attribute_feature(reader, &attribute),
                ModelOutcome::Degraded,
            );
        }
    }
    let name = name.ok_or(OdfError::MalformedContent)?;
    if name.is_empty() {
        return Err(OdfError::MalformedContent);
    }
    if !family_seen {
        return Err(OdfError::MalformedContent);
    }
    Ok(OpenStyle {
        depth,
        name,
        style: OdfStyle {
            family,
            parent,
            ..OdfStyle::default()
        },
    })
}

fn finish_style(
    open_style: &mut Option<OpenStyle>,
    styles: &mut AutomaticStyles,
) -> Result<(), OdfError> {
    let style = open_style.take().ok_or(OdfError::MalformedContent)?;
    let Some(family) = style.style.family else {
        return Ok(());
    };
    if styles.insert((family, style.name), style.style).is_some() {
        return Err(OdfError::MalformedContent);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn read_text_style_properties(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    style: &mut OpenStyle,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    if style.style.family != Some(StyleFamily::Text) {
        count_and_report_attributes(
            reader,
            element,
            attributes,
            attribute_bytes,
            limits,
            reporter,
        )?;
        reporter.report(
            "odf.style.text-properties.family".to_owned(),
            ModelOutcome::Degraded,
        );
        return Ok(());
    }
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        let value = decode_attribute(&attribute)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        let mapped = match (namespace_kind(&namespace), local.as_ref()) {
            (NamespaceKind::Fo, b"font-weight") => {
                style.style.run_properties.bold = parse_toggle(&value, "bold", "normal");
                style.style.run_properties.bold.is_some()
            }
            (NamespaceKind::Fo, b"font-style") => {
                style.style.run_properties.italic = parse_toggle(&value, "italic", "normal");
                style.style.run_properties.italic.is_some()
            }
            (NamespaceKind::Style, b"text-underline-style") => {
                style.style.run_properties.underline = parse_toggle(&value, "solid", "none");
                style.style.run_properties.underline.is_some()
            }
            (NamespaceKind::Style, b"text-line-through-style") => {
                style.style.run_properties.strike = parse_toggle(&value, "solid", "none");
                style.style.run_properties.strike.is_some()
            }
            (NamespaceKind::Fo, b"color") => {
                style.style.run_properties.color = parse_rgb_color(&value).map(Color::Rgb);
                style.style.run_properties.color.is_some()
            }
            (NamespaceKind::Fo, b"font-size") => {
                style.style.run_properties.size_half_points = parse_half_points(&value);
                style.style.run_properties.size_half_points.is_some()
            }
            _ => false,
        };
        if !mapped && !is_namespace_declaration(&attribute) {
            reporter.report(
                attribute_feature(reader, &attribute),
                ModelOutcome::Degraded,
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn read_paragraph_style_properties(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    style: &mut OpenStyle,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    if style.style.family != Some(StyleFamily::Paragraph) {
        count_and_report_attributes(
            reader,
            element,
            attributes,
            attribute_bytes,
            limits,
            reporter,
        )?;
        reporter.report(
            "odf.style.paragraph-properties.family".to_owned(),
            ModelOutcome::Degraded,
        );
        return Ok(());
    }
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        let value = decode_attribute(&attribute)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        let mapped =
            if namespace_kind(&namespace) == NamespaceKind::Fo && local.as_ref() == b"text-align" {
                style.style.alignment = match value.as_str() {
                    "start" => Some(Alignment::Start),
                    "end" => Some(Alignment::End),
                    "center" => Some(Alignment::Center),
                    "justify" => Some(Alignment::Justify),
                    _ => None,
                };
                style.style.alignment.is_some()
            } else {
                false
            };
        if !mapped && !is_namespace_declaration(&attribute) {
            reporter.report(
                attribute_feature(reader, &attribute),
                ModelOutcome::Degraded,
            );
        }
    }
    Ok(())
}

fn parse_toggle(value: &str, enabled: &str, disabled: &str) -> Option<bool> {
    match value {
        value if value == enabled => Some(true),
        value if value == disabled => Some(false),
        _ => None,
    }
}

fn parse_rgb_color(value: &str) -> Option<RgbColor> {
    let value = value.strip_prefix('#')?;
    if value.len() != 6 || !value.is_ascii() {
        return None;
    }
    Some(RgbColor {
        r: u8::from_str_radix(&value[0..2], 16).ok()?,
        g: u8::from_str_radix(&value[2..4], 16).ok()?,
        b: u8::from_str_radix(&value[4..6], 16).ok()?,
    })
}

fn parse_half_points(value: &str) -> Option<u32> {
    let value = value.strip_suffix("pt")?;
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let whole = whole.parse::<u32>().ok()?;
    let scale = 10_u32.checked_pow(u32::try_from(fraction.len()).ok()?)?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u32>().ok()?
    };
    let doubled_fraction = fraction.checked_mul(2)?;
    if doubled_fraction % scale != 0 {
        return None;
    }
    let result = whole
        .checked_mul(2)?
        .checked_add(doubled_fraction / scale)?;
    (1..=65_534).contains(&result).then_some(result)
}

/// Imports standalone ODT `content.xml` bytes under explicit bounds.
pub fn import_content_xml(
    bytes: &[u8],
    expected_version: OdfVersion,
    limits: OdfImportLimits,
) -> Result<OdtImport, OdfError> {
    import_content_xml_with_cancellation(
        bytes,
        expected_version,
        limits,
        &CancellationToken::default(),
    )
}

/// Imports standalone ODT `content.xml` while honoring cooperative cancellation.
pub fn import_content_xml_with_cancellation(
    bytes: &[u8],
    expected_version: OdfVersion,
    limits: OdfImportLimits,
    cancellation: &CancellationToken,
) -> Result<OdtImport, OdfError> {
    import_content_xml_with_styles_and_cancellation(
        bytes,
        None,
        expected_version,
        limits,
        cancellation,
    )
}

pub(crate) fn import_content_xml_with_styles_and_cancellation(
    bytes: &[u8],
    styles_bytes: Option<&[u8]>,
    expected_version: OdfVersion,
    limits: OdfImportLimits,
    cancellation: &CancellationToken,
) -> Result<OdtImport, OdfError> {
    limits.validate()?;
    enforce("odf_content_bytes", bytes.len(), limits.max_content_bytes)?;
    if let Some(styles_bytes) = styles_bytes {
        enforce(
            "odf_styles_bytes",
            styles_bytes.len(),
            limits.max_styles_bytes,
        )?;
    }
    check_cancelled(cancellation)?;

    let mut reporter = Reporter::new(limits.max_report_features);
    let style_catalog = parse_style_catalog(
        bytes,
        styles_bytes,
        expected_version,
        limits,
        cancellation,
        &mut reporter,
    )?;

    let mut reader = NsReader::from_reader(bytes);
    reader
        .resolver_mut()
        .set_max_declarations_per_element(MAX_NAMESPACE_DECLARATIONS_PER_ELEMENT);
    let mut buffer = Vec::new();
    let mut depth = 0_usize;
    let mut elements = 0_usize;
    let mut attributes = 0_usize;
    let mut attribute_bytes = 0_usize;
    let mut text_bytes = 0_usize;
    let mut inline_nodes = 0_usize;
    let mut root_seen = false;
    let mut root_closed = false;
    let mut body_seen = false;
    let mut body_depth = None;
    let mut text_body_depth = None;
    let mut automatic_styles_depth = None;
    let mut leaf_depth = None;
    let mut body_kind_seen = false;
    let mut current = None;
    let mut active_link = None;
    let mut paragraphs = Vec::new();
    let mut blocks = Vec::new();
    let mut open_tables = Vec::new();
    let mut bookmarks = Vec::new();
    let mut open_bookmarks = BTreeMap::new();
    let mut active_run_properties = RunProperties::default();
    let mut open_spans = Vec::new();
    let mut open_lists = Vec::new();
    let mut open_list_items = Vec::new();
    let mut list_count = 0_usize;
    let mut list_instances = 0_usize;
    let mut table_count = 0_usize;
    let mut table_row_count = 0_usize;
    let mut table_cell_count = 0_usize;
    let mut notes = Vec::new();
    let mut note_source_ids = BTreeSet::new();
    let mut open_note = None;

    loop {
        check_cancelled(cancellation)?;
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| OdfError::MalformedContent)?;
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err(OdfError::MalformedContent),
            Event::Start(element) => {
                if leaf_depth.is_some() {
                    return Err(OdfError::MalformedContent);
                }
                depth = depth.checked_add(1).ok_or(OdfError::MalformedContent)?;
                enforce("odf_content_xml_depth", depth, limits.max_xml_depth)?;
                elements = checked_increment(elements)?;
                enforce(
                    "odf_content_xml_elements",
                    elements,
                    limits.max_xml_elements,
                )?;
                validate_name(&element, limits)?;
                let name = resolved_name(&reader, &element);
                if is_active(&name) {
                    return Err(OdfError::ActiveContent);
                }
                if !root_seen {
                    if depth != 1 || !is_name(&name, NamespaceKind::Office, b"document-content") {
                        return Err(OdfError::MalformedContent);
                    }
                    root_seen = true;
                    let version = read_version(
                        &reader,
                        &element,
                        &mut attributes,
                        &mut attribute_bytes,
                        limits,
                        &mut reporter,
                    )?;
                    if version != expected_version {
                        return Err(OdfError::ManifestMismatch);
                    }
                } else if depth == 2 && is_name(&name, NamespaceKind::Office, b"automatic-styles") {
                    automatic_styles_depth = Some(depth);
                    count_attributes_only(&element, &mut attributes, &mut attribute_bytes, limits)?;
                } else if automatic_styles_depth.is_some() {
                    count_attributes_only(&element, &mut attributes, &mut attribute_bytes, limits)?;
                } else {
                    if root_closed {
                        return Err(OdfError::MalformedContent);
                    }
                    if text_body_depth.is_some()
                        && current.is_some()
                        && is_name(&name, NamespaceKind::Text, b"note")
                    {
                        start_note(
                            &reader,
                            &element,
                            depth,
                            limits,
                            &mut attributes,
                            &mut attribute_bytes,
                            &mut current,
                            &mut active_link,
                            &mut active_run_properties,
                            &mut open_spans,
                            &mut open_lists,
                            &mut open_list_items,
                            &mut open_bookmarks,
                            &open_tables,
                            &notes,
                            &mut note_source_ids,
                            &mut open_note,
                            &mut reporter,
                        )?;
                    } else if open_note
                        .as_ref()
                        .is_some_and(|note: &OpenNote| note.body_depth.is_none())
                    {
                        if !process_note_container_start(
                            &reader,
                            &element,
                            &name,
                            depth,
                            limits,
                            &mut attributes,
                            &mut attribute_bytes,
                            &mut open_note,
                            false,
                            &mut reporter,
                        )? {
                            return Err(OdfError::MalformedContent);
                        }
                    } else if text_body_depth.is_some()
                        && current.is_none()
                        && is_name(&name, NamespaceKind::Text, b"list")
                    {
                        start_list(
                            &reader,
                            &element,
                            depth,
                            limits,
                            &mut attributes,
                            &mut attribute_bytes,
                            &mut open_lists,
                            &open_list_items,
                            &mut list_count,
                            &mut list_instances,
                            &style_catalog.lists,
                            &mut reporter,
                        )?;
                    } else if text_body_depth.is_some()
                        && current.is_none()
                        && is_name(&name, NamespaceKind::Text, b"list-item")
                    {
                        start_list_item(
                            &reader,
                            &element,
                            depth,
                            limits,
                            &mut attributes,
                            &mut attribute_bytes,
                            &open_lists,
                            &mut open_list_items,
                            &mut reporter,
                        )?;
                    } else if text_body_depth.is_some()
                        && current.is_none()
                        && name.namespace == NamespaceKind::Table
                    {
                        process_table_start(
                            &reader,
                            &element,
                            &name,
                            depth,
                            limits,
                            &mut attributes,
                            &mut attribute_bytes,
                            &mut open_tables,
                            &mut table_count,
                            &mut leaf_depth,
                            &mut reporter,
                        )?;
                    } else {
                        process_start(
                            &reader,
                            &element,
                            &name,
                            depth,
                            limits,
                            &mut attributes,
                            &mut attribute_bytes,
                            &mut body_seen,
                            &mut body_depth,
                            &mut text_body_depth,
                            &mut leaf_depth,
                            &mut body_kind_seen,
                            &mut current,
                            &mut active_link,
                            &mut bookmarks,
                            &mut open_bookmarks,
                            &mut inline_nodes,
                            &mut text_bytes,
                            &style_catalog.automatic,
                            &open_lists,
                            &mut open_list_items,
                            &mut active_run_properties,
                            &mut open_spans,
                            &mut reporter,
                        )?;
                    }
                }
            }
            Event::Empty(element) => {
                elements = checked_increment(elements)?;
                enforce(
                    "odf_content_xml_elements",
                    elements,
                    limits.max_xml_elements,
                )?;
                validate_name(&element, limits)?;
                if !root_seen || root_closed {
                    return Err(OdfError::MalformedContent);
                }
                let name = resolved_name(&reader, &element);
                if is_active(&name) {
                    return Err(OdfError::ActiveContent);
                }
                if (depth + 1 == 2 && is_name(&name, NamespaceKind::Office, b"automatic-styles"))
                    || automatic_styles_depth.is_some()
                {
                    count_attributes_only(&element, &mut attributes, &mut attribute_bytes, limits)?;
                } else if text_body_depth.is_some()
                    && current.is_some()
                    && is_name(&name, NamespaceKind::Text, b"note")
                {
                    return Err(OdfError::MalformedContent);
                } else if open_note
                    .as_ref()
                    .is_some_and(|note: &OpenNote| note.body_depth.is_none())
                {
                    if !process_note_container_start(
                        &reader,
                        &element,
                        &name,
                        depth + 1,
                        limits,
                        &mut attributes,
                        &mut attribute_bytes,
                        &mut open_note,
                        true,
                        &mut reporter,
                    )? {
                        return Err(OdfError::MalformedContent);
                    }
                } else if text_body_depth.is_some()
                    && current.is_none()
                    && name.namespace == NamespaceKind::Table
                {
                    process_table_empty(
                        &reader,
                        &element,
                        &name,
                        depth + 1,
                        limits,
                        &mut attributes,
                        &mut attribute_bytes,
                        &mut open_tables,
                        &mut table_row_count,
                        &mut table_cell_count,
                        &mut reporter,
                    )?;
                } else {
                    process_empty(
                        &reader,
                        &element,
                        &name,
                        depth + 1,
                        limits,
                        &mut attributes,
                        &mut attribute_bytes,
                        &mut body_seen,
                        body_depth,
                        text_body_depth,
                        &mut body_kind_seen,
                        &mut current,
                        &mut active_link,
                        &mut bookmarks,
                        &mut open_bookmarks,
                        &mut paragraphs,
                        &mut blocks,
                        &mut open_tables,
                        &mut open_note,
                        &mut inline_nodes,
                        &mut text_bytes,
                        &style_catalog.automatic,
                        &open_lists,
                        &mut open_list_items,
                        &mut active_run_properties,
                        &mut open_spans,
                        &mut reporter,
                    )?;
                }
            }
            Event::End(element) => {
                if leaf_depth == Some(depth) {
                    leaf_depth = None;
                }
                if active_link
                    .as_ref()
                    .is_some_and(|link: &LinkDraft| link.depth == depth)
                {
                    finish_link(&mut current, &mut active_link, &mut reporter)?;
                }
                if open_spans
                    .last()
                    .is_some_and(|span: &OpenSpan| span.depth == depth)
                {
                    let span = open_spans.pop().ok_or(OdfError::MalformedContent)?;
                    active_run_properties = span.previous_properties;
                }
                if current
                    .as_ref()
                    .is_some_and(|paragraph: &ParagraphDraft| paragraph.depth == depth)
                {
                    let paragraph = current.take().ok_or(OdfError::MalformedContent)?;
                    push_paragraph_block(
                        &mut paragraphs,
                        &mut blocks,
                        &mut open_tables,
                        &mut open_note,
                        paragraph,
                        limits,
                    )?;
                    active_run_properties = RunProperties::default();
                }
                if open_list_items
                    .last()
                    .is_some_and(|item: &OpenListItem| item.depth == depth)
                {
                    open_list_items.pop();
                }
                if open_lists
                    .last()
                    .is_some_and(|list: &OpenList| list.depth == depth)
                {
                    open_lists.pop();
                }
                if open_tables.last().is_some_and(|table| {
                    table
                        .current_row
                        .as_ref()
                        .and_then(|row| row.current_cell.as_ref())
                        .is_some_and(|cell| cell.depth == depth)
                }) {
                    finish_table_cell(&mut open_tables, limits)?;
                }
                if open_tables.last().is_some_and(|table| {
                    table
                        .current_row
                        .as_ref()
                        .is_some_and(|row| row.depth == depth)
                }) {
                    finish_table_row(
                        &mut open_tables,
                        limits,
                        &mut table_row_count,
                        &mut table_cell_count,
                    )?;
                }
                if open_tables
                    .last()
                    .is_some_and(|table| table.row_container_depth == Some(depth))
                {
                    let table = open_tables.last_mut().ok_or(OdfError::MalformedContent)?;
                    table.row_container_depth = None;
                    table.row_container_header = false;
                }
                if open_tables.last().is_some_and(|table| table.depth == depth) {
                    finish_table(&mut open_tables, &mut blocks, &mut open_note)?;
                }
                if open_note
                    .as_ref()
                    .is_some_and(|note: &OpenNote| note.citation_depth == Some(depth))
                {
                    open_note
                        .as_mut()
                        .ok_or(OdfError::MalformedContent)?
                        .citation_depth = None;
                }
                if open_note
                    .as_ref()
                    .is_some_and(|note: &OpenNote| note.body_depth == Some(depth))
                {
                    open_note
                        .as_mut()
                        .ok_or(OdfError::MalformedContent)?
                        .body_depth = None;
                }
                if open_note
                    .as_ref()
                    .is_some_and(|note: &OpenNote| note.depth == depth)
                {
                    finish_note(
                        limits,
                        &mut current,
                        &mut active_link,
                        &mut active_run_properties,
                        &mut open_spans,
                        &mut open_lists,
                        &mut open_list_items,
                        &mut open_bookmarks,
                        &open_tables,
                        &mut inline_nodes,
                        &mut notes,
                        &mut open_note,
                        &mut reporter,
                    )?;
                }
                if automatic_styles_depth == Some(depth) {
                    automatic_styles_depth = None;
                }
                if text_body_depth == Some(depth) {
                    text_body_depth = None;
                }
                if body_depth == Some(depth) {
                    body_depth = None;
                }
                if depth == 1 {
                    if element.local_name().as_ref() != b"document-content" {
                        return Err(OdfError::MalformedContent);
                    }
                    root_closed = true;
                }
                depth = depth.checked_sub(1).ok_or(OdfError::MalformedContent)?;
            }
            Event::Text(text) => {
                if leaf_depth.is_some() {
                    return Err(OdfError::MalformedContent);
                }
                let decoded = text.decode().map_err(|_| OdfError::MalformedContent)?;
                let value = quick_xml::escape::unescape(&decoded)
                    .map_err(|_| OdfError::MalformedContent)?;
                if let Some(note) = &mut open_note
                    && note.citation_depth.is_some()
                {
                    note.citation_has_text |= !value.is_empty();
                } else if open_note
                    .as_ref()
                    .is_some_and(|note: &OpenNote| note.body_depth.is_none())
                {
                    if !value.trim().is_empty() {
                        return Err(OdfError::MalformedContent);
                    }
                } else if let Some(paragraph) = &mut current {
                    append_text(
                        paragraph,
                        &mut active_link,
                        &value,
                        &active_run_properties,
                        &mut text_bytes,
                        limits,
                    )?;
                } else if !value.trim().is_empty() {
                    return Err(OdfError::MalformedContent);
                }
            }
            Event::CData(text) => {
                if leaf_depth.is_some() {
                    return Err(OdfError::MalformedContent);
                }
                let value = text.decode().map_err(|_| OdfError::MalformedContent)?;
                if let Some(note) = &mut open_note
                    && note.citation_depth.is_some()
                {
                    note.citation_has_text |= !value.is_empty();
                } else if open_note
                    .as_ref()
                    .is_some_and(|note: &OpenNote| note.body_depth.is_none())
                {
                    if !value.trim().is_empty() {
                        return Err(OdfError::MalformedContent);
                    }
                } else if let Some(paragraph) = &mut current {
                    append_text(
                        paragraph,
                        &mut active_link,
                        &value,
                        &active_run_properties,
                        &mut text_bytes,
                        limits,
                    )?;
                } else if !value.trim().is_empty() {
                    return Err(OdfError::MalformedContent);
                }
            }
            Event::GeneralRef(reference) => {
                if leaf_depth.is_some() {
                    return Err(OdfError::MalformedContent);
                }
                let value = decode_reference(&reference)?;
                if let Some(note) = &mut open_note
                    && note.citation_depth.is_some()
                {
                    note.citation_has_text |= !value.is_empty();
                } else if open_note
                    .as_ref()
                    .is_some_and(|note: &OpenNote| note.body_depth.is_none())
                {
                    if !value.trim().is_empty() {
                        return Err(OdfError::MalformedContent);
                    }
                } else if let Some(paragraph) = &mut current {
                    append_text(
                        paragraph,
                        &mut active_link,
                        &value,
                        &active_run_properties,
                        &mut text_bytes,
                        limits,
                    )?;
                } else if !value.trim().is_empty() {
                    return Err(OdfError::MalformedContent);
                }
            }
            _ => {}
        }
        buffer.clear();
    }

    if !root_seen
        || !root_closed
        || depth != 0
        || body_depth.is_some()
        || text_body_depth.is_some()
        || automatic_styles_depth.is_some()
        || leaf_depth.is_some()
        || current.is_some()
        || active_link.is_some()
        || !open_spans.is_empty()
        || !open_lists.is_empty()
        || !open_list_items.is_empty()
        || !open_tables.is_empty()
        || open_note.is_some()
    {
        return Err(OdfError::MalformedContent);
    }
    if !body_kind_seen {
        return Err(OdfError::UnsupportedDocumentKind);
    }
    if blocks.is_empty() {
        push_paragraph_block(
            &mut paragraphs,
            &mut blocks,
            &mut open_tables,
            &mut open_note,
            ParagraphDraft {
                depth: 0,
                outline_level: None,
                alignment: None,
                numbering: None,
                inlines: Vec::new(),
            },
            limits,
        )?;
    }
    for index in open_bookmarks.into_values() {
        if let Some(bookmark) = bookmarks.get_mut(index) {
            bookmark.paired = false;
        }
        reporter.report(
            "odf.element.text.bookmark-start".to_owned(),
            ModelOutcome::Omitted,
        );
    }
    for paragraph in &mut paragraphs {
        normalize_inline_drafts(&mut paragraph.inlines, &bookmarks, &mut reporter);
    }
    enforce_expanded_block_limits(&blocks, &notes, &paragraphs, limits)?;
    let document = build_document(
        expected_version,
        &paragraphs,
        &blocks,
        &notes,
        &bookmarks,
        &style_catalog.lists,
        &mut reporter,
    )?;
    Ok(OdtImport {
        document,
        report: reporter.finish(),
    })
}

#[allow(clippy::too_many_arguments)]
fn start_list(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    open_lists: &mut Vec<OpenList>,
    open_list_items: &[OpenListItem],
    list_count: &mut usize,
    list_instances: &mut usize,
    list_styles: &ListStyles,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    if let Some(parent) = open_lists.last()
        && open_list_items
            .last()
            .is_none_or(|item| item.depth != parent.depth + 1 || depth != item.depth + 1)
    {
        return Err(OdfError::MalformedContent);
    }
    *list_count = checked_increment(*list_count)?;
    enforce("odf_content_lists", *list_count, limits.max_lists)?;
    let list_depth = open_lists
        .len()
        .checked_add(1)
        .ok_or(OdfError::MalformedContent)?;
    enforce("odf_content_list_depth", list_depth, limits.max_list_depth)?;
    let mut explicit_style = None;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        if namespace_kind(&namespace) == NamespaceKind::Text && local.as_ref() == b"style-name" {
            if explicit_style.is_some() {
                return Err(OdfError::MalformedContent);
            }
            explicit_style = Some(decode_attribute(&attribute)?);
        } else if namespace_kind(&namespace) == NamespaceKind::Text
            && matches!(local.as_ref(), b"continue-list" | b"continue-numbering")
        {
            reporter.report("odf.list.continuation".to_owned(), ModelOutcome::Degraded);
        } else if !is_namespace_declaration(&attribute) {
            reporter.report(
                attribute_feature(reader, &attribute),
                ModelOutcome::Degraded,
            );
        }
    }
    let inherited_style = open_lists.last().and_then(|list| list.style_name.clone());
    let style_name = explicit_style.or(inherited_style);
    match style_name.as_deref() {
        Some(name) if list_styles.contains_key(name) => {}
        Some(_) => reporter.report(
            "odf.list-style.unresolved".to_owned(),
            ModelOutcome::Degraded,
        ),
        None => reporter.report(
            "odf.list-style.defaulted".to_owned(),
            ModelOutcome::Degraded,
        ),
    }
    let (instance, level) = if let Some(parent) = open_lists.first() {
        let level = u8::try_from(open_lists.len()).map_err(|_| OdfError::MalformedContent)?;
        (parent.instance, level)
    } else {
        let instance = *list_instances;
        *list_instances = list_instances
            .checked_add(1)
            .ok_or(OdfError::MalformedContent)?;
        (instance, 0)
    };
    open_lists.push(OpenList {
        depth,
        instance,
        level,
        style_name,
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn start_list_item(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    open_lists: &[OpenList],
    open_list_items: &mut Vec<OpenListItem>,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    if open_lists.last().is_none_or(|list| depth != list.depth + 1) {
        return Err(OdfError::MalformedContent);
    }
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        if namespace_kind(&namespace) == NamespaceKind::Text
            && matches!(local.as_ref(), b"start-value" | b"style-override")
        {
            reporter.report("odf.list.item-override".to_owned(), ModelOutcome::Degraded);
        } else if !is_namespace_declaration(&attribute) {
            reporter.report(
                attribute_feature(reader, &attribute),
                ModelOutcome::Degraded,
            );
        }
    }
    open_list_items.push(OpenListItem {
        depth,
        first_paragraph: true,
    });
    Ok(())
}

fn list_numbering(
    paragraph_depth: usize,
    open_lists: &[OpenList],
    open_list_items: &mut [OpenListItem],
) -> Result<Option<ListParagraphDraft>, OdfError> {
    let Some(list) = open_lists.last() else {
        return Ok(None);
    };
    let item = open_list_items
        .last_mut()
        .ok_or(OdfError::MalformedContent)?;
    if item.depth != list.depth + 1 || paragraph_depth != item.depth + 1 {
        return Err(OdfError::MalformedContent);
    }
    if !item.first_paragraph {
        return Ok(None);
    }
    item.first_paragraph = false;
    Ok(Some(ListParagraphDraft {
        instance: list.instance,
        level: list.level,
        style_name: list.style_name.clone(),
    }))
}

const MAX_TABLE_COLUMNS: usize = 16_384;

#[allow(clippy::too_many_arguments)]
fn process_table_start(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    name: &ResolvedName,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    open_tables: &mut Vec<OpenTable>,
    table_count: &mut usize,
    leaf_depth: &mut Option<usize>,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    match name.local.as_slice() {
        b"table" => start_table(
            reader,
            element,
            depth,
            limits,
            attributes,
            attribute_bytes,
            open_tables,
            table_count,
            reporter,
        ),
        b"table-header-rows" | b"table-rows" => start_table_row_container(
            reader,
            element,
            name.local.as_slice() == b"table-header-rows",
            depth,
            limits,
            attributes,
            attribute_bytes,
            open_tables,
            reporter,
        ),
        b"table-column" => {
            add_table_columns(
                reader,
                element,
                depth,
                limits,
                attributes,
                attribute_bytes,
                open_tables,
                reporter,
            )?;
            *leaf_depth = Some(depth);
            Ok(())
        }
        b"table-row" => start_table_row(
            reader,
            element,
            depth,
            limits,
            attributes,
            attribute_bytes,
            open_tables,
            reporter,
        ),
        b"table-cell" => start_table_cell(
            reader,
            element,
            depth,
            limits,
            attributes,
            attribute_bytes,
            open_tables,
            reporter,
        ),
        b"covered-table-cell" => {
            add_covered_table_cells(
                reader,
                element,
                depth,
                limits,
                attributes,
                attribute_bytes,
                open_tables,
                reporter,
            )?;
            *leaf_depth = Some(depth);
            Ok(())
        }
        _ => Err(OdfError::MalformedContent),
    }
}

#[allow(clippy::too_many_arguments)]
fn process_table_empty(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    name: &ResolvedName,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    open_tables: &mut [OpenTable],
    table_row_count: &mut usize,
    table_cell_count: &mut usize,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    match name.local.as_slice() {
        b"table-column" => add_table_columns(
            reader,
            element,
            depth,
            limits,
            attributes,
            attribute_bytes,
            open_tables,
            reporter,
        ),
        b"covered-table-cell" => add_covered_table_cells(
            reader,
            element,
            depth,
            limits,
            attributes,
            attribute_bytes,
            open_tables,
            reporter,
        ),
        b"table-cell" => {
            start_table_cell(
                reader,
                element,
                depth,
                limits,
                attributes,
                attribute_bytes,
                open_tables,
                reporter,
            )?;
            finish_table_cell(open_tables, limits)
        }
        b"table-header-rows" | b"table-rows" => {
            start_table_row_container(
                reader,
                element,
                name.local.as_slice() == b"table-header-rows",
                depth,
                limits,
                attributes,
                attribute_bytes,
                open_tables,
                reporter,
            )?;
            let table = open_tables.last_mut().ok_or(OdfError::MalformedContent)?;
            table.row_container_depth = None;
            table.row_container_header = false;
            Ok(())
        }
        b"table-row" => {
            start_table_row(
                reader,
                element,
                depth,
                limits,
                attributes,
                attribute_bytes,
                open_tables,
                reporter,
            )?;
            finish_table_row(open_tables, limits, table_row_count, table_cell_count)
        }
        // Empty tables cannot satisfy the normalized model's non-empty invariant.
        b"table" => Err(OdfError::MalformedContent),
        _ => Err(OdfError::MalformedContent),
    }
}

#[allow(clippy::too_many_arguments)]
fn start_table(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    open_tables: &mut Vec<OpenTable>,
    table_count: &mut usize,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    if let Some(parent) = open_tables.last()
        && parent
            .current_row
            .as_ref()
            .and_then(|row| row.current_cell.as_ref())
            .is_none()
    {
        return Err(OdfError::MalformedContent);
    }
    *table_count = checked_increment(*table_count)?;
    enforce("odf_content_tables", *table_count, limits.max_tables)?;
    let nesting = open_tables
        .len()
        .checked_add(1)
        .ok_or(OdfError::MalformedContent)?;
    enforce("odf_content_table_depth", nesting, limits.max_table_depth)?;
    count_and_report_attributes(
        reader,
        element,
        attributes,
        attribute_bytes,
        limits,
        reporter,
    )?;
    open_tables.push(OpenTable {
        depth,
        columns: 0,
        rows: Vec::new(),
        row_container_depth: None,
        row_container_header: false,
        current_row: None,
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn start_table_row_container(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    header: bool,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    open_tables: &mut [OpenTable],
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    let table = open_tables.last_mut().ok_or(OdfError::MalformedContent)?;
    if table.current_row.is_some()
        || table.row_container_depth.is_some()
        || depth != table.depth + 1
    {
        return Err(OdfError::MalformedContent);
    }
    count_and_report_attributes(
        reader,
        element,
        attributes,
        attribute_bytes,
        limits,
        reporter,
    )?;
    table.row_container_depth = Some(depth);
    table.row_container_header = header;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn add_table_columns(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    open_tables: &mut [OpenTable],
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    let table = open_tables.last_mut().ok_or(OdfError::MalformedContent)?;
    if table.current_row.is_some()
        || table.row_container_depth.is_some()
        || depth != table.depth + 1
    {
        return Err(OdfError::MalformedContent);
    }
    let repeat = read_table_repeat(
        reader,
        element,
        b"number-columns-repeated",
        limits,
        attributes,
        attribute_bytes,
        reporter,
    )?;
    let observed = table
        .columns
        .checked_add(repeat)
        .ok_or(OdfError::MalformedContent)?;
    enforce("odf_content_table_columns", observed, MAX_TABLE_COLUMNS)?;
    table.columns = observed;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn start_table_row(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    open_tables: &mut [OpenTable],
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    let table = open_tables.last_mut().ok_or(OdfError::MalformedContent)?;
    let expected_depth = table.row_container_depth.unwrap_or(table.depth) + 1;
    if table.current_row.is_some() || depth != expected_depth {
        return Err(OdfError::MalformedContent);
    }
    let repeat = read_table_repeat(
        reader,
        element,
        b"number-rows-repeated",
        limits,
        attributes,
        attribute_bytes,
        reporter,
    )?;
    table.current_row = Some(OpenTableRow {
        depth,
        repeat,
        header: table.row_container_header,
        slots: Vec::new(),
        current_cell: None,
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn start_table_cell(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    open_tables: &mut [OpenTable],
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    let row = open_tables
        .last_mut()
        .and_then(|table| table.current_row.as_mut())
        .ok_or(OdfError::MalformedContent)?;
    if row.current_cell.is_some() || depth != row.depth + 1 {
        return Err(OdfError::MalformedContent);
    }
    let mut repeat = None;
    let mut column_span = None;
    let mut row_span = None;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        if namespace_kind(&namespace) == NamespaceKind::Table {
            let target = match local.as_ref() {
                b"number-columns-repeated" => &mut repeat,
                b"number-columns-spanned" => &mut column_span,
                b"number-rows-spanned" => &mut row_span,
                _ => {
                    reporter.report(
                        attribute_feature(reader, &attribute),
                        ModelOutcome::Degraded,
                    );
                    continue;
                }
            };
            if target.is_some() {
                return Err(OdfError::MalformedContent);
            }
            *target = Some(parse_positive_usize(&decode_attribute(&attribute)?)?);
        } else if !is_namespace_declaration(&attribute) {
            reporter.report(
                attribute_feature(reader, &attribute),
                ModelOutcome::Degraded,
            );
        }
    }
    let repeat = repeat.unwrap_or(1);
    let column_span = column_span.unwrap_or(1);
    let row_span = row_span.unwrap_or(1);
    enforce("odf_content_table_columns", column_span, MAX_TABLE_COLUMNS)?;
    let column_span = u32::try_from(column_span).map_err(|_| OdfError::MalformedContent)?;
    let row_span = u32::try_from(row_span).map_err(|_| OdfError::MalformedContent)?;
    row.current_cell = Some(OpenTableCell {
        depth,
        repeat,
        column_span,
        row_span,
        blocks: Vec::new(),
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn add_covered_table_cells(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    open_tables: &mut [OpenTable],
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    let row = open_tables
        .last_mut()
        .and_then(|table| table.current_row.as_mut())
        .ok_or(OdfError::MalformedContent)?;
    if row.current_cell.is_some() || depth != row.depth + 1 {
        return Err(OdfError::MalformedContent);
    }
    let repeat = read_table_repeat(
        reader,
        element,
        b"number-columns-repeated",
        limits,
        attributes,
        attribute_bytes,
        reporter,
    )?;
    let observed = row
        .slots
        .len()
        .checked_add(repeat)
        .ok_or(OdfError::MalformedContent)?;
    enforce("odf_content_table_columns", observed, MAX_TABLE_COLUMNS)?;
    enforce("odf_content_table_cells", observed, limits.max_table_cells)?;
    row.slots
        .extend(std::iter::repeat_n(TableSlotDraft::Covered, repeat));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn read_table_repeat(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    expected_local: &[u8],
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    reporter: &mut Reporter,
) -> Result<usize, OdfError> {
    let mut repeat = None;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        if namespace_kind(&namespace) == NamespaceKind::Table && local.as_ref() == expected_local {
            if repeat.is_some() {
                return Err(OdfError::MalformedContent);
            }
            repeat = Some(parse_positive_usize(&decode_attribute(&attribute)?)?);
        } else if !is_namespace_declaration(&attribute) {
            reporter.report(
                attribute_feature(reader, &attribute),
                ModelOutcome::Degraded,
            );
        }
    }
    Ok(repeat.unwrap_or(1))
}

fn parse_positive_usize(raw: &str) -> Result<usize, OdfError> {
    let value = raw
        .parse::<usize>()
        .map_err(|_| OdfError::MalformedContent)?;
    if value == 0 {
        return Err(OdfError::MalformedContent);
    }
    Ok(value)
}

fn finish_table_cell(
    open_tables: &mut [OpenTable],
    limits: OdfImportLimits,
) -> Result<(), OdfError> {
    let row = open_tables
        .last_mut()
        .and_then(|table| table.current_row.as_mut())
        .ok_or(OdfError::MalformedContent)?;
    let cell = row.current_cell.take().ok_or(OdfError::MalformedContent)?;
    let observed = row
        .slots
        .len()
        .checked_add(cell.repeat)
        .ok_or(OdfError::MalformedContent)?;
    enforce("odf_content_table_columns", observed, MAX_TABLE_COLUMNS)?;
    enforce("odf_content_table_cells", observed, limits.max_table_cells)?;
    let draft = TableCellDraft {
        column_span: cell.column_span,
        row_span: cell.row_span,
        blocks: cell.blocks,
    };
    row.slots.extend(std::iter::repeat_n(
        TableSlotDraft::Cell(draft),
        cell.repeat,
    ));
    Ok(())
}

fn finish_table_row(
    open_tables: &mut [OpenTable],
    limits: OdfImportLimits,
    table_row_count: &mut usize,
    table_cell_count: &mut usize,
) -> Result<(), OdfError> {
    let table = open_tables.last_mut().ok_or(OdfError::MalformedContent)?;
    let row = table.current_row.take().ok_or(OdfError::MalformedContent)?;
    if row.current_cell.is_some() || row.slots.is_empty() {
        return Err(OdfError::MalformedContent);
    }
    let rows_observed = table_row_count
        .checked_add(row.repeat)
        .ok_or(OdfError::MalformedContent)?;
    enforce(
        "odf_content_table_rows",
        rows_observed,
        limits.max_table_rows,
    )?;
    let added_cells = row
        .slots
        .len()
        .checked_mul(row.repeat)
        .ok_or(OdfError::MalformedContent)?;
    let cells_observed = table_cell_count
        .checked_add(added_cells)
        .ok_or(OdfError::MalformedContent)?;
    enforce(
        "odf_content_table_cells",
        cells_observed,
        limits.max_table_cells,
    )?;
    *table_row_count = rows_observed;
    *table_cell_count = cells_observed;
    let draft = TableRowDraft {
        header: row.header,
        slots: row.slots,
    };
    table.rows.extend(std::iter::repeat_n(draft, row.repeat));
    Ok(())
}

fn finish_table(
    open_tables: &mut Vec<OpenTable>,
    blocks: &mut Vec<BlockDraft>,
    open_note: &mut Option<OpenNote>,
) -> Result<(), OdfError> {
    let table = open_tables.pop().ok_or(OdfError::MalformedContent)?;
    if table.current_row.is_some() || table.row_container_depth.is_some() || table.rows.is_empty() {
        return Err(OdfError::MalformedContent);
    }
    let columns = table
        .rows
        .iter()
        .map(|row| row.slots.len())
        .max()
        .unwrap_or(0)
        .max(table.columns);
    enforce("odf_content_table_columns", columns, MAX_TABLE_COLUMNS)?;
    let draft = TableDraft {
        columns,
        rows: table.rows,
    };
    validate_table_topology(&draft)?;
    push_block_draft(blocks, open_tables, open_note, BlockDraft::Table(draft))
}

fn validate_table_topology(table: &TableDraft) -> Result<(), OdfError> {
    let mut occupied = BTreeMap::<(usize, usize), (usize, usize)>::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        for (column_index, slot) in row.slots.iter().enumerate() {
            let TableSlotDraft::Cell(cell) = slot else {
                continue;
            };
            if occupied.contains_key(&(row_index, column_index)) {
                return Err(OdfError::MalformedContent);
            }
            let row_end = row_index
                .checked_add(cell.row_span as usize)
                .ok_or(OdfError::MalformedContent)?;
            let column_end = column_index
                .checked_add(cell.column_span as usize)
                .ok_or(OdfError::MalformedContent)?;
            if row_end > table.rows.len() || column_end > table.columns {
                return Err(OdfError::MalformedContent);
            }
            for covered_row in row_index..row_end {
                let target_row = table
                    .rows
                    .get(covered_row)
                    .ok_or(OdfError::MalformedContent)?;
                if column_end > target_row.slots.len() {
                    return Err(OdfError::MalformedContent);
                }
                for covered_column in column_index..column_end {
                    let coordinate = (covered_row, covered_column);
                    if occupied
                        .insert(coordinate, (row_index, column_index))
                        .is_some()
                    {
                        return Err(OdfError::MalformedContent);
                    }
                    if coordinate != (row_index, column_index)
                        && !matches!(
                            target_row.slots.get(covered_column),
                            Some(TableSlotDraft::Covered)
                        )
                    {
                        return Err(OdfError::MalformedContent);
                    }
                }
            }
        }
    }
    for (row_index, row) in table.rows.iter().enumerate() {
        for (column_index, slot) in row.slots.iter().enumerate() {
            if matches!(slot, TableSlotDraft::Covered)
                && !occupied.contains_key(&(row_index, column_index))
            {
                return Err(OdfError::MalformedContent);
            }
        }
    }
    Ok(())
}

fn push_block_draft(
    blocks: &mut Vec<BlockDraft>,
    open_tables: &mut [OpenTable],
    open_note: &mut Option<OpenNote>,
    block: BlockDraft,
) -> Result<(), OdfError> {
    if open_note
        .as_ref()
        .is_some_and(|note| open_tables.len() <= note.table_stack_len)
    {
        open_note
            .as_mut()
            .ok_or(OdfError::MalformedContent)?
            .blocks
            .push(block);
    } else if let Some(table) = open_tables.last_mut() {
        let cell = table
            .current_row
            .as_mut()
            .and_then(|row| row.current_cell.as_mut())
            .ok_or(OdfError::MalformedContent)?;
        cell.blocks.push(block);
    } else {
        blocks.push(block);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn start_note(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    current: &mut Option<ParagraphDraft>,
    active_link: &mut Option<LinkDraft>,
    active_run_properties: &mut RunProperties,
    open_spans: &mut Vec<OpenSpan>,
    open_lists: &mut Vec<OpenList>,
    open_list_items: &mut Vec<OpenListItem>,
    open_bookmarks: &mut BTreeMap<String, usize>,
    open_tables: &[OpenTable],
    notes: &[NoteDraft],
    note_source_ids: &mut BTreeSet<String>,
    open_note: &mut Option<OpenNote>,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    if open_note.is_some() {
        return Err(OdfError::MalformedContent);
    }
    enforce(
        "odf_content_notes",
        notes
            .len()
            .checked_add(1)
            .ok_or(OdfError::MalformedContent)?,
        limits.max_notes,
    )?;
    let mut source_id = None;
    let mut kind = None;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        if namespace_kind(&namespace) == NamespaceKind::Text && local.as_ref() == b"id" {
            if source_id.is_some() {
                return Err(OdfError::MalformedContent);
            }
            source_id = Some(decode_attribute(&attribute)?);
        } else if namespace_kind(&namespace) == NamespaceKind::Text
            && local.as_ref() == b"note-class"
        {
            if kind.is_some() {
                return Err(OdfError::MalformedContent);
            }
            kind = Some(match decode_attribute(&attribute)?.as_str() {
                "footnote" => NoteKind::Footnote,
                "endnote" => NoteKind::Endnote,
                _ => return Err(OdfError::MalformedContent),
            });
        } else if !is_namespace_declaration(&attribute) {
            reporter.report(
                attribute_feature(reader, &attribute),
                ModelOutcome::Degraded,
            );
        }
    }
    let source_id = source_id.ok_or(OdfError::MalformedContent)?;
    if source_id.is_empty() || source_id.len() > 255 || !note_source_ids.insert(source_id.clone()) {
        return Err(OdfError::MalformedContent);
    }
    let outer = SuspendedInlineContext {
        paragraph: current.take().ok_or(OdfError::MalformedContent)?,
        active_link: active_link.take(),
        active_run_properties: std::mem::take(active_run_properties),
        open_spans: std::mem::take(open_spans),
        open_lists: std::mem::take(open_lists),
        open_list_items: std::mem::take(open_list_items),
        open_bookmarks: std::mem::take(open_bookmarks),
    };
    *open_note = Some(OpenNote {
        depth,
        body_depth: None,
        body_seen: false,
        citation_depth: None,
        citation_has_text: false,
        kind: kind.ok_or(OdfError::MalformedContent)?,
        blocks: Vec::new(),
        table_stack_len: open_tables.len(),
        outer,
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn process_note_container_start(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    name: &ResolvedName,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    open_note: &mut Option<OpenNote>,
    empty: bool,
    reporter: &mut Reporter,
) -> Result<bool, OdfError> {
    let note = open_note.as_mut().ok_or(OdfError::MalformedContent)?;
    if depth != note.depth + 1 {
        return Ok(false);
    }
    if is_name(name, NamespaceKind::Text, b"note-citation") {
        if note.citation_depth.is_some() || note.body_seen {
            return Err(OdfError::MalformedContent);
        }
        count_and_report_attributes(
            reader,
            element,
            attributes,
            attribute_bytes,
            limits,
            reporter,
        )?;
        if !empty {
            note.citation_depth = Some(depth);
        }
        return Ok(true);
    }
    if is_name(name, NamespaceKind::Text, b"note-body") {
        if note.body_seen || note.citation_depth.is_some() {
            return Err(OdfError::MalformedContent);
        }
        count_and_report_attributes(
            reader,
            element,
            attributes,
            attribute_bytes,
            limits,
            reporter,
        )?;
        note.body_seen = true;
        if !empty {
            note.body_depth = Some(depth);
        }
        return Ok(true);
    }
    Err(OdfError::MalformedContent)
}

#[allow(clippy::too_many_arguments)]
fn finish_note(
    limits: OdfImportLimits,
    current: &mut Option<ParagraphDraft>,
    active_link: &mut Option<LinkDraft>,
    active_run_properties: &mut RunProperties,
    open_spans: &mut Vec<OpenSpan>,
    open_lists: &mut Vec<OpenList>,
    open_list_items: &mut Vec<OpenListItem>,
    open_bookmarks: &mut BTreeMap<String, usize>,
    open_tables: &[OpenTable],
    inline_nodes: &mut usize,
    notes: &mut Vec<NoteDraft>,
    open_note: &mut Option<OpenNote>,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    let note = open_note.take().ok_or(OdfError::MalformedContent)?;
    if !note.body_seen
        || note.body_depth.is_some()
        || note.citation_depth.is_some()
        || current.is_some()
        || active_link.is_some()
        || !open_spans.is_empty()
        || !open_lists.is_empty()
        || !open_list_items.is_empty()
        || !open_bookmarks.is_empty()
        || open_tables.len() != note.table_stack_len
    {
        return Err(OdfError::MalformedContent);
    }
    if note.citation_has_text {
        reporter.report(
            "odf.element.text.note-citation".to_owned(),
            ModelOutcome::Degraded,
        );
    }
    let index = notes.len();
    let kind = note.kind;
    notes.push(NoteDraft {
        kind,
        blocks: note.blocks,
    });
    *current = Some(note.outer.paragraph);
    *active_link = note.outer.active_link;
    *active_run_properties = note.outer.active_run_properties;
    *open_spans = note.outer.open_spans;
    *open_lists = note.outer.open_lists;
    *open_list_items = note.outer.open_list_items;
    *open_bookmarks = note.outer.open_bookmarks;
    *inline_nodes = checked_increment(*inline_nodes)?;
    enforce(
        "odf_content_inline_nodes",
        *inline_nodes,
        limits.max_inline_nodes,
    )?;
    push_inline_draft(
        current.as_mut().ok_or(OdfError::MalformedContent)?,
        active_link,
        InlineDraft::NoteReference { index, kind },
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn process_start(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    name: &ResolvedName,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    body_seen: &mut bool,
    body_depth: &mut Option<usize>,
    text_body_depth: &mut Option<usize>,
    leaf_depth: &mut Option<usize>,
    body_kind_seen: &mut bool,
    current: &mut Option<ParagraphDraft>,
    active_link: &mut Option<LinkDraft>,
    bookmarks: &mut Vec<BookmarkDraft>,
    open_bookmarks: &mut BTreeMap<String, usize>,
    inline_nodes: &mut usize,
    text_bytes: &mut usize,
    automatic_styles: &AutomaticStyles,
    open_lists: &[OpenList],
    open_list_items: &mut [OpenListItem],
    active_run_properties: &mut RunProperties,
    open_spans: &mut Vec<OpenSpan>,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    if is_name(name, NamespaceKind::Office, b"body") {
        if *body_seen || body_depth.is_some() || depth != 2 {
            return Err(OdfError::MalformedContent);
        }
        *body_seen = true;
        *body_depth = Some(depth);
        count_and_report_attributes(
            reader,
            element,
            attributes,
            attribute_bytes,
            limits,
            reporter,
        )?;
    } else if body_depth.is_some() && depth == body_depth.unwrap_or(0) + 1 {
        if is_name(name, NamespaceKind::Office, b"text") {
            if *body_kind_seen {
                return Err(OdfError::MalformedContent);
            }
            *body_kind_seen = true;
            *text_body_depth = Some(depth);
            count_and_report_attributes(
                reader,
                element,
                attributes,
                attribute_bytes,
                limits,
                reporter,
            )?;
        } else if is_office_body_kind(name) {
            return Err(OdfError::UnsupportedDocumentKind);
        } else {
            return Err(OdfError::MalformedContent);
        }
    } else if text_body_depth.is_some() && is_name(name, NamespaceKind::Text, b"p") {
        let numbering = list_numbering(depth, open_lists, open_list_items)?;
        start_paragraph(
            reader,
            element,
            depth,
            false,
            limits,
            attributes,
            attribute_bytes,
            current,
            automatic_styles,
            numbering,
            reporter,
        )?;
    } else if text_body_depth.is_some() && is_name(name, NamespaceKind::Text, b"h") {
        let numbering = list_numbering(depth, open_lists, open_list_items)?;
        start_paragraph(
            reader,
            element,
            depth,
            true,
            limits,
            attributes,
            attribute_bytes,
            current,
            automatic_styles,
            numbering,
            reporter,
        )?;
    } else if current.is_some() {
        let is_leaf = is_name(name, NamespaceKind::Text, b"s")
            || is_name(name, NamespaceKind::Text, b"tab")
            || is_name(name, NamespaceKind::Text, b"line-break")
            || is_bookmark_element(name);
        process_inline(
            reader,
            element,
            name,
            limits,
            attributes,
            attribute_bytes,
            current,
            active_link,
            bookmarks,
            open_bookmarks,
            depth,
            inline_nodes,
            text_bytes,
            automatic_styles,
            active_run_properties,
            open_spans,
            reporter,
        )?;
        if is_leaf {
            *leaf_depth = Some(depth);
        }
    } else {
        count_and_report_attributes(
            reader,
            element,
            attributes,
            attribute_bytes,
            limits,
            reporter,
        )?;
        if text_body_depth.is_some() {
            reporter.report(feature("element", name), ModelOutcome::Degraded);
        } else if depth == 2 {
            reporter.report(feature("element", name), ModelOutcome::Omitted);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn process_empty(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    name: &ResolvedName,
    depth: usize,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    body_seen: &mut bool,
    body_depth: Option<usize>,
    text_body_depth: Option<usize>,
    body_kind_seen: &mut bool,
    current: &mut Option<ParagraphDraft>,
    active_link: &mut Option<LinkDraft>,
    bookmarks: &mut Vec<BookmarkDraft>,
    open_bookmarks: &mut BTreeMap<String, usize>,
    paragraphs: &mut Vec<ParagraphDraft>,
    blocks: &mut Vec<BlockDraft>,
    open_tables: &mut [OpenTable],
    open_note: &mut Option<OpenNote>,
    inline_nodes: &mut usize,
    text_bytes: &mut usize,
    automatic_styles: &AutomaticStyles,
    open_lists: &[OpenList],
    open_list_items: &mut [OpenListItem],
    active_run_properties: &mut RunProperties,
    open_spans: &mut Vec<OpenSpan>,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    if is_name(name, NamespaceKind::Office, b"body") && depth == 2 {
        if *body_seen {
            return Err(OdfError::MalformedContent);
        }
        *body_seen = true;
        count_and_report_attributes(
            reader,
            element,
            attributes,
            attribute_bytes,
            limits,
            reporter,
        )?;
    } else if is_name(name, NamespaceKind::Office, b"text") && body_depth == Some(2) && depth == 3 {
        if *body_kind_seen {
            return Err(OdfError::MalformedContent);
        }
        *body_kind_seen = true;
        count_and_report_attributes(
            reader,
            element,
            attributes,
            attribute_bytes,
            limits,
            reporter,
        )?;
    } else if text_body_depth.is_some()
        && (is_name(name, NamespaceKind::Text, b"p") || is_name(name, NamespaceKind::Text, b"h"))
    {
        let numbering = list_numbering(depth, open_lists, open_list_items)?;
        start_paragraph(
            reader,
            element,
            depth,
            is_name(name, NamespaceKind::Text, b"h"),
            limits,
            attributes,
            attribute_bytes,
            current,
            automatic_styles,
            numbering,
            reporter,
        )?;
        push_paragraph_block(
            paragraphs,
            blocks,
            open_tables,
            open_note,
            current.take().ok_or(OdfError::MalformedContent)?,
            limits,
        )?;
    } else if current.is_some() {
        process_inline(
            reader,
            element,
            name,
            limits,
            attributes,
            attribute_bytes,
            current,
            active_link,
            bookmarks,
            open_bookmarks,
            depth,
            inline_nodes,
            text_bytes,
            automatic_styles,
            active_run_properties,
            open_spans,
            reporter,
        )?;
        if is_name(name, NamespaceKind::Text, b"a") {
            finish_link(current, active_link, reporter)?;
        } else if is_name(name, NamespaceKind::Text, b"span") {
            let span = open_spans.pop().ok_or(OdfError::MalformedContent)?;
            if span.depth != depth {
                return Err(OdfError::MalformedContent);
            }
            *active_run_properties = span.previous_properties;
        }
    } else {
        count_and_report_attributes(
            reader,
            element,
            attributes,
            attribute_bytes,
            limits,
            reporter,
        )?;
        if text_body_depth.is_some() {
            reporter.report(feature("element", name), ModelOutcome::Degraded);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn start_paragraph(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    depth: usize,
    heading: bool,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    current: &mut Option<ParagraphDraft>,
    automatic_styles: &AutomaticStyles,
    numbering: Option<ListParagraphDraft>,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    if current.is_some() {
        return Err(OdfError::MalformedContent);
    }
    let mut outline_level = None;
    let mut alignment = None;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        if heading
            && namespace_kind(&namespace) == NamespaceKind::Text
            && local.as_ref() == b"outline-level"
        {
            let raw = decode_attribute(&attribute)?;
            let parsed = raw.parse::<u16>().map_err(|_| OdfError::MalformedContent)?;
            if parsed == 0 {
                return Err(OdfError::MalformedContent);
            }
            outline_level = Some(
                u8::try_from(parsed.saturating_sub(1).min(9))
                    .map_err(|_| OdfError::MalformedContent)?,
            );
            if parsed > 10 {
                reporter.report(
                    "odf.attribute.text.outline-level".to_owned(),
                    ModelOutcome::Degraded,
                );
            }
        } else if namespace_kind(&namespace) == NamespaceKind::Text
            && local.as_ref() == b"style-name"
        {
            let style_name = decode_attribute(&attribute)?;
            if let Some(style) = automatic_styles.get(&(StyleFamily::Paragraph, style_name)) {
                alignment = style.alignment;
            } else {
                reporter.report("odf.style.unresolved".to_owned(), ModelOutcome::Degraded);
            }
        } else if !is_namespace_declaration(&attribute) {
            reporter.report(
                attribute_feature(reader, &attribute),
                ModelOutcome::Degraded,
            );
        }
    }
    if heading && outline_level.is_none() {
        return Err(OdfError::MalformedContent);
    }
    *current = Some(ParagraphDraft {
        depth,
        outline_level,
        alignment,
        numbering,
        inlines: Vec::new(),
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn process_inline(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    name: &ResolvedName,
    limits: OdfImportLimits,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    current: &mut Option<ParagraphDraft>,
    active_link: &mut Option<LinkDraft>,
    bookmarks: &mut Vec<BookmarkDraft>,
    open_bookmarks: &mut BTreeMap<String, usize>,
    depth: usize,
    inline_nodes: &mut usize,
    text_bytes: &mut usize,
    automatic_styles: &AutomaticStyles,
    active_run_properties: &mut RunProperties,
    open_spans: &mut Vec<OpenSpan>,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    if is_name(name, NamespaceKind::Text, b"s") {
        let mut count = 1_usize;
        for attribute in element.attributes() {
            let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
            count_attribute(&attribute, attributes, attribute_bytes, limits)?;
            let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
            if namespace_kind(&namespace) == NamespaceKind::Text && local.as_ref() == b"c" {
                count = decode_attribute(&attribute)?
                    .parse()
                    .map_err(|_| OdfError::MalformedContent)?;
                if count == 0 {
                    return Err(OdfError::MalformedContent);
                }
            } else if !is_namespace_declaration(&attribute) {
                reporter.report(
                    attribute_feature(reader, &attribute),
                    ModelOutcome::Degraded,
                );
            }
        }
        enforce("odf_content_space_repeat", count, limits.max_space_repeat)?;
        let spaces = " ".repeat(count);
        append_text(
            current.as_mut().ok_or(OdfError::MalformedContent)?,
            active_link,
            &spaces,
            active_run_properties,
            text_bytes,
            limits,
        )?;
    } else if is_name(name, NamespaceKind::Text, b"tab")
        || is_name(name, NamespaceKind::Text, b"line-break")
    {
        count_and_report_attributes(
            reader,
            element,
            attributes,
            attribute_bytes,
            limits,
            reporter,
        )?;
        *inline_nodes = checked_increment(*inline_nodes)?;
        enforce(
            "odf_content_inline_nodes",
            *inline_nodes,
            limits.max_inline_nodes,
        )?;
        push_inline_draft(
            current.as_mut().ok_or(OdfError::MalformedContent)?,
            active_link,
            if is_name(name, NamespaceKind::Text, b"tab") {
                InlineDraft::Tab
            } else {
                InlineDraft::LineBreak
            },
        );
    } else if is_name(name, NamespaceKind::Text, b"a") {
        if active_link.is_some() {
            return Err(OdfError::MalformedContent);
        }
        *active_link = Some(LinkDraft {
            depth,
            target: read_link_target(
                reader,
                element,
                attributes,
                attribute_bytes,
                limits,
                reporter,
            )?,
            inlines: Vec::new(),
        });
    } else if is_bookmark_element(name) {
        let name_value = read_bookmark_name(
            reader,
            element,
            attributes,
            attribute_bytes,
            limits,
            reporter,
        )?;
        process_bookmark(
            name,
            name_value,
            current.as_mut().ok_or(OdfError::MalformedContent)?,
            active_link,
            bookmarks,
            open_bookmarks,
            reporter,
        )?;
    } else if is_name(name, NamespaceKind::Text, b"span") {
        let properties = read_span_properties(
            reader,
            element,
            attributes,
            attribute_bytes,
            limits,
            automatic_styles,
            reporter,
        )?;
        let previous_properties = active_run_properties.clone();
        merge_run_properties(active_run_properties, &properties);
        open_spans.push(OpenSpan {
            depth,
            previous_properties,
        });
    } else {
        count_and_report_attributes(
            reader,
            element,
            attributes,
            attribute_bytes,
            limits,
            reporter,
        )?;
        reporter.report(feature("element", name), ModelOutcome::Degraded);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn read_span_properties(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    limits: OdfImportLimits,
    automatic_styles: &AutomaticStyles,
    reporter: &mut Reporter,
) -> Result<RunProperties, OdfError> {
    let mut properties = RunProperties::default();
    let mut style_name = None;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        if namespace_kind(&namespace) == NamespaceKind::Text && local.as_ref() == b"style-name" {
            if style_name.is_some() {
                return Err(OdfError::MalformedContent);
            }
            style_name = Some(decode_attribute(&attribute)?);
        } else if !is_namespace_declaration(&attribute) {
            reporter.report(
                attribute_feature(reader, &attribute),
                ModelOutcome::Degraded,
            );
        }
    }
    if let Some(style_name) = style_name {
        if let Some(style) = automatic_styles.get(&(StyleFamily::Text, style_name)) {
            properties = style.run_properties.clone();
        } else {
            reporter.report("odf.style.unresolved".to_owned(), ModelOutcome::Degraded);
        }
    }
    Ok(properties)
}

fn merge_run_properties(target: &mut RunProperties, overlay: &RunProperties) {
    if overlay.bold.is_some() {
        target.bold = overlay.bold;
    }
    if overlay.italic.is_some() {
        target.italic = overlay.italic;
    }
    if overlay.underline.is_some() {
        target.underline = overlay.underline;
    }
    if overlay.strike.is_some() {
        target.strike = overlay.strike;
    }
    if overlay.color.is_some() {
        target.color = overlay.color;
    }
    if overlay.size_half_points.is_some() {
        target.size_half_points = overlay.size_half_points;
    }
}

fn append_text(
    paragraph: &mut ParagraphDraft,
    active_link: &mut Option<LinkDraft>,
    value: &str,
    properties: &RunProperties,
    text_bytes: &mut usize,
    limits: OdfImportLimits,
) -> Result<(), OdfError> {
    *text_bytes = text_bytes
        .checked_add(value.len())
        .ok_or(OdfError::LimitExceeded {
            limit: "odf_content_text_bytes",
            observed: usize::MAX,
            allowed: limits.max_text_bytes,
        })?;
    enforce("odf_content_text_bytes", *text_bytes, limits.max_text_bytes)?;
    if let Some(link) = active_link {
        push_text_draft(&mut link.inlines, value, properties);
    } else {
        paragraph.push_text(value, properties);
    }
    Ok(())
}

fn push_inline_draft(
    paragraph: &mut ParagraphDraft,
    active_link: &mut Option<LinkDraft>,
    inline: InlineDraft,
) {
    if let Some(link) = active_link {
        link.inlines.push(inline);
    } else {
        paragraph.inlines.push(inline);
    }
}

fn finish_link(
    current: &mut Option<ParagraphDraft>,
    active_link: &mut Option<LinkDraft>,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    let link = active_link.take().ok_or(OdfError::MalformedContent)?;
    let paragraph = current.as_mut().ok_or(OdfError::MalformedContent)?;
    if let Some(target) = link.target
        && !link.inlines.is_empty()
    {
        paragraph.inlines.push(InlineDraft::Hyperlink {
            target,
            inlines: link.inlines,
        });
    } else {
        if link.inlines.is_empty() {
            reporter.report("odf.element.text.a".to_owned(), ModelOutcome::Omitted);
        }
        for inline in link.inlines {
            match inline {
                InlineDraft::Text { text, properties } => {
                    paragraph.push_text(&text, properties.as_ref());
                }
                inline => paragraph.inlines.push(inline),
            }
        }
    }
    Ok(())
}

fn read_link_target(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    limits: OdfImportLimits,
    reporter: &mut Reporter,
) -> Result<Option<HyperlinkTarget>, OdfError> {
    let mut href = None;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        if namespace_kind(&namespace) == NamespaceKind::Xlink && local.as_ref() == b"href" {
            if href.is_some() {
                return Err(OdfError::MalformedContent);
            }
            href = Some(decode_attribute(&attribute)?);
        } else if namespace_kind(&namespace) == NamespaceKind::Xlink
            && local.as_ref() == b"type"
            && decode_attribute(&attribute)? == "simple"
        {
            // The model's hyperlink target already implies the simple-link type.
        } else if !is_namespace_declaration(&attribute) {
            reporter.report(
                attribute_feature(reader, &attribute),
                ModelOutcome::Degraded,
            );
        }
    }
    let Some(href) = href else {
        reporter.report("odf.element.text.a".to_owned(), ModelOutcome::Degraded);
        return Ok(None);
    };
    let blocked_scheme = has_blocked_link_scheme(&href);
    let target = if let Some(anchor) = href.strip_prefix('#') {
        if anchor.is_empty() || anchor.len() > 255 {
            None
        } else {
            Some(HyperlinkTarget::Internal(InternalTarget {
                anchor: anchor.to_owned(),
            }))
        }
    } else if href.is_empty() || href.len() > 2_048 || blocked_scheme {
        if blocked_scheme {
            reporter.report_with_retention(
                "odf.hyperlink.blocked-scheme".to_owned(),
                ModelOutcome::Degraded,
                RetentionOutcome::Blocked,
            );
        }
        None
    } else {
        Some(HyperlinkTarget::External(ExternalTarget { url: href }))
    };
    if target.is_none() {
        reporter.report("odf.element.text.a".to_owned(), ModelOutcome::Degraded);
    }
    Ok(target)
}

fn has_blocked_link_scheme(href: &str) -> bool {
    let Some((scheme, _)) = href.split_once(':') else {
        return false;
    };
    !matches!(
        scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "mailto"
    )
}

fn is_bookmark_element(name: &ResolvedName) -> bool {
    name.namespace == NamespaceKind::Text
        && matches!(
            name.local.as_slice(),
            b"bookmark" | b"bookmark-start" | b"bookmark-end"
        )
}

fn read_bookmark_name(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    limits: OdfImportLimits,
    reporter: &mut Reporter,
) -> Result<Option<String>, OdfError> {
    let mut name = None;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        if namespace_kind(&namespace) == NamespaceKind::Text && local.as_ref() == b"name" {
            if name.is_some() {
                return Err(OdfError::MalformedContent);
            }
            name = Some(decode_attribute(&attribute)?);
        } else if !is_namespace_declaration(&attribute) {
            reporter.report(
                attribute_feature(reader, &attribute),
                ModelOutcome::Degraded,
            );
        }
    }
    if name
        .as_ref()
        .is_none_or(|name| name.is_empty() || name.len() > 255)
    {
        reporter.report("odf.attribute.text.name".to_owned(), ModelOutcome::Omitted);
        Ok(None)
    } else {
        Ok(name)
    }
}

fn process_bookmark(
    element_name: &ResolvedName,
    name: Option<String>,
    paragraph: &mut ParagraphDraft,
    active_link: &mut Option<LinkDraft>,
    bookmarks: &mut Vec<BookmarkDraft>,
    open_bookmarks: &mut BTreeMap<String, usize>,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    let Some(name) = name else {
        return Ok(());
    };
    if element_name.local == b"bookmark" {
        let index = bookmarks.len();
        bookmarks.push(BookmarkDraft { name, paired: true });
        push_inline_draft(paragraph, active_link, InlineDraft::BookmarkStart(index));
        push_inline_draft(paragraph, active_link, InlineDraft::BookmarkEnd(index));
    } else if element_name.local == b"bookmark-start" {
        if open_bookmarks.contains_key(&name) {
            return Err(OdfError::MalformedContent);
        }
        let index = bookmarks.len();
        bookmarks.push(BookmarkDraft {
            name: name.clone(),
            paired: false,
        });
        open_bookmarks.insert(name, index);
        push_inline_draft(paragraph, active_link, InlineDraft::BookmarkStart(index));
    } else {
        let Some(index) = open_bookmarks.remove(&name) else {
            reporter.report(
                "odf.element.text.bookmark-end".to_owned(),
                ModelOutcome::Omitted,
            );
            return Ok(());
        };
        bookmarks[index].paired = true;
        push_inline_draft(paragraph, active_link, InlineDraft::BookmarkEnd(index));
    }
    Ok(())
}

fn count_inline_drafts(inlines: &[InlineDraft], mut total: usize) -> Result<usize, OdfError> {
    for inline in inlines {
        total = total.checked_add(1).ok_or(OdfError::LimitExceeded {
            limit: "odf_content_inline_nodes",
            observed: usize::MAX,
            allowed: usize::MAX,
        })?;
        if let InlineDraft::Hyperlink { inlines, .. } = inline {
            total = count_inline_drafts(inlines, total)?;
        }
    }
    Ok(total)
}

#[derive(Default)]
struct ExpandedBlockCounts {
    paragraphs: usize,
    inline_nodes: usize,
    text_bytes: usize,
    tables: usize,
    table_rows: usize,
    table_cells: usize,
}

fn enforce_expanded_block_limits(
    blocks: &[BlockDraft],
    notes: &[NoteDraft],
    paragraphs: &[ParagraphDraft],
    limits: OdfImportLimits,
) -> Result<(), OdfError> {
    let mut counts = ExpandedBlockCounts::default();
    count_expanded_blocks(blocks, paragraphs, limits, &mut counts)?;
    enforce("odf_content_notes", notes.len(), limits.max_notes)?;
    for note in notes {
        count_expanded_blocks(&note.blocks, paragraphs, limits, &mut counts)?;
    }
    Ok(())
}

fn count_expanded_blocks(
    blocks: &[BlockDraft],
    paragraphs: &[ParagraphDraft],
    limits: OdfImportLimits,
    counts: &mut ExpandedBlockCounts,
) -> Result<(), OdfError> {
    for block in blocks {
        match block {
            BlockDraft::Paragraph(index) => {
                let paragraph = paragraphs.get(*index).ok_or(OdfError::InvalidModel)?;
                count_expanded_paragraph(paragraph, limits, counts)?;
            }
            BlockDraft::Table(table) => {
                counts.tables = checked_increment(counts.tables)?;
                enforce("odf_content_tables", counts.tables, limits.max_tables)?;
                let owners = table_owners(table)?;
                for (row_index, row) in table.rows.iter().enumerate() {
                    counts.table_rows = checked_increment(counts.table_rows)?;
                    enforce(
                        "odf_content_table_rows",
                        counts.table_rows,
                        limits.max_table_rows,
                    )?;
                    for (column_index, slot) in row.slots.iter().enumerate() {
                        counts.table_cells = checked_increment(counts.table_cells)?;
                        enforce(
                            "odf_content_table_cells",
                            counts.table_cells,
                            limits.max_table_cells,
                        )?;
                        match slot {
                            TableSlotDraft::Cell(cell) => {
                                if cell.blocks.is_empty() {
                                    count_empty_paragraph(limits, counts)?;
                                } else {
                                    count_expanded_blocks(
                                        &cell.blocks,
                                        paragraphs,
                                        limits,
                                        counts,
                                    )?;
                                }
                            }
                            TableSlotDraft::Covered => {
                                let (anchor_row, anchor_column) = owners
                                    .get(&(row_index, column_index))
                                    .copied()
                                    .ok_or(OdfError::InvalidModel)?;
                                if row_index > anchor_row && column_index == anchor_column {
                                    count_empty_paragraph(limits, counts)?;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn count_expanded_paragraph(
    paragraph: &ParagraphDraft,
    limits: OdfImportLimits,
    counts: &mut ExpandedBlockCounts,
) -> Result<(), OdfError> {
    count_empty_paragraph(limits, counts)?;
    counts.inline_nodes = count_inline_drafts(&paragraph.inlines, counts.inline_nodes)?;
    enforce(
        "odf_content_inline_nodes",
        counts.inline_nodes,
        limits.max_inline_nodes,
    )?;
    let added_text = inline_draft_text_bytes(&paragraph.inlines)?;
    counts.text_bytes = counts
        .text_bytes
        .checked_add(added_text)
        .ok_or(OdfError::MalformedContent)?;
    enforce(
        "odf_content_text_bytes",
        counts.text_bytes,
        limits.max_text_bytes,
    )
}

fn count_empty_paragraph(
    limits: OdfImportLimits,
    counts: &mut ExpandedBlockCounts,
) -> Result<(), OdfError> {
    counts.paragraphs = checked_increment(counts.paragraphs)?;
    enforce(
        "odf_content_paragraphs",
        counts.paragraphs,
        limits.max_paragraphs,
    )
}

fn inline_draft_text_bytes(inlines: &[InlineDraft]) -> Result<usize, OdfError> {
    let mut total = 0_usize;
    for inline in inlines {
        match inline {
            InlineDraft::Text { text, .. } => {
                total = total
                    .checked_add(text.len())
                    .ok_or(OdfError::MalformedContent)?;
            }
            InlineDraft::Hyperlink { inlines, .. } => {
                total = total
                    .checked_add(inline_draft_text_bytes(inlines)?)
                    .ok_or(OdfError::MalformedContent)?;
            }
            InlineDraft::Tab
            | InlineDraft::LineBreak
            | InlineDraft::BookmarkStart(_)
            | InlineDraft::BookmarkEnd(_)
            | InlineDraft::NoteReference { .. } => {}
        }
    }
    Ok(total)
}

fn normalize_inline_drafts(
    inlines: &mut Vec<InlineDraft>,
    bookmarks: &[BookmarkDraft],
    reporter: &mut Reporter,
) {
    let mut normalized = Vec::with_capacity(inlines.len());
    for mut inline in std::mem::take(inlines) {
        if let InlineDraft::Hyperlink {
            inlines: children, ..
        } = &mut inline
        {
            normalize_inline_drafts(children, bookmarks, reporter);
            if children.is_empty() {
                reporter.report("odf.element.text.a".to_owned(), ModelOutcome::Omitted);
                continue;
            }
        }
        if let InlineDraft::BookmarkStart(index) | InlineDraft::BookmarkEnd(index) = &inline
            && !bookmarks
                .get(*index)
                .is_some_and(|bookmark| bookmark.paired)
        {
            continue;
        }
        if let InlineDraft::Text { text, properties } = inline {
            push_text_draft(&mut normalized, &text, properties.as_ref());
        } else {
            normalized.push(inline);
        }
    }
    *inlines = normalized;
}

fn push_paragraph(
    paragraphs: &mut Vec<ParagraphDraft>,
    paragraph: ParagraphDraft,
    limits: OdfImportLimits,
) -> Result<(), OdfError> {
    let observed = paragraphs
        .len()
        .checked_add(1)
        .ok_or(OdfError::LimitExceeded {
            limit: "odf_content_paragraphs",
            observed: usize::MAX,
            allowed: limits.max_paragraphs,
        })?;
    enforce("odf_content_paragraphs", observed, limits.max_paragraphs)?;
    paragraphs.push(paragraph);
    Ok(())
}

fn push_paragraph_block(
    paragraphs: &mut Vec<ParagraphDraft>,
    blocks: &mut Vec<BlockDraft>,
    open_tables: &mut [OpenTable],
    open_note: &mut Option<OpenNote>,
    paragraph: ParagraphDraft,
    limits: OdfImportLimits,
) -> Result<(), OdfError> {
    let index = paragraphs.len();
    push_paragraph(paragraphs, paragraph, limits)?;
    push_block_draft(blocks, open_tables, open_note, BlockDraft::Paragraph(index))
}

fn build_document(
    version: OdfVersion,
    paragraphs: &[ParagraphDraft],
    blocks: &[BlockDraft],
    notes: &[NoteDraft],
    bookmarks: &[BookmarkDraft],
    list_styles: &ListStyles,
    reporter: &mut Reporter,
) -> Result<Document, OdfError> {
    let mut namespace = 0xcbf2_9ce4_8422_2325_u64;
    hash_bytes(&mut namespace, version.as_str().as_bytes());
    for paragraph in paragraphs {
        hash_bytes(&mut namespace, b"p");
        hash_bytes(
            &mut namespace,
            &[paragraph.outline_level.unwrap_or(u8::MAX)],
        );
        if let Some(numbering) = &paragraph.numbering {
            hash_bytes(&mut namespace, b"list");
            hash_bytes(&mut namespace, &numbering.instance.to_le_bytes());
            hash_bytes(&mut namespace, &[numbering.level]);
            if let Some(style_name) = &numbering.style_name {
                hash_bytes(&mut namespace, style_name.as_bytes());
            }
        }
        hash_bytes(
            &mut namespace,
            &[match paragraph.alignment {
                None => u8::MAX,
                Some(Alignment::Start) => 0,
                Some(Alignment::End) => 1,
                Some(Alignment::Center) => 2,
                Some(Alignment::Justify) => 3,
            }],
        );
        for inline in &paragraph.inlines {
            hash_inline_draft(&mut namespace, inline, bookmarks);
        }
    }
    hash_block_drafts(&mut namespace, blocks);
    for note in notes {
        hash_bytes(&mut namespace, b"note");
        hash_bytes(
            &mut namespace,
            match note.kind {
                NoteKind::Footnote => b"footnote".as_slice(),
                NoteKind::Endnote => b"endnote".as_slice(),
            },
        );
        hash_block_drafts(&mut namespace, &note.blocks);
    }
    if namespace == 0 {
        namespace = 1;
    }
    let mut ids = IdGenerator::new(namespace);
    let document_id = ids.next_id().map_err(|_| OdfError::InvalidModel)?;
    let mut definitions = Definitions::default();
    let mut list_levels = BTreeMap::<usize, BTreeMap<u8, OdfListLevel>>::new();
    for paragraph in paragraphs {
        let Some(numbering) = &paragraph.numbering else {
            continue;
        };
        let resolved = numbering
            .style_name
            .as_ref()
            .and_then(|name| list_styles.get(name))
            .and_then(|levels| levels.get(&numbering.level))
            .cloned()
            .unwrap_or_else(|| {
                reporter.report(
                    "odf.list-style.missing-level".to_owned(),
                    ModelOutcome::Degraded,
                );
                OdfListLevel {
                    level: numbering.level,
                    num_fmt: NumberFormat::Bullet,
                    lvl_text: "\u{2022}".to_owned(),
                    start: 1,
                }
            });
        if let Some(previous) = list_levels
            .entry(numbering.instance)
            .or_default()
            .insert(numbering.level, resolved.clone())
            && previous != resolved
        {
            reporter.report(
                "odf.list-style.level-conflict".to_owned(),
                ModelOutcome::Degraded,
            );
        }
    }
    let mut numbering_ids = BTreeMap::new();
    for (instance, levels) in list_levels {
        let abstract_id =
            AbstractNumberingId::new(ids.next_id().map_err(|_| OdfError::InvalidModel)?);
        let instance_id =
            NumberingInstanceId::new(ids.next_id().map_err(|_| OdfError::InvalidModel)?);
        definitions.abstract_numbering.insert(
            abstract_id,
            AbstractNumbering {
                levels: levels
                    .into_values()
                    .map(|level| NumberingLevel {
                        level: level.level,
                        start: level.start,
                        num_fmt: Some(level.num_fmt),
                        lvl_text: Some(level.lvl_text),
                        lvl_jc: Some(LevelJustification::Start),
                        suff: Some(LevelSuffix::Tab),
                        is_lgl: false,
                        paragraph_properties: None,
                        run_properties: None,
                        style_ref: None,
                        pstyle: None,
                        lvl_restart: None,
                    })
                    .collect(),
                multi_level_type: None,
                num_style_link: None,
                style_link: None,
            },
        );
        definitions.numbering.insert(
            instance_id,
            NumberingInstance {
                abstract_ref: abstract_id,
                overrides: Vec::new(),
            },
        );
        numbering_ids.insert(instance, instance_id);
    }
    let mut bookmark_ids = Vec::with_capacity(bookmarks.len());
    for bookmark in bookmarks {
        if bookmark.paired {
            let id = BookmarkId::new(ids.next_id().map_err(|_| OdfError::InvalidModel)?);
            definitions.bookmarks.insert(
                id,
                Bookmark {
                    name: bookmark.name.clone(),
                },
            );
            bookmark_ids.push(Some(id));
        } else {
            bookmark_ids.push(None);
        }
    }
    let mut note_ids = Vec::with_capacity(notes.len());
    for _ in notes {
        note_ids.push(NoteId::new(
            ids.next_id().map_err(|_| OdfError::InvalidModel)?,
        ));
    }
    for (index, note) in notes.iter().enumerate() {
        let blocks = build_blocks(
            &note.blocks,
            paragraphs,
            &mut ids,
            &bookmark_ids,
            &numbering_ids,
            &note_ids,
        )?;
        match note.kind {
            NoteKind::Footnote => {
                definitions
                    .footnotes
                    .insert(note_ids[index], Note { blocks });
            }
            NoteKind::Endnote => {
                definitions
                    .endnotes
                    .insert(note_ids[index], Note { blocks });
            }
        }
    }
    let body = build_blocks(
        blocks,
        paragraphs,
        &mut ids,
        &bookmark_ids,
        &numbering_ids,
        &note_ids,
    )?;
    Document::new(document_id, body, definitions).map_err(|_| OdfError::InvalidModel)
}

fn hash_block_drafts(hash: &mut u64, blocks: &[BlockDraft]) {
    for block in blocks {
        match block {
            BlockDraft::Paragraph(index) => {
                hash_bytes(hash, b"block-paragraph");
                hash_bytes(hash, &index.to_le_bytes());
            }
            BlockDraft::Table(table) => {
                hash_bytes(hash, b"block-table");
                hash_bytes(hash, &table.columns.to_le_bytes());
                for row in &table.rows {
                    hash_bytes(hash, b"row");
                    hash_bytes(hash, &[u8::from(row.header)]);
                    for slot in &row.slots {
                        match slot {
                            TableSlotDraft::Covered => hash_bytes(hash, b"covered"),
                            TableSlotDraft::Cell(cell) => {
                                hash_bytes(hash, b"cell");
                                hash_bytes(hash, &cell.column_span.to_le_bytes());
                                hash_bytes(hash, &cell.row_span.to_le_bytes());
                                hash_block_drafts(hash, &cell.blocks);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn build_blocks(
    drafts: &[BlockDraft],
    paragraphs: &[ParagraphDraft],
    ids: &mut IdGenerator,
    bookmark_ids: &[Option<BookmarkId>],
    numbering_ids: &BTreeMap<usize, NumberingInstanceId>,
    note_ids: &[NoteId],
) -> Result<Vec<BlockNode>, OdfError> {
    let mut blocks = Vec::with_capacity(drafts.len());
    for draft in drafts {
        blocks.push(match draft {
            BlockDraft::Paragraph(index) => BlockNode::Paragraph(build_paragraph(
                paragraphs.get(*index).ok_or(OdfError::InvalidModel)?,
                ids,
                bookmark_ids,
                numbering_ids,
                note_ids,
            )?),
            BlockDraft::Table(table) => BlockNode::Table(build_table(
                table,
                paragraphs,
                ids,
                bookmark_ids,
                numbering_ids,
                note_ids,
            )?),
        });
    }
    Ok(blocks)
}

fn build_paragraph(
    draft: &ParagraphDraft,
    ids: &mut IdGenerator,
    bookmark_ids: &[Option<BookmarkId>],
    numbering_ids: &BTreeMap<usize, NumberingInstanceId>,
    note_ids: &[NoteId],
) -> Result<Paragraph, OdfError> {
    let id = ids.next_id().map_err(|_| OdfError::InvalidModel)?;
    let properties = ParagraphProperties {
        outline_level: draft.outline_level,
        alignment: draft.alignment,
        numbering: draft.numbering.as_ref().map(|numbering| NumberingRef {
            instance: numbering_ids[&numbering.instance],
            level: numbering.level,
        }),
        ..ParagraphProperties::default()
    };
    Ok(Paragraph {
        id,
        properties,
        inlines: build_inlines(&draft.inlines, ids, bookmark_ids, note_ids)?,
    })
}

fn build_empty_paragraph(ids: &mut IdGenerator) -> Result<BlockNode, OdfError> {
    Ok(BlockNode::Paragraph(Paragraph {
        id: ids.next_id().map_err(|_| OdfError::InvalidModel)?,
        properties: ParagraphProperties::default(),
        inlines: Vec::new(),
    }))
}

fn build_table(
    draft: &TableDraft,
    paragraphs: &[ParagraphDraft],
    ids: &mut IdGenerator,
    bookmark_ids: &[Option<BookmarkId>],
    numbering_ids: &BTreeMap<usize, NumberingInstanceId>,
    note_ids: &[NoteId],
) -> Result<Table, OdfError> {
    let owners = table_owners(draft)?;
    let id = ids.next_id().map_err(|_| OdfError::InvalidModel)?;
    let mut rows = Vec::with_capacity(draft.rows.len());
    for (row_index, row) in draft.rows.iter().enumerate() {
        let row_id = ids.next_id().map_err(|_| OdfError::InvalidModel)?;
        let mut cells = Vec::new();
        for (column_index, slot) in row.slots.iter().enumerate() {
            match slot {
                TableSlotDraft::Cell(cell) => {
                    let mut blocks = build_blocks(
                        &cell.blocks,
                        paragraphs,
                        ids,
                        bookmark_ids,
                        numbering_ids,
                        note_ids,
                    )?;
                    if blocks.is_empty() {
                        blocks.push(build_empty_paragraph(ids)?);
                    }
                    cells.push(TableCell {
                        id: ids.next_id().map_err(|_| OdfError::InvalidModel)?,
                        properties: TableCellProperties {
                            grid_span: (cell.column_span > 1).then_some(cell.column_span),
                            vertical_merge: (cell.row_span > 1).then_some(VerticalMerge::Restart),
                            ..TableCellProperties::default()
                        },
                        blocks,
                    });
                }
                TableSlotDraft::Covered => {
                    let (anchor_row, anchor_column) = owners
                        .get(&(row_index, column_index))
                        .copied()
                        .ok_or(OdfError::InvalidModel)?;
                    if row_index > anchor_row && column_index == anchor_column {
                        let anchor = match draft.rows[anchor_row].slots.get(anchor_column) {
                            Some(TableSlotDraft::Cell(cell)) => cell,
                            _ => return Err(OdfError::InvalidModel),
                        };
                        cells.push(TableCell {
                            id: ids.next_id().map_err(|_| OdfError::InvalidModel)?,
                            properties: TableCellProperties {
                                grid_span: (anchor.column_span > 1).then_some(anchor.column_span),
                                vertical_merge: Some(VerticalMerge::Continue),
                                ..TableCellProperties::default()
                            },
                            blocks: vec![build_empty_paragraph(ids)?],
                        });
                    }
                }
            }
        }
        if cells.is_empty() {
            return Err(OdfError::InvalidModel);
        }
        rows.push(TableRow {
            id: row_id,
            properties: TableRowProperties {
                header: row.header,
                ..TableRowProperties::default()
            },
            cells,
        });
    }
    Ok(Table {
        id,
        grid: (0..draft.columns)
            .map(|_| GridColumn { width_twips: None })
            .collect(),
        grid_change: None,
        properties: TableProperties::default(),
        rows,
    })
}

fn table_owners(table: &TableDraft) -> Result<TableOwners, OdfError> {
    let mut owners = BTreeMap::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        for (column_index, slot) in row.slots.iter().enumerate() {
            let TableSlotDraft::Cell(cell) = slot else {
                continue;
            };
            let row_end = row_index
                .checked_add(cell.row_span as usize)
                .ok_or(OdfError::InvalidModel)?;
            let column_end = column_index
                .checked_add(cell.column_span as usize)
                .ok_or(OdfError::InvalidModel)?;
            for covered_row in row_index..row_end {
                for covered_column in column_index..column_end {
                    owners.insert((covered_row, covered_column), (row_index, column_index));
                }
            }
        }
    }
    Ok(owners)
}

fn hash_inline_draft(hash: &mut u64, inline: &InlineDraft, bookmarks: &[BookmarkDraft]) {
    match inline {
        InlineDraft::Text { text, properties } => {
            hash_bytes(hash, b"t");
            hash_bytes(hash, text.as_bytes());
            hash_run_properties(hash, properties.as_ref());
        }
        InlineDraft::Tab => hash_bytes(hash, b"tab"),
        InlineDraft::LineBreak => hash_bytes(hash, b"br"),
        InlineDraft::Hyperlink { target, inlines } => {
            hash_bytes(hash, b"link");
            match target {
                HyperlinkTarget::External(target) => {
                    hash_bytes(hash, b"external");
                    hash_bytes(hash, target.url.as_bytes());
                }
                HyperlinkTarget::Internal(target) => {
                    hash_bytes(hash, b"internal");
                    hash_bytes(hash, target.anchor.as_bytes());
                }
            }
            for inline in inlines {
                hash_inline_draft(hash, inline, bookmarks);
            }
        }
        InlineDraft::BookmarkStart(index) | InlineDraft::BookmarkEnd(index) => {
            if bookmarks
                .get(*index)
                .is_some_and(|bookmark| bookmark.paired)
            {
                hash_bytes(
                    hash,
                    if matches!(inline, InlineDraft::BookmarkStart(_)) {
                        b"bookmark-start".as_slice()
                    } else {
                        b"bookmark-end".as_slice()
                    },
                );
                hash_bytes(hash, bookmarks[*index].name.as_bytes());
            }
        }
        InlineDraft::NoteReference { index, kind } => {
            hash_bytes(hash, b"note-reference");
            hash_bytes(hash, &index.to_le_bytes());
            hash_bytes(
                hash,
                match kind {
                    NoteKind::Footnote => b"footnote".as_slice(),
                    NoteKind::Endnote => b"endnote".as_slice(),
                },
            );
        }
    }
}

fn hash_run_properties(hash: &mut u64, properties: &RunProperties) {
    for value in [
        properties.bold,
        properties.italic,
        properties.underline,
        properties.strike,
    ] {
        hash_bytes(
            hash,
            &[match value {
                None => u8::MAX,
                Some(false) => 0,
                Some(true) => 1,
            }],
        );
    }
    match properties.color {
        Some(Color::Rgb(color)) => hash_bytes(hash, &[color.r, color.g, color.b]),
        _ => hash_bytes(hash, b"no-rgb"),
    }
    hash_bytes(
        hash,
        &properties
            .size_half_points
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
}

fn build_inlines(
    drafts: &[InlineDraft],
    ids: &mut IdGenerator,
    bookmark_ids: &[Option<BookmarkId>],
    note_ids: &[NoteId],
) -> Result<Vec<InlineNode>, OdfError> {
    let mut inlines = Vec::with_capacity(drafts.len());
    for draft in drafts {
        let bookmark = match draft {
            InlineDraft::BookmarkStart(index) | InlineDraft::BookmarkEnd(index) => {
                bookmark_ids.get(*index).copied().flatten()
            }
            _ => None,
        };
        if matches!(
            draft,
            InlineDraft::BookmarkStart(_) | InlineDraft::BookmarkEnd(_)
        ) && bookmark.is_none()
        {
            continue;
        }
        let id = ids.next_id().map_err(|_| OdfError::InvalidModel)?;
        inlines.push(match draft {
            InlineDraft::Text { text, properties } => InlineNode::Run(Run {
                id,
                properties: properties.as_ref().clone(),
                text: text.clone(),
            }),
            InlineDraft::Tab => InlineNode::Tab(Tab { id }),
            InlineDraft::LineBreak => InlineNode::Break(Break {
                id,
                kind: BreakKind::Line,
            }),
            InlineDraft::Hyperlink {
                target,
                inlines: children,
            } => InlineNode::Hyperlink(Hyperlink {
                id,
                target: target.clone(),
                tooltip: None,
                inlines: build_inlines(children, ids, bookmark_ids, note_ids)?,
            }),
            InlineDraft::BookmarkStart(_) => InlineNode::BookmarkStart(BookmarkStart {
                id,
                bookmark: bookmark.ok_or(OdfError::InvalidModel)?,
            }),
            InlineDraft::BookmarkEnd(_) => InlineNode::BookmarkEnd(BookmarkEnd {
                id,
                bookmark: bookmark.ok_or(OdfError::InvalidModel)?,
            }),
            InlineDraft::NoteReference { index, kind } => {
                InlineNode::NoteReference(NoteReference {
                    id,
                    kind: *kind,
                    note: *note_ids.get(*index).ok_or(OdfError::InvalidModel)?,
                })
            }
        });
    }
    Ok(inlines)
}

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    *hash ^= 0xff;
    *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
}

fn read_version(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    limits: OdfImportLimits,
    reporter: &mut Reporter,
) -> Result<OdfVersion, OdfError> {
    let mut version = None;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
        if namespace_kind(&namespace) == NamespaceKind::Office && local.as_ref() == b"version" {
            version = Some(decode_attribute(&attribute)?);
        } else if !is_namespace_declaration(&attribute) {
            reporter.report(
                attribute_feature(reader, &attribute),
                ModelOutcome::Degraded,
            );
        }
    }
    OdfVersion::parse(version.as_deref().ok_or(OdfError::UnsupportedVersion)?)
}

fn count_and_report_attributes(
    reader: &NsReader<&[u8]>,
    element: &BytesStart<'_>,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    limits: OdfImportLimits,
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
        if is_namespace_declaration(&attribute) {
            continue;
        }
        reporter.report(
            attribute_feature(reader, &attribute),
            ModelOutcome::Degraded,
        );
    }
    Ok(())
}

fn count_attributes_only(
    element: &BytesStart<'_>,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    limits: OdfImportLimits,
) -> Result<(), OdfError> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| OdfError::MalformedContent)?;
        count_attribute(&attribute, attributes, attribute_bytes, limits)?;
    }
    Ok(())
}

fn count_attribute(
    attribute: &quick_xml::events::attributes::Attribute<'_>,
    attributes: &mut usize,
    attribute_bytes: &mut usize,
    limits: OdfImportLimits,
) -> Result<(), OdfError> {
    enforce(
        "odf_content_xml_name_bytes",
        attribute.key.as_ref().len(),
        limits.max_xml_name_bytes,
    )?;
    *attributes = checked_increment(*attributes)?;
    enforce(
        "odf_content_xml_attributes",
        *attributes,
        limits.max_xml_attributes,
    )?;
    *attribute_bytes =
        attribute_bytes
            .checked_add(attribute.value.len())
            .ok_or(OdfError::LimitExceeded {
                limit: "odf_content_xml_attribute_bytes",
                observed: usize::MAX,
                allowed: limits.max_xml_attribute_bytes,
            })?;
    enforce(
        "odf_content_xml_attribute_bytes",
        *attribute_bytes,
        limits.max_xml_attribute_bytes,
    )
}

fn validate_name(element: &BytesStart<'_>, limits: OdfImportLimits) -> Result<(), OdfError> {
    enforce(
        "odf_content_xml_name_bytes",
        element.name().as_ref().len(),
        limits.max_xml_name_bytes,
    )
}

fn checked_increment(value: usize) -> Result<usize, OdfError> {
    value.checked_add(1).ok_or(OdfError::MalformedContent)
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

fn check_cancelled(cancellation: &CancellationToken) -> Result<(), OdfError> {
    if cancellation.is_cancelled() {
        Err(OdfError::Cancelled)
    } else {
        Ok(())
    }
}

fn decode_attribute(
    attribute: &quick_xml::events::attributes::Attribute<'_>,
) -> Result<String, OdfError> {
    let raw =
        core::str::from_utf8(attribute.value.as_ref()).map_err(|_| OdfError::MalformedContent)?;
    quick_xml::escape::unescape(raw)
        .map(|value| value.into_owned())
        .map_err(|_| OdfError::MalformedContent)
}

fn decode_reference(reference: &BytesRef<'_>) -> Result<String, OdfError> {
    let name = reference.decode().map_err(|_| OdfError::MalformedContent)?;
    let encoded = format!("&{name};");
    quick_xml::escape::unescape(&encoded)
        .map(|value| value.into_owned())
        .map_err(|_| OdfError::MalformedContent)
}

fn resolved_name(reader: &NsReader<&[u8]>, element: &BytesStart<'_>) -> ResolvedName {
    let (namespace, local) = reader.resolver().resolve_element(element.name());
    ResolvedName {
        namespace: namespace_kind(&namespace),
        local: local.as_ref().to_vec(),
    }
}

fn is_name(name: &ResolvedName, namespace: NamespaceKind, local: &[u8]) -> bool {
    name.namespace == namespace && name.local == local
}

fn namespace_kind(namespace: &ResolveResult<'_>) -> NamespaceKind {
    match namespace {
        ResolveResult::Bound(Namespace(value)) if *value == OFFICE_NS => NamespaceKind::Office,
        ResolveResult::Bound(Namespace(value)) if *value == TEXT_NS => NamespaceKind::Text,
        ResolveResult::Bound(Namespace(value)) if *value == SCRIPT_NS => NamespaceKind::Script,
        ResolveResult::Bound(Namespace(value)) if *value == XLINK_NS => NamespaceKind::Xlink,
        ResolveResult::Bound(Namespace(value)) if *value == STYLE_NS => NamespaceKind::Style,
        ResolveResult::Bound(Namespace(value)) if *value == FO_NS => NamespaceKind::Fo,
        ResolveResult::Bound(Namespace(value)) if *value == TABLE_NS => NamespaceKind::Table,
        _ => NamespaceKind::Foreign,
    }
}

fn is_office_body_kind(name: &ResolvedName) -> bool {
    name.namespace == NamespaceKind::Office
        && matches!(
            name.local.as_slice(),
            b"spreadsheet" | b"presentation" | b"drawing" | b"chart" | b"database"
        )
}

fn is_active(name: &ResolvedName) -> bool {
    (name.namespace == NamespaceKind::Office && name.local == b"scripts")
        || (name.namespace == NamespaceKind::Script && name.local == b"event-listener")
}

const fn namespace_label(namespace: NamespaceKind) -> &'static str {
    match namespace {
        NamespaceKind::Office => "office",
        NamespaceKind::Text => "text",
        NamespaceKind::Script => "script",
        NamespaceKind::Xlink => "xlink",
        NamespaceKind::Style => "style",
        NamespaceKind::Fo => "fo",
        NamespaceKind::Table => "table",
        NamespaceKind::Foreign => "foreign",
    }
}

fn feature(kind: &str, name: &ResolvedName) -> String {
    format!(
        "odf.{kind}.{}.{}",
        namespace_label(name.namespace),
        String::from_utf8_lossy(&name.local)
    )
}

fn attribute_feature(
    reader: &NsReader<&[u8]>,
    attribute: &quick_xml::events::attributes::Attribute<'_>,
) -> String {
    let (namespace, local) = reader.resolver().resolve_attribute(attribute.key);
    feature(
        "attribute",
        &ResolvedName {
            namespace: namespace_kind(&namespace),
            local: local.as_ref().to_vec(),
        },
    )
}

fn is_namespace_declaration(attribute: &quick_xml::events::attributes::Attribute<'_>) -> bool {
    let key = attribute.key.as_ref();
    key == b"xmlns" || key.starts_with(b"xmlns:")
}
