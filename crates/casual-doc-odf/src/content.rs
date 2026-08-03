//! Bounded, namespace-aware semantic import of ODT `content.xml`.

use std::collections::BTreeMap;

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{
    BlockNode, Bookmark, BookmarkEnd, BookmarkId, BookmarkStart, Break, BreakKind, Definitions,
    Document, ExternalTarget, Hyperlink, HyperlinkTarget, InlineNode, InternalTarget, Paragraph,
    ParagraphProperties, Run, RunProperties, Tab,
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
const MAX_NAMESPACE_DECLARATIONS_PER_ELEMENT: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NamespaceKind {
    Office,
    Text,
    Script,
    Xlink,
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
            max_xml_depth: 96,
            max_xml_elements: 1_000_000,
            max_xml_attributes: 3_000_000,
            max_xml_attribute_bytes: 128 * 1024 * 1024,
            max_xml_name_bytes: 1_024,
            max_paragraphs: 250_000,
            max_inline_nodes: 2_000_000,
            max_text_bytes: 128 * 1024 * 1024,
            max_space_repeat: 65_536,
            max_report_features: 4_096,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum InlineDraft {
    Text(String),
    Tab,
    LineBreak,
    Hyperlink {
        target: HyperlinkTarget,
        inlines: Vec<InlineDraft>,
    },
    BookmarkStart(usize),
    BookmarkEnd(usize),
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
    inlines: Vec<InlineDraft>,
}

impl ParagraphDraft {
    fn push_text(&mut self, value: &str) {
        push_text_draft(&mut self.inlines, value);
    }
}

fn push_text_draft(inlines: &mut Vec<InlineDraft>, value: &str) {
    if value.is_empty() {
        return;
    }
    if let Some(InlineDraft::Text(previous)) = inlines.last_mut() {
        previous.push_str(value);
    } else {
        inlines.push(InlineDraft::Text(value.to_owned()));
    }
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
    limits.validate()?;
    enforce("odf_content_bytes", bytes.len(), limits.max_content_bytes)?;
    check_cancelled(cancellation)?;

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
    let mut leaf_depth = None;
    let mut body_kind_seen = false;
    let mut current = None;
    let mut active_link = None;
    let mut paragraphs = Vec::new();
    let mut bookmarks = Vec::new();
    let mut open_bookmarks = BTreeMap::new();
    let mut reporter = Reporter::new(limits.max_report_features);

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
                } else {
                    if root_closed {
                        return Err(OdfError::MalformedContent);
                    }
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
                        &mut reporter,
                    )?;
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
                    &mut inline_nodes,
                    &mut text_bytes,
                    &mut reporter,
                )?;
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
                if current
                    .as_ref()
                    .is_some_and(|paragraph: &ParagraphDraft| paragraph.depth == depth)
                {
                    let paragraph = current.take().ok_or(OdfError::MalformedContent)?;
                    push_paragraph(&mut paragraphs, paragraph, limits)?;
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
                if let Some(paragraph) = &mut current {
                    append_text(paragraph, &mut active_link, &value, &mut text_bytes, limits)?;
                } else if !value.trim().is_empty() {
                    return Err(OdfError::MalformedContent);
                }
            }
            Event::CData(text) => {
                if leaf_depth.is_some() {
                    return Err(OdfError::MalformedContent);
                }
                let value = text.decode().map_err(|_| OdfError::MalformedContent)?;
                if let Some(paragraph) = &mut current {
                    append_text(paragraph, &mut active_link, &value, &mut text_bytes, limits)?;
                } else if !value.trim().is_empty() {
                    return Err(OdfError::MalformedContent);
                }
            }
            Event::GeneralRef(reference) => {
                if leaf_depth.is_some() {
                    return Err(OdfError::MalformedContent);
                }
                let value = decode_reference(&reference)?;
                if let Some(paragraph) = &mut current {
                    append_text(paragraph, &mut active_link, &value, &mut text_bytes, limits)?;
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
        || leaf_depth.is_some()
        || current.is_some()
        || active_link.is_some()
    {
        return Err(OdfError::MalformedContent);
    }
    if !body_kind_seen {
        return Err(OdfError::UnsupportedDocumentKind);
    }
    if paragraphs.is_empty() {
        push_paragraph(
            &mut paragraphs,
            ParagraphDraft {
                depth: 0,
                outline_level: None,
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
    let normalized_inline_nodes = paragraphs.iter().try_fold(0_usize, |total, paragraph| {
        count_inline_drafts(&paragraph.inlines, total)
    })?;
    enforce(
        "odf_content_inline_nodes",
        normalized_inline_nodes,
        limits.max_inline_nodes,
    )?;
    let document = build_document(expected_version, &paragraphs, &bookmarks)?;
    Ok(OdtImport {
        document,
        report: reporter.finish(),
    })
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
        start_paragraph(
            reader,
            element,
            depth,
            false,
            limits,
            attributes,
            attribute_bytes,
            current,
            reporter,
        )?;
    } else if text_body_depth.is_some() && is_name(name, NamespaceKind::Text, b"h") {
        start_paragraph(
            reader,
            element,
            depth,
            true,
            limits,
            attributes,
            attribute_bytes,
            current,
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
    inline_nodes: &mut usize,
    text_bytes: &mut usize,
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
        start_paragraph(
            reader,
            element,
            depth,
            is_name(name, NamespaceKind::Text, b"h"),
            limits,
            attributes,
            attribute_bytes,
            current,
            reporter,
        )?;
        push_paragraph(
            paragraphs,
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
            reporter,
        )?;
        if is_name(name, NamespaceKind::Text, b"a") {
            finish_link(current, active_link, reporter)?;
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
    reporter: &mut Reporter,
) -> Result<(), OdfError> {
    if current.is_some() {
        return Err(OdfError::MalformedContent);
    }
    let mut outline_level = None;
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
        count_and_report_attributes(
            reader,
            element,
            attributes,
            attribute_bytes,
            limits,
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
    Ok(())
}

fn append_text(
    paragraph: &mut ParagraphDraft,
    active_link: &mut Option<LinkDraft>,
    value: &str,
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
        push_text_draft(&mut link.inlines, value);
    } else {
        paragraph.push_text(value);
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
                InlineDraft::Text(text) => paragraph.push_text(&text),
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
        if let InlineDraft::Text(text) = inline {
            push_text_draft(&mut normalized, &text);
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

fn build_document(
    version: OdfVersion,
    paragraphs: &[ParagraphDraft],
    bookmarks: &[BookmarkDraft],
) -> Result<Document, OdfError> {
    let mut namespace = 0xcbf2_9ce4_8422_2325_u64;
    hash_bytes(&mut namespace, version.as_str().as_bytes());
    for paragraph in paragraphs {
        hash_bytes(&mut namespace, b"p");
        hash_bytes(
            &mut namespace,
            &[paragraph.outline_level.unwrap_or(u8::MAX)],
        );
        for inline in &paragraph.inlines {
            hash_inline_draft(&mut namespace, inline, bookmarks);
        }
    }
    if namespace == 0 {
        namespace = 1;
    }
    let mut ids = IdGenerator::new(namespace);
    let document_id = ids.next_id().map_err(|_| OdfError::InvalidModel)?;
    let mut definitions = Definitions::default();
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
    let mut body = Vec::with_capacity(paragraphs.len());
    for draft in paragraphs {
        let paragraph_id = ids.next_id().map_err(|_| OdfError::InvalidModel)?;
        let properties = ParagraphProperties {
            outline_level: draft.outline_level,
            ..ParagraphProperties::default()
        };
        let inlines = build_inlines(&draft.inlines, &mut ids, &bookmark_ids)?;
        body.push(BlockNode::Paragraph(Paragraph {
            id: paragraph_id,
            properties,
            inlines,
        }));
    }
    Document::new(document_id, body, definitions).map_err(|_| OdfError::InvalidModel)
}

fn hash_inline_draft(hash: &mut u64, inline: &InlineDraft, bookmarks: &[BookmarkDraft]) {
    match inline {
        InlineDraft::Text(value) => {
            hash_bytes(hash, b"t");
            hash_bytes(hash, value.as_bytes());
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
    }
}

fn build_inlines(
    drafts: &[InlineDraft],
    ids: &mut IdGenerator,
    bookmark_ids: &[Option<BookmarkId>],
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
            InlineDraft::Text(text) => InlineNode::Run(Run {
                id,
                properties: RunProperties::default(),
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
                inlines: build_inlines(children, ids, bookmark_ids)?,
            }),
            InlineDraft::BookmarkStart(_) => InlineNode::BookmarkStart(BookmarkStart {
                id,
                bookmark: bookmark.ok_or(OdfError::InvalidModel)?,
            }),
            InlineDraft::BookmarkEnd(_) => InlineNode::BookmarkEnd(BookmarkEnd {
                id,
                bookmark: bookmark.ok_or(OdfError::InvalidModel)?,
            }),
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
