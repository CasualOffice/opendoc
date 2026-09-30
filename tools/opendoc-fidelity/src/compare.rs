// SPDX-License-Identifier: Apache-2.0

//! The geometry comparison: one document, two renderers, **differences as
//! measurements**.
//!
//! Page count, per-page text extents, per-line baselines, line counts, words per
//! line and the faces each side resolved — each reported with a number and a
//! location, never as an impression. The reduction is
//! [`casual_doc_layout::text_region`] on our side and [`crate::oracle`] on the
//! reference side; both produce the quantity `pdftotext -bbox` reports per word,
//! so no fudge factor is involved.
//!
//! # What this cannot do: ONLYOFFICE
//!
//! The obvious third column is ONLYOFFICE, and it cannot be automated the way
//! LibreOffice is. Their web client cannot open a file at all without a server:
//! format I/O is the native `x2t` binary, and `core/X2tConverter/build/` ships
//! only `Android/` and `Qt/` — there is no WASM build, so nothing in a browser
//! converts a `.docx`. Automating them therefore means standing up Document
//! Server (AGPL-3.0, Docker, a conversion API) and rendering through it, which is
//! a different kind of dependency from "run a binary over a file" and cannot live
//! in this harness's one-command shape.
//!
//! What *is* possible, and what the shape here is designed to accept: their
//! server's conversion endpoint produces a PDF from a `.docx`. That PDF reduces
//! through [`crate::oracle::reduce_page`] unchanged — the reducer takes
//! `pdftotext -bbox` output, not LibreOffice specifically. So adding ONLYOFFICE
//! is a matter of supplying the PDF, not of rewriting the comparison. Until
//! somebody stands that server up, this harness has one reference and says so
//! rather than implying three.

use std::error::Error;
use std::path::Path;

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text_region::{PageTextRegion, page_text_region};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

use crate::oracle::{OracleDocument, OraclePage};

/// Our side of the comparison for a whole document.
#[derive(Clone, Debug)]
pub struct OurDocument {
    /// One reduced region per page.
    pub pages: Vec<PageTextRegion>,
}

/// Imports and paginates `docx`, then reduces every page.
///
/// # Errors
/// If the package cannot be opened or imported.
pub fn our_document(docx: &[u8]) -> Result<OurDocument, Box<dyn Error>> {
    let mut package = DocxPackage::open(docx, PackageLimits::default())?;
    let document = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )?
    .document;
    let layout = paginate_document(&document, &ParleyShaper::new());
    Ok(OurDocument {
        pages: layout.pages.iter().map(page_text_region).collect(),
    })
}

/// One measured difference, with its location and its magnitude.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Finding {
    /// 1-based page, or 0 for a document-level finding.
    pub page: usize,
    /// What was measured.
    pub quantity: String,
    /// Our value, as text (numbers are twips unless the quantity says otherwise).
    pub ours: String,
    /// The reference's value.
    pub reference: String,
    /// The signed difference in twips where one is meaningful.
    pub delta: Option<i32>,
}

impl Finding {
    fn document(quantity: &str, ours: impl ToString, reference: impl ToString) -> Self {
        Self {
            page: 0,
            quantity: quantity.to_owned(),
            ours: ours.to_string(),
            reference: reference.to_string(),
            delta: None,
        }
    }

    fn page(page: usize, quantity: &str, ours: i32, reference: i32) -> Self {
        Self {
            page,
            quantity: quantity.to_owned(),
            ours: ours.to_string(),
            reference: reference.to_string(),
            delta: Some(ours - reference),
        }
    }
}

/// The edge names, in the order a bbox stores them.
const EDGES: [&str; 4] = ["x0", "y0", "x1", "y1"];

/// Every measured difference between the two renderings beyond `tolerance`
/// twips, in report order.
///
/// Findings are *measurements*, not verdicts: a document that legitimately uses a
/// face neither side has will differ, and the report says by how much and where
/// rather than passing or failing.
#[must_use]
pub fn compare(ours: &OurDocument, reference: &OracleDocument, tolerance: i32) -> Vec<Finding> {
    let mut findings = Vec::new();
    if ours.pages.len() != reference.pages.len() {
        findings.push(Finding::document(
            "page count",
            ours.pages.len(),
            reference.pages.len(),
        ));
    }
    for (index, (ours, theirs)) in ours.pages.iter().zip(&reference.pages).enumerate() {
        let page = index + 1;
        for (axis, name) in ["page width", "page height"].into_iter().enumerate() {
            if (ours.size[axis] - theirs.size[axis]).abs() > tolerance {
                findings.push(Finding::page(
                    page,
                    name,
                    ours.size[axis],
                    theirs.size[axis],
                ));
            }
        }
        match (ours.content_bbox, theirs.content_bbox) {
            (Some(a), Some(b)) => {
                for (edge, name) in EDGES.into_iter().enumerate() {
                    if (a[edge] - b[edge]).abs() > tolerance {
                        findings.push(Finding::page(
                            page,
                            &format!("text {name}"),
                            a[edge],
                            b[edge],
                        ));
                    }
                }
            }
            (a, b) if a.is_some() != b.is_some() => findings.push(Finding::document(
                &format!("page {page}: comparable text present"),
                a.is_some(),
                b.is_some(),
            )),
            _ => {}
        }
        if (ours.excluded_extent - theirs.excluded_extent).abs() > tolerance {
            findings.push(Finding::page(
                page,
                "font-parity excluded extent",
                ours.excluded_extent,
                theirs.excluded_extent,
            ));
        }
        let our_lines = ours.pinned_lines().count();
        let their_lines = theirs.lines.iter().filter(|line| line.pinned).count();
        if our_lines != their_lines {
            findings.push(Finding {
                page,
                quantity: "comparable line count".to_owned(),
                ours: our_lines.to_string(),
                reference: their_lines.to_string(),
                delta: None,
            });
        }
        findings.extend(line_findings(page, ours, theirs, tolerance));
    }
    findings
}

/// Per-line findings for one page, matched by position in the top-to-bottom
/// order of each side's *comparable* lines.
///
/// Positional matching is the honest option: there is no stable line
/// correspondence between two renderers, and inventing one by text would hide
/// exactly the case this harness exists to expose — a line that wraps in a
/// different place. When the two sides have different line counts the matching
/// stops making sense after the first divergence, so only the shared prefix is
/// reported and the line-count finding above carries the rest.
fn line_findings(
    page: usize,
    ours: &PageTextRegion,
    theirs: &OraclePage,
    tolerance: i32,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let our_lines: Vec<_> = ours.pinned_lines().collect();
    let their_lines: Vec<_> = theirs.lines.iter().filter(|line| line.pinned).collect();
    for (index, (a, b)) in our_lines.iter().zip(&their_lines).enumerate() {
        let line = index + 1;
        if (a.baseline - b.y1).abs() > tolerance * 4 {
            // The oracle reports no baseline, only the descriptor box, so the
            // comparable vertical quantity is the box bottom. A 4x band, because
            // this is a per-line position inside a page whose total drift is
            // already reported by the bbox edges above.
            findings.push(Finding::page(
                page,
                &format!("line {line} bottom"),
                a.y1,
                b.y1,
            ));
        }
        if (a.x1 - b.x1).abs() > tolerance {
            findings.push(Finding::page(
                page,
                &format!("line {line} right edge"),
                a.x1,
                b.x1,
            ));
        }
        if a.words != b.words {
            findings.push(Finding {
                page,
                quantity: format!("line {line} words"),
                ours: a.words.to_string(),
                reference: b.words.to_string(),
                delta: None,
            });
        }
    }
    findings
}

/// Formats the whole comparison as a report a human reads and a diff tool can
/// track: a header naming both renderers and both font sets, a per-page table,
/// and the findings.
#[must_use]
pub fn report(
    input: &Path,
    ours: &OurDocument,
    reference: &OracleDocument,
    findings: &[Finding],
    tolerance: i32,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "document: {}", input.display());
    let _ = writeln!(
        out,
        "reference: {}",
        if reference.version.is_empty() {
            "LibreOffice (version unknown)"
        } else {
            &reference.version
        }
    );
    let _ = writeln!(out, "tolerance: {tolerance} twips");
    let our_fonts = our_font_names(ours);
    let _ = writeln!(out, "fonts ours:      {}", join(&our_fonts));
    let _ = writeln!(out, "fonts reference: {}", join(&reference.fonts));
    let _ = writeln!(
        out,
        "pages ours: {}   reference: {}",
        ours.pages.len(),
        reference.pages.len()
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{:>4}  {:>28}  {:>28}  {:>18}  {:>9}",
        "page", "text bbox ours", "text bbox reference", "delta x0,y0,x1,y1", "lines o/r"
    );
    for (index, ours_page) in ours.pages.iter().enumerate() {
        let theirs = reference.pages.get(index);
        let ours_box = bbox_text(ours_page.content_bbox);
        let their_box = bbox_text(theirs.and_then(|p| p.content_bbox));
        let delta = match (ours_page.content_bbox, theirs.and_then(|p| p.content_bbox)) {
            (Some(a), Some(b)) => format!(
                "{:+},{:+},{:+},{:+}",
                a[0] - b[0],
                a[1] - b[1],
                a[2] - b[2],
                a[3] - b[3]
            ),
            _ => "-".to_owned(),
        };
        let our_lines = ours_page.pinned_lines().count();
        let their_lines = theirs.map_or(0, |p| p.lines.iter().filter(|line| line.pinned).count());
        let _ = writeln!(
            out,
            "{:>4}  {ours_box:>28}  {their_box:>28}  {delta:>18}  {:>9}",
            index + 1,
            format!("{our_lines}/{their_lines}")
        );
    }
    let _ = writeln!(out);
    if findings.is_empty() {
        let _ = writeln!(out, "no differences beyond {tolerance} twips");
    } else {
        let _ = writeln!(out, "{} finding(s):", findings.len());
        for finding in findings {
            let where_ = if finding.page == 0 {
                "document".to_owned()
            } else {
                format!("page {}", finding.page)
            };
            let delta = finding
                .delta
                .map_or_else(String::new, |d| format!("  (delta {d:+})"));
            let _ = writeln!(
                out,
                "  {where_}: {} — ours {}, reference {}{delta}",
                finding.quantity, finding.ours, finding.reference
            );
        }
    }
    out
}

/// A side-by-side dump of one page's lines, for reading *where* a divergence
/// starts.
///
/// Lines are listed in each side's own top-to-bottom order and paired by
/// position, excluded lines included and marked, because the first line at which
/// the two orders stop describing the same text is the finding — and hiding the
/// excluded lines would hide exactly the case where one side measured text the
/// other dropped.
#[must_use]
pub fn page_detail(ours: &PageTextRegion, theirs: &OraclePage) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:>4}  {:>32}  {:>5}  {:>24}  {:>32}  {:>5}",
        "line", "ours x0..x1 / y0..y1", "words", "ours fonts", "reference x0..x1 / y0..y1", "words"
    );
    let rows = ours.lines.len().max(theirs.lines.len());
    for index in 0..rows {
        let a = ours.lines.get(index);
        let b = theirs.lines.get(index);
        let ours_box = a.map_or_else(
            || "-".to_owned(),
            |l| {
                format!(
                    "{}..{} / {}..{}{}",
                    l.x0,
                    l.x1,
                    l.y0,
                    l.y1,
                    if l.pinned { "" } else { "  [excluded]" }
                )
            },
        );
        let their_box = b.map_or_else(
            || "-".to_owned(),
            |l| {
                format!(
                    "{}..{} / {}..{}{}",
                    l.x0,
                    l.x1,
                    l.y0,
                    l.y1,
                    if l.pinned { "" } else { "  [excluded]" }
                )
            },
        );
        let fonts = a.map_or_else(String::new, |l| {
            l.fonts
                .iter()
                .map(|f| casual_doc_layout::text_region::font_label(*f))
                .collect::<Vec<_>>()
                .join("+")
        });
        let _ = writeln!(
            out,
            "{:>4}  {ours_box:>32}  {:>5}  {fonts:>24}  {their_box:>32}  {:>5}",
            index + 1,
            a.map_or_else(|| "-".to_owned(), |l| l.words.to_string()),
            b.map_or_else(|| "-".to_owned(), |l| l.words.to_string()),
        );
    }
    out
}

/// The distinct faces our side resolved, across every page.
fn our_font_names(ours: &OurDocument) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for name in ours.pages.iter().flat_map(PageTextRegion::font_names) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names.sort();
    names
}

fn join(names: &[String]) -> String {
    if names.is_empty() {
        "(none)".to_owned()
    } else {
        names.join(", ")
    }
}

fn bbox_text(bbox: Option<[i32; 4]>) -> String {
    bbox.map_or_else(
        || "(no comparable text)".to_owned(),
        |b| format!("{},{},{},{}", b[0], b[1], b[2], b[3]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oracle::OracleLine;
    use casual_doc_layout::text_region::TextLine;

    fn our_page(bbox: Option<[i32; 4]>, lines: Vec<TextLine>) -> PageTextRegion {
        PageTextRegion {
            size: [11906, 16838],
            content_bbox: bbox,
            excluded_extent: 0,
            excluded_lines: 0,
            lines,
        }
    }

    fn our_line(y1: i32, x1: i32, words: usize) -> TextLine {
        TextLine {
            x0: 1134,
            y0: y1 - 250,
            x1,
            y1,
            baseline: y1 - 50,
            pinned: true,
            words,
            fonts: Vec::new(),
        }
    }

    fn their_line(y1: i32, x1: i32, words: usize) -> OracleLine {
        OracleLine {
            x0: 1136,
            y0: y1 - 250,
            x1,
            y1,
            pinned: true,
            words,
        }
    }

    fn their_page(bbox: Option<[i32; 4]>, lines: Vec<OracleLine>) -> OraclePage {
        OraclePage {
            size: [11906, 16838],
            content_bbox: bbox,
            excluded_extent: 0,
            excluded_lines: 0,
            lines,
        }
    }

    fn reference(pages: Vec<OraclePage>) -> OracleDocument {
        OracleDocument {
            version: "LibreOffice 26.2.4.2".to_owned(),
            fonts: vec!["LiberationSerif".to_owned()],
            pages,
        }
    }

    #[test]
    fn identical_geometry_produces_no_findings() {
        let ours = OurDocument {
            pages: vec![our_page(
                Some([1134, 800, 6900, 2400]),
                vec![our_line(1100, 6900, 9)],
            )],
        };
        let theirs = reference(vec![their_page(
            Some([1134, 800, 6900, 2400]),
            vec![their_line(1100, 6900, 9)],
        )]);
        assert!(compare(&ours, &theirs, 40).is_empty());
    }

    #[test]
    fn a_page_count_difference_is_reported_at_document_level() {
        let ours = OurDocument {
            pages: vec![our_page(None, Vec::new()), our_page(None, Vec::new())],
        };
        let theirs = reference(vec![their_page(None, Vec::new())]);
        let findings = compare(&ours, &theirs, 40);
        assert_eq!(findings[0].page, 0);
        assert_eq!(findings[0].quantity, "page count");
        assert_eq!((&*findings[0].ours, &*findings[0].reference), ("2", "1"));
    }

    #[test]
    fn a_bbox_edge_beyond_tolerance_is_reported_with_its_delta() {
        let ours = OurDocument {
            pages: vec![our_page(
                Some([1134, 800, 6900, 2700]),
                vec![our_line(1100, 6900, 9)],
            )],
        };
        let theirs = reference(vec![their_page(
            Some([1134, 800, 6900, 2400]),
            vec![their_line(1100, 6900, 9)],
        )]);
        let findings = compare(&ours, &theirs, 40);
        let y1 = findings
            .iter()
            .find(|f| f.quantity == "text y1")
            .expect("the bottom edge differs by 300 twips");
        assert_eq!(y1.delta, Some(300));
        // …and a 30-twip nudge, inside the band, is not reported.
        let ours = OurDocument {
            pages: vec![our_page(
                Some([1134, 800, 6900, 2430]),
                vec![our_line(1100, 6900, 9)],
            )],
        };
        assert!(compare(&ours, &theirs, 40).is_empty());
    }

    #[test]
    fn fewer_words_on_a_line_is_reported_even_when_the_page_box_agrees() {
        // The live signal this harness was built for: the page's outer box can
        // agree to the twip while each line holds fewer words, because the
        // advances are wider. A bbox-only comparison cannot see it.
        let ours = OurDocument {
            pages: vec![our_page(
                Some([1134, 800, 6900, 2400]),
                vec![our_line(1100, 6900, 7), our_line(1400, 6900, 7)],
            )],
        };
        let theirs = reference(vec![their_page(
            Some([1134, 800, 6900, 2400]),
            vec![their_line(1100, 6900, 9), their_line(1400, 6900, 9)],
        )]);
        let findings = compare(&ours, &theirs, 40);
        let words: Vec<_> = findings
            .iter()
            .filter(|f| f.quantity.ends_with("words"))
            .collect();
        assert_eq!(words.len(), 2, "both lines hold fewer words: {findings:#?}");
        assert_eq!((&*words[0].ours, &*words[0].reference), ("7", "9"));
    }

    #[test]
    fn an_unpinned_line_is_not_compared_on_either_side() {
        // Font-parity scoping: a line neither side can pin is excluded before any
        // advance is compared, so it cannot manufacture a finding.
        let mut ours_line = our_line(1100, 9999, 3);
        ours_line.pinned = false;
        let ours = OurDocument {
            pages: vec![our_page(Some([1134, 800, 6900, 2400]), vec![ours_line])],
        };
        let mut their = their_line(1100, 1200, 1);
        their.pinned = false;
        let theirs = reference(vec![their_page(Some([1134, 800, 6900, 2400]), vec![their])]);
        assert!(compare(&ours, &theirs, 40).is_empty());
    }

    #[test]
    fn the_report_names_both_renderers_and_both_font_sets() {
        let ours = OurDocument {
            pages: vec![our_page(
                Some([1134, 800, 6900, 2400]),
                vec![our_line(1100, 6900, 9)],
            )],
        };
        let theirs = reference(vec![their_page(
            Some([1134, 800, 6900, 2400]),
            vec![their_line(1100, 6900, 9)],
        )]);
        let text = report(Path::new("a.docx"), &ours, &theirs, &[], 40);
        assert!(text.contains("LibreOffice 26.2.4.2"), "{text}");
        assert!(text.contains("fonts reference: LiberationSerif"), "{text}");
        assert!(text.contains("no differences beyond 40 twips"), "{text}");
    }
}
