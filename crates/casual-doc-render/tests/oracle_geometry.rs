//! Phase H2 of the oracle visual-fidelity harness (docs/94): diff our page
//! geometry against a pinned **LibreOffice** reference.
//!
//! Unlike the self-referential H1 snapshot (which locks *our own* geometry), H2
//! compares against an independent oracle: LibreOffice renders each corpus
//! fixture to PDF, and `scripts/oracle/extract-geometry.sh` reduces the PDF word
//! boxes to a per-page reference (page size + the content text-region bounding
//! box) committed under `fixtures/oracle/<id>.geom.json`. This test imports the
//! same fixture, paginates it, reduces our placed content to the same shape, and
//! asserts the two agree within a tolerance band (rasterizer/shaper differences
//! make exact equality impossible — see docs/94 §Determinism).
//!
//! The reference files are produced by a separate, pinned-LibreOffice CI job
//! (`.github/workflows/oracle-geometry.yml`), not the hermetic main CI. Until a
//! fixture has a committed reference, its comparison is **skipped** — so this
//! test is inert (never red) before the oracle job has run, and becomes a live
//! fidelity gate once references land.
//!
//! Font parity is the crux and is already solved: both renderers use the bundled
//! metric-compatible faces (Liberation/Carlito/Caladea), installed in the oracle
//! container, so line breaking and advances match (docs/40, docs/94).

use std::path::PathBuf;

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_ooxml::{DocxPackage, PackageLimits};

/// One page's oracle-comparable geometry, in twips: the page size and the
/// bounding box of its content (`[x0, y0, x1, y1]`). Page-level rather than
/// per-block so it survives the absence of a stable block correspondence between
/// the two renderers while still catching gross placement/extent errors.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PageGeom {
    size: [i32; 2],
    content_bbox: Option<[i32; 4]>,
}

/// The placement tolerance (twips). 40 twips = 2pt absorbs anti-aliasing and
/// sub-point shaper differences without hiding a real regression (docs/94: ±1pt
/// on placement is the design target; the content bbox aggregates several edges,
/// so it is given a touch more slack).
const TOLERANCE_TWIPS: i32 = 40;

/// Every human-readable geometry discrepancy between `ours` and the `oracle`
/// reference beyond `tolerance`. Empty ⇒ the two agree.
fn geometry_diffs(ours: &[PageGeom], oracle: &[PageGeom], tolerance: i32) -> Vec<String> {
    let mut diffs = Vec::new();
    if ours.len() != oracle.len() {
        diffs.push(format!(
            "page count: ours={} oracle={}",
            ours.len(),
            oracle.len()
        ));
        return diffs;
    }
    for (index, (a, b)) in ours.iter().zip(oracle).enumerate() {
        for (axis, (av, bv)) in a.size.iter().zip(&b.size).enumerate() {
            if (av - bv).abs() > tolerance {
                let dim = if axis == 0 { "width" } else { "height" };
                diffs.push(format!("page {}: {dim} ours={av} oracle={bv}", index + 1));
            }
        }
        match (a.content_bbox, b.content_bbox) {
            (Some(a_box), Some(b_box)) => {
                const EDGES: [&str; 4] = ["x0", "y0", "x1", "y1"];
                for (edge, (av, bv)) in a_box.iter().zip(&b_box).enumerate() {
                    if (av - bv).abs() > tolerance {
                        diffs.push(format!(
                            "page {}: content {} ours={av} oracle={bv}",
                            index + 1,
                            EDGES[edge]
                        ));
                    }
                }
            }
            (a_box, b_box) if a_box != b_box => diffs.push(format!(
                "page {}: content presence ours={} oracle={}",
                index + 1,
                a_box.is_some(),
                b_box.is_some()
            )),
            _ => {}
        }
    }
    diffs
}

/// Our page geometry for a `.docx`: import → paginate → reduce each page to its
/// size and the union bbox of its placed body fragments.
fn our_geometry(docx: &[u8]) -> Vec<PageGeom> {
    let mut package = DocxPackage::open(docx, PackageLimits::default()).unwrap();
    let document = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .unwrap()
    .document;
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&document, &shaper);
    layout
        .pages
        .iter()
        .map(|page| {
            let content_bbox = page
                .placed
                .iter()
                .fold(None, |acc: Option<[i32; 4]>, placed| {
                    let r = placed.rect;
                    let x0 = r.origin.x.raw();
                    let y0 = r.origin.y.raw();
                    let x1 = x0 + r.size.width.raw();
                    let y1 = y0 + r.size.height.raw();
                    Some(match acc {
                        None => [x0, y0, x1, y1],
                        Some([ax0, ay0, ax1, ay1]) => {
                            [ax0.min(x0), ay0.min(y0), ax1.max(x1), ay1.max(y1)]
                        }
                    })
                });
            PageGeom {
                size: [page.page_size.width.raw(), page.page_size.height.raw()],
                content_bbox,
            }
        })
        .collect()
}

/// Reads the committed oracle reference for `fixture_id`, or `None` if the
/// LibreOffice re-bless job has not produced it yet.
fn oracle_reference(fixture_id: &str) -> Option<Vec<PageGeom>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/oracle")
        .join(format!("{fixture_id}.geom.json"));
    let text = std::fs::read_to_string(path).ok()?;
    Some(parse_oracle_geometry(&text))
}

/// Parses the oracle geometry JSON (the shape `extract-geometry.sh` emits):
/// `{ "pages": [ { "sizeTwips": [w,h], "contentBboxTwips": [x0,y0,x1,y1] | null } ] }`.
/// Kept dependency-free (no serde) so the harness stays light; a malformed file
/// is a hard error (a produced reference must be well-formed).
fn parse_oracle_geometry(text: &str) -> Vec<PageGeom> {
    let value: serde_json::Value =
        serde_json::from_str(text).expect("oracle geometry is valid JSON");
    value["pages"]
        .as_array()
        .expect("oracle geometry has a pages array")
        .iter()
        .map(|page| {
            let size = page["sizeTwips"].as_array().expect("page sizeTwips array");
            let content_bbox = page["contentBboxTwips"].as_array().map(|bbox| {
                [
                    bbox[0].as_i64().unwrap() as i32,
                    bbox[1].as_i64().unwrap() as i32,
                    bbox[2].as_i64().unwrap() as i32,
                    bbox[3].as_i64().unwrap() as i32,
                ]
            });
            PageGeom {
                size: [
                    size[0].as_i64().unwrap() as i32,
                    size[1].as_i64().unwrap() as i32,
                ],
                content_bbox,
            }
        })
        .collect()
}

const LIBREOFFICE_CORPUS: &[u8] =
    include_bytes!("../../../fixtures/corpus/real-producer-libreoffice.docx");

// Geometry is shaped-metric dependent, so like the H1 snapshot the comparison is
// pinned to the deterministic Linux/macOS platforms (docs/94, PR #316).
#[cfg_attr(
    target_os = "windows",
    ignore = "shaped geometry differs on Windows; the oracle reference is blessed on Linux/macOS"
)]
#[test]
fn our_geometry_matches_the_libreoffice_oracle_within_tolerance() {
    let Some(oracle) = oracle_reference("docx-real-producer-libreoffice") else {
        // No committed reference yet: the pinned-LibreOffice re-bless job
        // (.github/workflows/oracle-geometry.yml) has not run. Inert, not red.
        return;
    };
    let ours = our_geometry(LIBREOFFICE_CORPUS);
    let diffs = geometry_diffs(&ours, &oracle, TOLERANCE_TWIPS);
    assert!(
        diffs.is_empty(),
        "layout geometry diverged from the LibreOffice oracle beyond {TOLERANCE_TWIPS} twips:\n  {}",
        diffs.join("\n  ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(w: i32, h: i32, bbox: Option<[i32; 4]>) -> PageGeom {
        PageGeom {
            size: [w, h],
            content_bbox: bbox,
        }
    }

    #[test]
    fn identical_geometry_has_no_diffs() {
        let g = vec![page(12240, 15840, Some([1440, 1440, 10800, 14400]))];
        assert!(geometry_diffs(&g, &g, TOLERANCE_TWIPS).is_empty());
    }

    #[test]
    fn within_tolerance_is_accepted_but_beyond_is_reported() {
        let ours = vec![page(12240, 15840, Some([1440, 1440, 10800, 14400]))];
        // Every edge nudged by 30 twips (< 40 tolerance): still a match.
        let close = vec![page(12240, 15840, Some([1470, 1410, 10770, 14430]))];
        assert!(geometry_diffs(&ours, &close, TOLERANCE_TWIPS).is_empty());
        // One edge pushed 200 twips out: reported.
        let far = vec![page(12240, 15840, Some([1440, 1440, 11000, 14400]))];
        let diffs = geometry_diffs(&ours, &far, TOLERANCE_TWIPS);
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].contains("content x1"), "got {diffs:?}");
    }

    #[test]
    fn page_count_and_size_mismatches_are_reported() {
        let one = vec![page(12240, 15840, None)];
        let two = vec![page(12240, 15840, None), page(12240, 15840, None)];
        assert_eq!(geometry_diffs(&one, &two, TOLERANCE_TWIPS).len(), 1);

        let wide = vec![page(15840, 15840, None)];
        let diffs = geometry_diffs(&one, &wide, TOLERANCE_TWIPS);
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].contains("width"), "got {diffs:?}");
    }

    #[test]
    fn our_geometry_extracts_pages_from_a_real_docx() {
        // The extraction path itself is exercised even before an oracle
        // reference exists: the corpus fixture paginates to at least one sized
        // page with content.
        let geom = our_geometry(LIBREOFFICE_CORPUS);
        assert!(
            !geom.is_empty(),
            "the corpus paginates to at least one page"
        );
        assert!(geom[0].size[0] > 0 && geom[0].size[1] > 0);
        assert!(geom.iter().any(|p| p.content_bbox.is_some()));
    }
}
