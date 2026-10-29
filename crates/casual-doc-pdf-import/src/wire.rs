//! A finished import as the bytes the editor's PDF module hands its main module
//! (`casual-doc-pdf-wire`, `docs/182` §17a), and the pieces of it a native
//! host that keeps the document in process uses.

use casual_doc_pdf_wire as wire;

use crate::ocr::{OcrSummary, PageClass};
use crate::{PdfImport, PdfImportReport, PdfModelOutcome, SourceMap};

/// The findings, in report order. O(findings).
#[must_use]
pub fn findings(report: &PdfImportReport) -> Vec<wire::Finding> {
    report
        .findings
        .iter()
        .map(|finding| wire::Finding {
            feature: finding.feature.to_owned(),
            occurrences: finding.occurrences,
            first_page: finding
                .first_page
                .map(|page| u32::try_from(page).unwrap_or(u32::MAX)),
            outcome: match finding.outcome {
                PdfModelOutcome::Mapped => wire::Outcome::Mapped,
                PdfModelOutcome::Degraded => wire::Outcome::Degraded,
                PdfModelOutcome::Omitted => wire::Outcome::Omitted,
            },
        })
        .collect()
}

/// The source map. O(pages + frames).
#[must_use]
pub fn sources(sources: &SourceMap) -> wire::SourceMap {
    wire::SourceMap {
        pages: sources
            .pages
            .iter()
            .map(|page| wire::PageSource {
                object: page.object,
                shows: page.shows,
            })
            .collect(),
        frames: sources
            .frames
            .iter()
            .map(|frame| wire::FrameSource {
                page: u32::try_from(frame.page).unwrap_or(u32::MAX),
                frame: frame.frame.as_u128(),
                shows: frame.shows.clone(),
                complete: frame.complete,
            })
            .collect(),
    }
}

/// What OCR did, reduced to what a summary shows. O(pages).
#[must_use]
pub fn ocr(summary: &OcrSummary) -> wire::OcrSummary {
    let count = |class| {
        let n = summary
            .classes
            .iter()
            .filter(|c| **c == Some(class))
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    };
    wire::OcrSummary {
        raster_pages: count(PageClass::Raster),
        hybrid_pages: count(PageClass::Hybrid),
        regions_recognized: summary.regions_recognized,
        regions_skipped: summary.regions_skipped,
        words_kept: summary.words_kept,
        provider: summary.provider.as_ref().map(|p| p.provider.to_owned()),
    }
}

/// The whole import as a package. O(document): the snapshot serialisation.
///
/// # Errors
///
/// The model's snapshot error, as text.
pub fn package(import: &PdfImport) -> Result<wire::ImportPackage, String> {
    Ok(wire::ImportPackage {
        document_json: import
            .document
            .to_json()
            .map_err(|error| error.to_string())?,
        resources: import
            .resources
            .iter()
            .map(|(name, bytes)| (name.clone(), bytes.clone()))
            .collect(),
        findings: findings(&import.report),
        sources: sources(&import.sources),
        ocr: ocr(&import.ocr),
        original_len: import.original.len() as u64,
        original_hash: wire::fnv1a64(&import.original),
        // The main module computes its own baseline today; the planner move
        // (`docs/182` §17a, not built) fills this.
        baseline: Vec::new(),
    })
}
