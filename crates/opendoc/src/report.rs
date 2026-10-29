// SPDX-License-Identifier: Apache-2.0

//! The loss-aware edge (`docs/176` P7): what an open did not carry, and what a
//! save lost, as a stable, versioned value.
//!
//! The engine's `CompatibilityReport` is internal; this is its public
//! projection. `schema` is bumped only when a field changes meaning, so a host
//! that stores reports can tell which shape it is reading.

use casual_doc_io::{CompatibilityEntry, CompatibilityReport, ModelOutcome, RetentionOutcome};
use serde::Serialize;

use crate::format::{Format, SaveMode};

/// The report schema version. Additive fields do not bump it.
pub const REPORT_SCHEMA: u32 = 1;

/// How a source construct reached the model.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Representation {
    /// Fully represented.
    Mapped,
    /// Partially represented.
    Degraded,
    /// Not represented.
    Omitted,
}

/// What happened to the source detail the model did not consume.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Retention {
    /// Kept verbatim, and written back by a preserving save.
    Preserved,
    /// Deliberately not kept.
    NotRetained,
    /// Refused by a security or host policy (macros, external content).
    Blocked,
    /// Invalid or over a limit, and rejected.
    Rejected,
    /// Nothing was left over.
    NotApplicable,
}

/// One finding: a feature, how often, where, and what became of it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable feature code (`"w:sdt"`, `"chart"`, ...).
    pub feature: String,
    /// How many times it occurred (saturating).
    pub occurrences: u32,
    /// The package part, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    /// The XML element, when the finding is about one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<String>,
    /// The XML attribute, when the finding is about one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribute: Option<String>,
    /// How it reached the model.
    pub representation: Representation,
    /// What became of the remainder.
    pub retention: Retention,
    /// Whether a save loses meaning here: not mapped, and not preserved.
    pub loss: bool,
}

impl Finding {
    fn from_entry(entry: &CompatibilityEntry) -> Self {
        let representation = match entry.model_outcome {
            ModelOutcome::Mapped => Representation::Mapped,
            ModelOutcome::Degraded => Representation::Degraded,
            ModelOutcome::Omitted => Representation::Omitted,
        };
        let retention = match entry.retention_outcome {
            RetentionOutcome::Preserved => Retention::Preserved,
            RetentionOutcome::NotRetained => Retention::NotRetained,
            RetentionOutcome::Blocked => Retention::Blocked,
            RetentionOutcome::Rejected => Retention::Rejected,
            RetentionOutcome::NotApplicable => Retention::NotApplicable,
        };
        Self {
            feature: entry.feature.clone(),
            occurrences: entry.occurrences,
            part: entry.location.part_name.clone(),
            element: entry.location.local_name.clone(),
            attribute: entry.location.attribute_name.clone(),
            representation,
            retention,
            loss: representation != Representation::Mapped && retention != Retention::Preserved,
        }
    }

    pub(crate) fn all(report: &CompatibilityReport) -> Vec<Self> {
        report.entries.iter().map(Self::from_entry).collect()
    }
}

/// One repair a best-effort open applied to damaged bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repair {
    /// Stable token for the kind of repair.
    pub token: String,
    /// One sentence for a reader.
    pub summary: String,
    /// `"structural"`, `"content-dropped"` or `"body-lost"`.
    pub severity: &'static str,
    /// The part the repair is charged to, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    /// How many times it was applied.
    pub occurrences: u32,
}

/// What an open did not carry into the model, and what it had to repair.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    /// [`REPORT_SCHEMA`].
    pub schema: u32,
    /// The detected source format (`"docx"`, ...).
    pub format: String,
    /// Every finding, in the engine's deterministic order.
    pub findings: Vec<Finding>,
    /// Repairs of damaged bytes; empty for a well-formed file.
    pub repairs: Vec<Repair>,
}

impl ImportReport {
    pub(crate) fn new(
        format: Option<Format>,
        report: &CompatibilityReport,
        recovery: &casual_doc_io::RecoveryReport,
    ) -> Self {
        Self {
            schema: REPORT_SCHEMA,
            format: format.map_or("unknown", Format::name).to_owned(),
            findings: Finding::all(report),
            repairs: recovery
                .repairs
                .iter()
                .map(|repair| Repair {
                    token: repair.token.clone(),
                    summary: repair.summary.clone(),
                    severity: match repair.severity {
                        casual_doc_io::RepairSeverity::Structural => "structural",
                        casual_doc_io::RepairSeverity::ContentDropped => "content-dropped",
                        casual_doc_io::RepairSeverity::BodyLost => "body-lost",
                    },
                    part: repair.part_name.clone(),
                    occurrences: repair.occurrences,
                })
                .collect(),
        }
    }

    /// The findings a save would make permanent.
    pub fn losses(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|finding| finding.loss)
    }

    /// Whether the open represented everything and repaired nothing.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.repairs.is_empty() && self.losses().next().is_none()
    }
}

/// What a save wrote, and what it lost.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveReport {
    /// [`REPORT_SCHEMA`].
    pub schema: u32,
    /// The target format.
    pub format: String,
    /// The mode the save ran in (`"exact"`, `"preserve"`, `"semantic"`).
    pub mode: &'static str,
    /// Whether the target is lossy by nature (TXT, HTML, Markdown, PDF): the
    /// save is a rendition, not a container for the document.
    pub lossy_by_nature: bool,
    /// The writer's findings.
    pub findings: Vec<Finding>,
    /// The import's losses that this save makes permanent, plus the writer's,
    /// merged by feature. Empty means nothing the source carried was lost.
    pub losses: Vec<Finding>,
}

impl SaveReport {
    pub(crate) fn new(
        format: Format,
        mode: SaveMode,
        written: &CompatibilityReport,
        import: &[Finding],
    ) -> Self {
        let findings = Finding::all(written);
        let mut losses: Vec<Finding> = Vec::new();
        for finding in import.iter().chain(&findings).filter(|f| f.loss) {
            if let Some(existing) = losses.iter_mut().find(|l| l.feature == finding.feature) {
                existing.occurrences = existing.occurrences.max(finding.occurrences);
            } else {
                losses.push(finding.clone());
            }
        }
        losses.sort_by(|a, b| a.feature.cmp(&b.feature));
        Self {
            schema: REPORT_SCHEMA,
            format: format.name().to_owned(),
            mode: mode.name(),
            lossy_by_nature: !format.carries_document(),
            findings,
            losses,
        }
    }

    /// Whether the save lost nothing the source carried.
    #[must_use]
    pub fn is_lossless(&self) -> bool {
        self.losses.is_empty()
    }
}
