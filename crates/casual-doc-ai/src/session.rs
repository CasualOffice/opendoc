// SPDX-License-Identifier: Apache-2.0

//! One open document: its state, its single commit choke point, and the
//! conversions a session offers (export with the loss gate, render).

use std::collections::BTreeMap;

use casual_doc_diff::content_digest_hex;
use casual_doc_edit::Operation;
use casual_doc_edit::access::refuse_if_not_permitted;
use casual_doc_edit::protection::refuse_if_protected;
use casual_doc_io::{
    CompatibilityReport, DetectionRequest, DocumentResources, ExportMode, ExportRequest, FormatId,
    FormatSelection, ModelOutcome, PlainTextLimits, RecoveryReport, RetentionOutcome,
    SourceEnvelope, builtin_registry_with_limits, formats,
};
use casual_doc_model::v1::Document;
use casual_doc_model::{IdGenerator, IdSpace};
use casual_doc_ooxml::PackageLimits;
use casual_doc_review::body::RevisionIdAllocator;
use casual_doc_transaction::{
    Coalesce, GroupId, Origin, RevisionLog, Transaction, TransactionError, TransactionId,
};
use opendoc_viewer::{ViewerDocument, ViewerOptions};
use serde::Serialize;
use serde_json::json;

use crate::error::{AiError, AiResult};
use crate::limits::Limits;
use crate::profile::Profile;

/// Undo groups a headless session retains.
const MAX_UNDO_GROUPS: usize = 1_000;

/// Where suggestion dates come from.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Clock {
    /// The system clock, as an ISO 8601 UTC timestamp to the second.
    #[default]
    System,
    /// A fixed timestamp (tests, reproducible batch runs).
    Fixed(String),
}

impl Clock {
    fn now(&self) -> String {
        match self {
            Self::Fixed(value) => value.clone(),
            Self::System => {
                let seconds = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |elapsed| elapsed.as_secs());
                iso8601(seconds)
            }
        }
    }
}

/// `seconds` since the epoch as `YYYY-MM-DDTHH:MM:SSZ` (Howard Hinnant's
/// civil-from-days). O(1).
fn iso8601(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rem = seconds % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

/// How a session is opened. Set by the operator; nothing in a document or a
/// tool argument can change it (`docs/178` §5.8).
#[derive(Clone, Debug)]
pub struct SessionConfig {
    /// The identity every suggestion and comment is authored by
    /// (`w:author`). Never taken from an edit plan.
    pub author: String,
    /// What the session may do.
    pub profile: Profile,
    /// Where suggestion dates come from.
    pub clock: Clock,
    /// Resource bounds.
    pub limits: Limits,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            author: crate::AI_AUTHOR.to_owned(),
            profile: Profile::Read,
            clock: Clock::System,
            limits: Limits::default(),
        }
    }
}

/// The mutable editing state, cloned whole for a dry run (copy-on-write
/// preview) and swapped in when a plan commits, so a refused plan leaves the
/// session exactly as it was.
#[derive(Clone, Debug)]
pub(crate) struct State {
    pub(crate) document: Document,
    pub(crate) log: RevisionLog,
    pub(crate) ids: IdGenerator,
    pub(crate) revisions: RevisionIdAllocator,
    next_transaction: u128,
    /// Picture bytes a plan wrote (`insert_image`), by package part name. The
    /// model holds the reference through a `SetMediaReference` in the same
    /// commit; the bytes join the session's resources only when the plan
    /// commits ([`Session::adopt`]), so a preview or a refused plan writes
    /// none. Empty between plans.
    pub(crate) media: Vec<(String, Vec<u8>)>,
}

impl State {
    /// The session's one commit choke point (I1). Every lowered operation is
    /// judged by the ENGINE against the profile's capabilities
    /// (`refuse_if_not_permitted`) and the document's own protection
    /// (`refuse_if_protected`) before the transaction log applies it.
    pub(crate) fn commit(
        &mut self,
        operations: Vec<Operation>,
        capabilities: casual_doc_edit::access::Capabilities,
        coalesce: Coalesce,
        origin: Origin,
    ) -> AiResult<GroupId> {
        if operations.is_empty() {
            return Err(AiError::new("no-change", "The edit changes nothing."));
        }
        refuse_if_not_permitted(Some(&self.document), &operations, capabilities).map_err(
            |refusal| {
                AiError::new("capability-withheld", refusal.reason())
                    .with_details(json!({ "enforcedBy": "engine" }))
            },
        )?;
        refuse_if_protected(&self.document, &operations, capabilities).map_err(|refusal| {
            AiError::new("document-protected", refusal.reason())
                .with_details(json!({ "enforcedBy": "engine" }))
        })?;
        self.next_transaction = self.next_transaction.saturating_add(1);
        let transaction = Transaction::reserve(
            TransactionId::new(self.next_transaction),
            self.log.head(),
            "AI edit",
            &mut self.ids,
            operations,
        )
        .ok_or_else(|| AiError::new("over-limit", "The document's id space is exhausted."))?
        .coalescing(coalesce)
        .with_origin(origin);
        let commit =
            self.log
                .apply(&mut self.document, transaction)
                .map_err(|error| match error {
                    TransactionError::Edit(error) => AiError::engine(
                        error
                            .reason()
                            .map_or_else(|| format!("{error:?}"), str::to_owned),
                    ),
                    other => AiError::engine(other.to_string()),
                })?;
        Ok(commit.group())
    }
}

/// One committed plan, by the id `apply` returned.
#[derive(Clone, Debug)]
pub(crate) struct ChangeRecord {
    pub(crate) id: String,
    pub(crate) group: GroupId,
    pub(crate) idempotency_key: Option<String>,
    pub(crate) revision_after: u64,
}

/// What the import kept, degraded or dropped.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSummary {
    /// Findings the model could not fully carry, with their outcome.
    pub findings: Vec<Finding>,
    /// Repairs a best-effort open applied to damaged bytes.
    pub repairs: Vec<String>,
}

/// One compatibility finding, projected.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable feature code (the loss code an export may name in `accept_loss`).
    pub feature: String,
    /// How many times it occurs.
    pub occurrences: u32,
    /// `mapped`, `degraded` or `omitted`.
    pub model: &'static str,
    /// `preserved`, `notRetained`, `blocked`, `rejected` or `notApplicable`.
    pub retention: &'static str,
    /// Whether this finding is content the target would not carry.
    pub loss: bool,
}

fn finding(entry: &casual_doc_io::CompatibilityEntry) -> Finding {
    let model = match entry.model_outcome {
        ModelOutcome::Mapped => "mapped",
        ModelOutcome::Degraded => "degraded",
        ModelOutcome::Omitted => "omitted",
    };
    let retention = match entry.retention_outcome {
        RetentionOutcome::Preserved => "preserved",
        RetentionOutcome::NotRetained => "notRetained",
        RetentionOutcome::Blocked => "blocked",
        RetentionOutcome::Rejected => "rejected",
        RetentionOutcome::NotApplicable => "notApplicable",
    };
    // A loss is meaning the model did not carry that is not kept aside either.
    let loss = entry.model_outcome != ModelOutcome::Mapped
        && entry.retention_outcome != RetentionOutcome::Preserved;
    Finding {
        feature: entry.feature.clone(),
        occurrences: entry.occurrences,
        model,
        retention,
        loss,
    }
}

fn summarise(report: &CompatibilityReport, recovery: Option<&RecoveryReport>) -> ReportSummary {
    ReportSummary {
        findings: report
            .entries
            .iter()
            .filter(|entry| entry.model_outcome != ModelOutcome::Mapped)
            .map(finding)
            .collect(),
        repairs: recovery
            .map(|recovery| {
                recovery
                    .repairs
                    .iter()
                    .map(|repair| repair.summary.clone())
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// An export target an agent can name.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    /// Word document.
    Docx,
    /// OpenDocument text.
    Odt,
    /// Real-text PDF from the editor's layout.
    Pdf,
    /// Single-file HTML.
    Html,
    /// CommonMark.
    Markdown,
    /// UTF-8 plain text.
    Txt,
    /// The normalized JSON snapshot.
    Json,
}

impl ExportFormat {
    /// Parses a format name or extension.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "docx" => Self::Docx,
            "odt" => Self::Odt,
            "pdf" => Self::Pdf,
            "html" | "htm" => Self::Html,
            "markdown" | "md" => Self::Markdown,
            "txt" | "text" => Self::Txt,
            "json" => Self::Json,
            _ => return None,
        })
    }

    fn id(self) -> &'static str {
        match self {
            Self::Docx => formats::DOCX,
            Self::Odt => formats::ODT,
            Self::Pdf => formats::PDF,
            Self::Html => formats::HTML,
            Self::Markdown => formats::MARKDOWN,
            Self::Txt => formats::TEXT,
            Self::Json => formats::NORMALIZED_JSON,
        }
    }

    /// Whether the format is lossy by its nature (it cannot carry a word
    /// processing document's structure), in which case an export states that
    /// rather than refusing (`docs/178` §5.5).
    #[must_use]
    pub const fn lossy_by_nature(self) -> bool {
        matches!(self, Self::Pdf | Self::Html | Self::Markdown | Self::Txt)
    }

    /// The file extension.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Odt => "odt",
            Self::Pdf => "pdf",
            Self::Html => "html",
            Self::Markdown => "md",
            Self::Txt => "txt",
            Self::Json => "json",
        }
    }
}

/// What an export would keep and lose.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityView {
    /// The target format.
    pub target: ExportFormat,
    /// Whether the target cannot carry the document's structure at all.
    pub lossy_by_nature: bool,
    /// Every loss a save would make, from the import and from the export.
    pub losses: Vec<Finding>,
    /// Everything else the writer reported (degraded but retained).
    pub other_findings: Vec<Finding>,
}

/// A finished export.
#[derive(Clone, Debug)]
pub struct Exported {
    /// The bytes.
    pub bytes: Vec<u8>,
    /// Its MIME type.
    pub mime_type: String,
    /// What the export kept and lost.
    pub report: CompatibilityView,
}

/// One open document.
#[derive(Debug)]
pub struct Session {
    pub(crate) state: State,
    pub(crate) config: SessionConfig,
    resources: DocumentResources,
    source: Option<SourceEnvelope>,
    source_format: FormatId,
    import_report: CompatibilityReport,
    recovery: RecoveryReport,
    opened_at: u64,
    pub(crate) changes: Vec<ChangeRecord>,
    layout: Option<(u64, ViewerDocument)>,
    name: String,
    /// What [`Session::revision`] adds to the log's head: zero for a session
    /// that opened a file, the live editor's document revision for one built by
    /// [`Session::for_editor`], so the revisions an agent reads and plans
    /// against are the editor's own.
    revision_base: u64,
}

/// The registry a session opens and saves through: the built-in one, plus PDF
/// import when this build compiled `pdf-import` (`docs/182` P1; `opendoc-mcp`
/// enables it, the editor's WASM build does not). Built per call: O(formats).
fn session_registry() -> casual_doc_io::FormatRegistry {
    #[allow(unused_mut)] // mutated only with `pdf-import`
    let mut registry =
        builtin_registry_with_limits(PackageLimits::default(), PlainTextLimits::default());
    #[cfg(feature = "pdf-import")]
    casual_doc_io::enable_pdf_import(&mut registry, casual_doc_io::PdfReadLimits::default())
        .expect("re-registering PDF into a cleared slot cannot collide");
    registry
}

/// [`session_registry`] whose PDF import reads scans with the host's OCR
/// and opens a PDF in `layout`.
#[cfg(feature = "pdf-import")]
fn session_registry_with_pdf(
    ocr: Option<&casual_doc_io::PdfOcrOptions>,
    layout: casual_doc_io::PdfLayout,
) -> casual_doc_io::FormatRegistry {
    let mut registry =
        builtin_registry_with_limits(PackageLimits::default(), PlainTextLimits::default());
    casual_doc_io::enable_pdf_import_with_layout(
        &mut registry,
        casual_doc_io::PdfReadLimits::default(),
        ocr.cloned().unwrap_or_default(),
        layout,
    )
    .expect("re-registering PDF into a cleared slot cannot collide");
    registry
}

impl Session {
    /// Opens untrusted bytes. `name` is the file name, used only as a format
    /// hint and a label. **O(document)**: one import walk.
    ///
    /// # Errors
    ///
    /// `over-limit` past the configured size; `open-failed` when no importer
    /// admits the bytes (a hostile package is refused by the importer's own
    /// limits here).
    pub fn open(bytes: &[u8], name: &str, config: SessionConfig) -> AiResult<Self> {
        Self::open_through(&session_registry(), bytes, name, config)
    }

    /// [`Session::open`] with the host's OCR for scanned PDF pages
    /// (`docs/182` §20a): a scan opens with its text instead of
    /// `pdf.import.ocr_unavailable`. Other formats open exactly as with
    /// [`Session::open`]. **O(document)** plus the provider's work, bounded
    /// by `ocr`'s budgets.
    ///
    /// # Errors
    ///
    /// As [`Session::open`].
    #[cfg(feature = "pdf-import")]
    pub fn open_with_pdf_ocr(
        bytes: &[u8],
        name: &str,
        config: SessionConfig,
        ocr: &casual_doc_io::PdfOcrOptions,
    ) -> AiResult<Self> {
        Self::open_pdf(
            bytes,
            name,
            config,
            Some(ocr),
            casual_doc_io::PdfLayout::default(),
        )
    }

    /// [`Session::open`] with the host's PDF choices (`docs/182` §4): OCR for
    /// scanned pages, and the layout. A PDF opens as a flowing document by
    /// default ([`casual_doc_io::PdfLayout::Flowing`]: paragraphs with styles,
    /// lists, tables, header and footer, footnotes, columns, no text boxes);
    /// [`casual_doc_io::PdfLayout::Fixed`] is "Keep exact page layout", the
    /// pages as drawn in positioned frames, the only layout whose save goes
    /// back into the original PDF. Other formats open exactly as with
    /// [`Session::open`]. **O(document)** plus OCR's bounded work.
    ///
    /// # Errors
    ///
    /// As [`Session::open`].
    #[cfg(feature = "pdf-import")]
    pub fn open_pdf(
        bytes: &[u8],
        name: &str,
        config: SessionConfig,
        ocr: Option<&casual_doc_io::PdfOcrOptions>,
        layout: casual_doc_io::PdfLayout,
    ) -> AiResult<Self> {
        Self::open_through(&session_registry_with_pdf(ocr, layout), bytes, name, config)
    }

    /// How this session's PDF was opened; `None` when it is not a PDF's.
    /// O(1).
    #[cfg(feature = "pdf-import")]
    #[must_use]
    pub fn pdf_layout(&self) -> Option<casual_doc_io::PdfLayout> {
        self.source.as_ref().and_then(casual_doc_io::pdf_layout)
    }

    /// What OCR did when this session's PDF was imported, and the manifest of
    /// the provider that read it (SHA-256 of every model): `None` for a
    /// session that did not open a PDF. O(1).
    #[cfg(feature = "pdf-import")]
    #[must_use]
    pub fn pdf_ocr_summary(&self) -> Option<&casual_doc_io::PdfOcrSummary> {
        self.source
            .as_ref()
            .and_then(casual_doc_io::pdf_ocr_summary)
    }

    fn open_through(
        registry: &casual_doc_io::FormatRegistry,
        bytes: &[u8],
        name: &str,
        config: SessionConfig,
    ) -> AiResult<Self> {
        if bytes.len() > config.limits.max_file_bytes {
            return Err(AiError::new(
                "over-limit",
                format!(
                    "The file is larger than this server opens ({} bytes).",
                    config.limits.max_file_bytes
                ),
            ));
        }
        let imported = registry
            .import(
                DetectionRequest {
                    bytes,
                    selection: FormatSelection::Auto,
                    file_name_hint: Some(name),
                    mime_hint: None,
                },
                true,
            )
            .map_err(|error| AiError::new("open-failed", error.to_string()))?;
        Ok(Self::from_artifact(imported, name, config))
    }

    /// "Convert to flowing document" (`docs/182` §18): a **new** session over
    /// the reflow of the PDF this session opened, with paragraphs, headings,
    /// lists, tables and columns inferred from its pages. This session is not
    /// touched. The conversion reads the PDF as it was imported (its source
    /// envelope), not this session's edits: a change of document shape keeps
    /// no edit history, which is Word's PDF Reflow model too. The new
    /// session's name is this one's with ` (flowing)` added, and its import
    /// report is the reflow's. **O(document log document)**.
    ///
    /// # Errors
    ///
    /// `not-a-pdf` when this session did not open a PDF (or is itself a
    /// conversion); `already-flowing` when the PDF opened as a flowing
    /// document (the default); `convert-failed` when the reflowed document
    /// fails validation (an importer defect).
    #[cfg(feature = "pdf-import")]
    pub fn convert_pdf_to_flowing(&self) -> AiResult<Self> {
        if self.source.as_ref().and_then(casual_doc_io::pdf_layout)
            == Some(casual_doc_io::PdfLayout::Flowing)
            && self
                .source
                .as_ref()
                .and_then(casual_doc_io::pdf_retained_original)
                .is_some()
        {
            return Err(AiError::new(
                "already-flowing",
                "This PDF already opened as a flowing document. Open it with the fixed layout to convert its pages.",
            ));
        }
        let reflowed = self
            .source
            .as_ref()
            .and_then(|source| casual_doc_io::pdf_reflow(source, &self.resources))
            .ok_or_else(|| {
                AiError::new(
                    "not-a-pdf",
                    "Only a document opened from a PDF converts to a flowing document.",
                )
            })?
            .map_err(|error| AiError::new("convert-failed", error.to_string()))?;
        Ok(Self::from_artifact(
            reflowed,
            &format!("{} (flowing)", self.name),
            self.config.clone(),
        ))
    }

    /// A session over an import artifact. O(document) for the id scan.
    fn from_artifact(
        imported: casual_doc_io::ImportArtifact,
        name: &str,
        config: SessionConfig,
    ) -> Self {
        let document = imported.document;
        // Edits mint in the OFFLINE space of this document's partition, seeded past
        // whatever that space already holds: the editor's rule, so a document an
        // agent edited can still be opened and edited in the editor without an id
        // collision.
        let mut ids = IdGenerator::new(IdSpace::local(IdSpace::of_document(document.id())).get());
        ids.reserve_through(document.highest_counter_in(IdSpace::new(ids.namespace())));
        let revisions = RevisionIdAllocator::from_document(&document);
        let log = RevisionLog::new(MAX_UNDO_GROUPS);
        let opened_at = log.head().get();
        Self {
            state: State {
                document,
                log,
                ids,
                revisions,
                next_transaction: 0,
                media: Vec::new(),
            },
            config,
            resources: imported.resources,
            source_format: imported.source.format().clone(),
            source: Some(imported.source),
            import_report: imported.report,
            recovery: imported.recovery,
            opened_at,
            changes: Vec::new(),
            layout: None,
            name: name.to_owned(),
            revision_base: 0,
        }
    }

    /// A session over the live editor's document (`docs/181` §4.4): the
    /// in-page assistant reads and compiles plans against exactly what the
    /// editor holds, and nothing it does reaches the editor until the editor
    /// commits the compiled operations through its own choke point.
    ///
    /// `ids` and `revisions` are clones of the EDITOR's allocators, so every id
    /// a plan mints is one the editor would have minted next, and the editor
    /// moves its allocators past them when it commits ([`crate::editor`]). A
    /// fresh generator here would mint in the editor's own space from the
    /// document's high-water mark and could hand out an id the editor had
    /// already used for content an undo removed. `revision` is the editor's
    /// document revision, which [`Session::revision`] then reports.
    ///
    /// **O(1)** beyond the moves; the caller already paid for the clone.
    #[must_use]
    pub fn for_editor(
        document: Document,
        ids: IdGenerator,
        revisions: RevisionIdAllocator,
        revision: u64,
        config: SessionConfig,
    ) -> Self {
        let log = RevisionLog::new(MAX_UNDO_GROUPS);
        let opened_at = log.head().get();
        Self {
            state: State {
                document,
                log,
                ids,
                revisions,
                next_transaction: 0,
                media: Vec::new(),
            },
            config,
            resources: DocumentResources::default(),
            source: None,
            source_format: FormatId::new(formats::NORMALIZED_JSON)
                .expect("a registered built-in format id"),
            import_report: CompatibilityReport::default(),
            recovery: RecoveryReport::default(),
            opened_at,
            changes: Vec::new(),
            layout: None,
            name: String::new(),
            revision_base: revision,
        }
    }

    /// The live document.
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.state.document
    }

    /// Makes `state` (a plan's committed clone) the session's, moving the
    /// picture bytes it wrote into the resources the model now references.
    /// O(pictures written).
    pub(crate) fn adopt(&mut self, mut state: State) {
        for (part, bytes) in std::mem::take(&mut state.media) {
            self.resources.insert(part, bytes);
        }
        self.state = state;
    }

    /// The bytes of the package part `part`, if the session holds it.
    #[must_use]
    pub fn resource(&self, part: &str) -> Option<&[u8]> {
        self.resources.get(part)
    }

    /// The session's configuration.
    #[must_use]
    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    /// The file name it was opened under.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The current revision: every committed plan advances it. An edit plan
    /// names the revision it was written against (`base_revision`).
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision_base
            .saturating_add(self.state.log.head().get())
    }

    /// The content digest of the current state. **O(document)**.
    #[must_use]
    pub fn digest(&self) -> String {
        content_digest_hex(&self.state.document)
    }

    /// The source format's stable id.
    #[must_use]
    pub fn source_format(&self) -> &str {
        self.source_format.as_str()
    }

    /// What the import could not carry, and what it repaired.
    #[must_use]
    pub fn import_summary(&self) -> ReportSummary {
        summarise(&self.import_report, Some(&self.recovery))
    }

    pub(crate) fn date(&self) -> String {
        self.config.clock.now()
    }

    /// The initials comments carry, from the configured author.
    pub(crate) fn initials(&self) -> String {
        self.config
            .author
            .split_whitespace()
            .take(2)
            .filter_map(|part| part.chars().next())
            .flat_map(char::to_uppercase)
            .collect()
    }

    /// The paginated layout of the current revision, built once per revision.
    /// **O(document)** on the first call after a change.
    fn layout(&mut self) -> &ViewerDocument {
        let revision = self.revision();
        if self.layout.as_ref().is_none_or(|(at, _)| *at != revision) {
            let viewer = ViewerDocument::from_document(
                self.state.document.clone(),
                self.resources.as_map().clone(),
                &ViewerOptions::default(),
            );
            self.layout = Some((revision, viewer));
        }
        &self.layout.as_ref().expect("built above").1
    }

    /// The number of pages at the current revision. **O(document)** after a change.
    pub fn page_count(&mut self) -> usize {
        self.layout().page_count()
    }

    /// Page `number` (1-based) as PNG at `dpi`, held to the DPI and pixel
    /// ceilings. **O(page)** once laid out.
    ///
    /// # Errors
    ///
    /// `invalid-argument` for a page past the end; `over-limit` past the pixel
    /// ceiling.
    pub fn render_page(&mut self, number: usize, dpi: Option<f32>) -> AiResult<(Vec<u8>, f32)> {
        let limits = self.config.limits.clone();
        let dpi = dpi
            .unwrap_or(limits.default_dpi)
            .clamp(24.0, limits.max_dpi);
        let layout = self.layout();
        let pages = layout.page_count();
        if number == 0 || number > pages {
            return Err(AiError::invalid(format!(
                "There is no page {number}; the document has {pages}."
            )));
        }
        let (width, height) = layout.page_size_twips(number - 1).unwrap_or((0, 0));
        let pixels = (f64::from(width) * f64::from(dpi) / 1440.0).ceil()
            * (f64::from(height) * f64::from(dpi) / 1440.0).ceil();
        if pixels > limits.max_page_pixels as f64 {
            return Err(AiError::new(
                "over-limit",
                "That page is too large to render at this DPI; ask for a lower one.",
            ));
        }
        let png = layout
            .render_page_png(number - 1, dpi)
            .map_err(|error| AiError::new("render-failed", error.to_string()))?;
        Ok((png, dpi))
    }

    /// What a save to `target` would keep and lose, without writing anything.
    /// **O(document)**: it runs the writer.
    ///
    /// # Errors
    ///
    /// `export-failed` when the writer refuses the document.
    pub fn compatibility(&self, target: ExportFormat) -> AiResult<CompatibilityView> {
        Ok(self.export_inner(target)?.report)
    }

    fn export_inner(&self, target: ExportFormat) -> AiResult<Exported> {
        let registry = session_registry();
        let format = FormatId::new(target.id())
            .map_err(|_| AiError::invalid("That export format is not registered."))?;
        let same_format = format == self.source_format;
        let artifact = registry
            .export(
                &format,
                ExportRequest {
                    document: &self.state.document,
                    resources: &self.resources,
                    source: if same_format {
                        self.source.as_ref()
                    } else {
                        None
                    },
                    source_unchanged: self.state.log.head().get() == self.opened_at,
                    mode: if same_format {
                        ExportMode::PreserveWhenSafe
                    } else {
                        ExportMode::Semantic
                    },
                },
            )
            .map_err(|error| AiError::new("export-failed", error.to_string()))?;
        // Losses are the import's (meaning already dropped at open, which a save
        // makes permanent) and the writer's, merged by feature.
        let mut losses: BTreeMap<String, Finding> = BTreeMap::new();
        let mut other = Vec::new();
        for entry in self
            .import_report
            .entries
            .iter()
            .chain(&artifact.report.entries)
        {
            if entry.model_outcome == ModelOutcome::Mapped {
                continue;
            }
            let found = finding(entry);
            if found.loss {
                losses
                    .entry(found.feature.clone())
                    .and_modify(|existing| {
                        existing.occurrences = existing.occurrences.max(found.occurrences);
                    })
                    .or_insert(found);
            } else {
                other.push(found);
            }
        }
        Ok(Exported {
            bytes: artifact.bytes,
            mime_type: artifact.mime_type,
            report: CompatibilityView {
                target,
                lossy_by_nature: target.lossy_by_nature(),
                losses: losses.into_values().collect(),
                other_findings: other,
            },
        })
    }

    /// Exports the current state. A format that can carry the document
    /// (DOCX, ODT, JSON) is **refused** when the save would lose content not
    /// named in `accept_loss`; a blanket "ignore" does not exist. A format that
    /// is lossy by nature states so in its report instead.
    ///
    /// # Errors
    ///
    /// `loss-refused` naming each unaccepted loss; `export-failed`.
    pub fn export(&self, target: ExportFormat, accept_loss: &[String]) -> AiResult<Exported> {
        let exported = self.export_inner(target)?;
        if !target.lossy_by_nature() {
            let unaccepted: Vec<&Finding> = exported
                .report
                .losses
                .iter()
                .filter(|loss| !accept_loss.contains(&loss.feature))
                .collect();
            if !unaccepted.is_empty() {
                return Err(AiError::new(
                    "loss-refused",
                    "This save would lose content. Name each loss you accept in accept_loss, or \
                     export to another format.",
                )
                .with_details(json!({ "losses": unaccepted })));
            }
        }
        Ok(exported)
    }

    /// Undoes the change `change_id` if, and only if, it is the latest one
    /// (`docs/178` §5.4).
    ///
    /// # Errors
    ///
    /// `not-found`; `undo-not-latest` naming the later changes; an engine
    /// refusal.
    pub fn undo_change(&mut self, change_id: &str) -> AiResult<u64> {
        let Some(position) = self
            .changes
            .iter()
            .position(|change| change.id == change_id)
        else {
            return Err(AiError::new(
                "not-found",
                "No change has that id in this session.",
            ));
        };
        let record = self.changes[position].clone();
        if self.state.log.undo_target() != Some(record.group) || position + 1 != self.changes.len()
        {
            let later: Vec<&str> = self.changes[position + 1..]
                .iter()
                .map(|change| change.id.as_str())
                .collect();
            return Err(AiError::new(
                "undo-not-latest",
                "Only the latest change can be undone; undo the later ones first.",
            )
            .with_details(json!({ "laterChanges": later })));
        }
        let inverse = self.state.log.inverse_of(record.group);
        // The inverse is the log's exact record of a group this session committed
        // and the engine admitted, and it is the latest group, so applying it can
        // only return the document to a state the session was already in. It is
        // therefore not re-judged as a new authoring act: withdrawing one's own
        // suggestion is removing a revision, which the suggest class (rightly)
        // does not admit as a fresh operation.
        let capabilities = casual_doc_edit::access::Capabilities::local();
        let mut scratch = self.state.clone();
        scratch.commit(
            inverse,
            capabilities,
            Coalesce::New,
            Origin::Undo {
                group: record.group,
            },
        )?;
        self.state = scratch;
        self.changes.pop();
        Ok(self.revision())
    }
}

#[cfg(test)]
mod tests {
    use super::{Clock, Coalesce, Origin, Session, SessionConfig, iso8601};
    use crate::profile::Profile;
    use casual_doc_edit::{Operation, Pos};

    /// `docs/178` §8 M2's exit gate: a suggest-profile session is refused a
    /// direct edit **by the engine**, with the adapter's own check out of the
    /// way. This drives the choke point directly, as a buggy adapter would.
    #[test]
    fn the_engine_refuses_an_untracked_edit_from_a_suggest_session() {
        let mut session = Session::open(
            b"One paragraph.\n",
            "one.txt",
            SessionConfig {
                profile: Profile::Suggest,
                clock: Clock::Fixed("2026-10-24T00:00:00Z".to_owned()),
                ..SessionConfig::default()
            },
        )
        .expect("open");
        let node = crate::structure::body_paragraphs(session.document())[0]
            .0
            .id;
        let before = session.digest();
        let capabilities = session.config.profile.capabilities();
        let refused = session
            .state
            .commit(
                vec![Operation::InsertText {
                    at: Pos::new(node, 0),
                    text: "Untracked ".to_owned(),
                }],
                capabilities,
                Coalesce::New,
                Origin::Edit,
            )
            .expect_err("a suggester may not edit directly");
        assert_eq!(refused.code, "capability-withheld");
        assert_eq!(refused.details["enforcedBy"], "engine");
        assert_eq!(session.digest(), before);
    }

    #[test]
    fn timestamps_are_civil_utc() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(1_798_329_600), "2026-12-27T00:00:00Z");
        assert_eq!(iso8601(951_782_400 + 3_661), "2000-02-29T01:01:01Z");
    }
}
