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
use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::GlyphRun;
use casual_doc_layout::units::Point;
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
/// size and the union bbox of its **inked text**.
///
/// The quantity here has to match what the oracle measures, and getting that
/// wrong is exactly how this gate first went red. `extract-geometry.sh` unions
/// `pdftotext -bbox` **word** boxes; an earlier version of this function unioned
/// `page.placed[].rect` — *block layout boxes* — which are a different thing
/// entirely. On the first fixture that produced `x1 = 11339` (page width 11906
/// minus the 567 right margin: a full-width paragraph box) against the oracle's
/// `7933`, where the last glyph actually ends, and `y0 = 567` (the top margin)
/// against `808` (the top of the first word box, which sits lower because a line
/// box carries leading above its glyphs). The comparison could never have
/// passed. See docs/94 §H2, which specifies the *text region*, and docs/105
/// FID-P-01.
///
/// What a word box actually is matters for the choice made below: `pdftotext`
/// derives it from the font's metrics for the text it contains, not from
/// rasterized glyph ink. So the comparable quantity on our side is
/// baseline ± the run's own `ascent`/`descent`, which is what this computes —
/// not true outline ink, which would need per-glyph bounding boxes and would
/// *not* match the oracle any better.
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
            let mut bbox = BBox::default();
            for placed in &page.placed {
                accumulate_block(&mut bbox, &placed.fragment, placed.rect.origin);
            }
            PageGeom {
                size: [page.page_size.width.raw(), page.page_size.height.raw()],
                content_bbox: bbox.finish(),
            }
        })
        .collect()
}

/// A growing union of text extents, in page-local twips.
#[derive(Clone, Copy, Debug, Default)]
struct BBox {
    bounds: Option<[i32; 4]>,
}

impl BBox {
    fn add(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        self.bounds = Some(match self.bounds {
            None => [x0, y0, x1, y1],
            Some([ax0, ay0, ax1, ay1]) => [ax0.min(x0), ay0.min(y0), ax1.max(x1), ay1.max(y1)],
        });
    }

    fn finish(self) -> Option<[i32; 4]> {
        self.bounds
    }
}

/// Unions the text extents of one block fragment, translated by `origin` (the
/// fragment's page-local top-left). Recurses into table cells, because their
/// text is ink on the page and `pdftotext` reports it like any other word.
///
/// Non-text fragments contribute nothing on purpose: `pdftotext -bbox` emits
/// `<word>` elements only, so an image or a rule has no word box for the oracle
/// to have seen, and counting one here would widen our bbox against a reference
/// that never included it.
fn accumulate_block(bbox: &mut BBox, fragment: &BlockFragment, origin: Point) {
    match fragment {
        BlockFragment::Paragraph { lines, .. } => {
            for line in &lines.lines {
                for run in &line.runs {
                    accumulate_run(bbox, run, origin);
                }
            }
        }
        BlockFragment::TableRow { cells, .. } => {
            for cell in cells {
                // A cell's blocks are flowed relative to the cell's content
                // box: the row origin, plus the cell's own x, plus its margins.
                let cell_origin = Point::new(
                    origin.x + cell.x + cell.margins.start,
                    origin.y + cell.margins.top,
                );
                for block in &cell.blocks {
                    accumulate_block(bbox, block, cell_origin);
                }
            }
        }
    }
}

/// Unions one glyph run's extent.
///
/// Trailing whitespace is excluded. A justified or space-padded line carries
/// advances past its last visible glyph, and `pdftotext` — which reports words —
/// never sees them; including them would inflate `x1` against every reference.
fn accumulate_run(bbox: &mut BBox, run: &GlyphRun, origin: Point) {
    let run_x = origin.x.raw() + run.origin.x.raw();
    let baseline = origin.y.raw() + run.origin.y.raw();

    // Walk the advances, tracking where the last non-whitespace glyph ends and
    // where the first one begins. A run that is entirely whitespace contributes
    // nothing at all.
    let mut pen = run_x;
    let mut first_ink: Option<i32> = None;
    let mut last_ink_end: Option<i32> = None;
    for glyph in &run.glyphs {
        let advance = glyph.advance.raw();
        if !glyph.is_whitespace {
            if first_ink.is_none() {
                first_ink = Some(pen);
            }
            last_ink_end = Some(pen + advance);
        }
        pen += advance;
    }
    let (Some(x0), Some(x1)) = (first_ink, last_ink_end) else {
        return;
    };

    // Vertically: the word box a PDF text extractor reports is the font's
    // ascent/descent band around the baseline, which is what these fields hold.
    bbox.add(
        x0,
        baseline - run.ascent.raw(),
        x1,
        baseline + run.descent.raw(),
    );
}

/// Reads the committed oracle reference for `fixture_id`, or `None` if there is
/// no **trustworthy** one yet.
///
/// A reference is only trustworthy if `fixtures/oracle/.fonts` records that the
/// producing container actually resolved the bundled metric-compatible faces.
/// This is not ceremony — the first reference ever produced was silently wrong
/// for exactly this reason. The job installs Liberation/Carlito/Caladea, but
/// nothing verified LibreOffice *used* them, and it did not: the reference came
/// back ~17.5% wider than the same string shaped in Liberation Sans
/// (0.616 em/glyph against Liberation Sans's 0.524 at 24pt) and with its first
/// baseline pushed down, both of which are the signature of a wider, taller
/// substitute face. Comparing against that reference does not measure our
/// fidelity; it measures which font the CI container happened to pick.
///
/// So: no recorded font provenance, no gate. An unverified reference is worse
/// than none, because it fails for a reason that has nothing to do with the
/// engine and trains everyone to ignore the result.
fn oracle_reference(fixture_id: &str) -> Option<Vec<PageGeom>> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/oracle");
    let provenance = std::fs::read_to_string(dir.join(".fonts")).ok()?;
    if !font_provenance_is_trustworthy(&provenance) {
        return None;
    }
    let text = std::fs::read_to_string(dir.join(format!("{fixture_id}.geom.json"))).ok()?;
    Some(parse_oracle_geometry(&text))
}

/// Whether the recorded `fc-match` provenance shows the oracle container
/// resolved every bundled family to itself rather than to a substitute.
///
/// The re-bless job writes one `<requested> => <resolved>` line per family.
/// Any line whose resolved family differs from the requested one means
/// LibreOffice shaped with something else, and the reference is void.
fn font_provenance_is_trustworthy(provenance: &str) -> bool {
    let mut checked = 0usize;
    for line in provenance.lines() {
        let Some((requested, resolved)) = line.split_once("=>") else {
            continue;
        };
        let (requested, resolved) = (requested.trim(), resolved.trim());
        if requested.is_empty() || resolved.is_empty() {
            continue;
        }
        if !resolved.eq_ignore_ascii_case(requested) {
            return false;
        }
        checked += 1;
    }
    // An empty or unparsable file proves nothing, so it is not trust.
    checked >= 3
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

    #[test]
    fn the_content_bbox_measures_text_not_the_column_it_sits_in() {
        // The guard for the defect that made this gate unpassable: the bbox
        // used to be the union of block LAYOUT rects, and a paragraph's rect
        // spans the full text column regardless of how short its text is. On
        // this fixture that produced x1 = 11339 — exactly page width 11906
        // minus the 567 right margin — against an oracle that unions
        // `pdftotext` WORD boxes and reported 7933.
        //
        // So: the right edge of the content must be strictly inside the column,
        // because no line in this fixture fills it. If this ever equals the
        // column edge again, the bbox has gone back to measuring boxes.
        let geom = our_geometry(LIBREOFFICE_CORPUS);
        let page = geom.first().expect("one page");
        let [x0, y0, x1, y1] = page.content_bbox.expect("the page has text");

        let page_width = page.size[0];
        // The fixture's margins are symmetric, so the column's right edge is
        // page width minus the left inset we measured.
        let column_right = page_width - x0;
        assert!(
            x1 < column_right - 100,
            "content x1 ({x1}) should be well inside the column right edge \
             ({column_right}); equal to it means the bbox is measuring \
             full-width paragraph boxes again, not text",
        );

        // Vertically: a real text band, not a degenerate or inverted one. (An
        // earlier draft compared `y0` against `x0` as a stand-in for the top
        // margin — they are different margins, 567 vs 1134 on this fixture, so
        // the assertion was simply wrong. The honest check is that the band is
        // inside the page and has height.)
        assert!(y0 > 0, "content top ({y0}) is inside the page");
        assert!(y1 > y0, "the vertical band has height ({y0}..{y1})");
        assert!(
            y1 < page.size[1],
            "content bottom ({y1}) is inside the page ({})",
            page.size[1],
        );
    }
}

#[cfg(test)]
mod provenance_tests {
    use super::font_provenance_is_trustworthy;

    #[test]
    fn a_reference_whose_fonts_all_resolved_to_themselves_is_trusted() {
        let ok = "Liberation Sans => Liberation Sans\n\
                  Liberation Serif => Liberation Serif\n\
                  Liberation Mono => Liberation Mono\n\
                  Carlito => Carlito\n\
                  Caladea => Caladea\n";
        assert!(font_provenance_is_trustworthy(ok));
    }

    #[test]
    fn a_single_substituted_face_voids_the_whole_reference() {
        // Exactly what produced the first (wrong) reference: the packages were
        // installed, fontconfig resolved something else, and nothing noticed.
        let substituted = "Liberation Sans => DejaVu Sans\n\
                           Liberation Serif => Liberation Serif\n\
                           Liberation Mono => Liberation Mono\n\
                           Carlito => Carlito\n\
                           Caladea => Caladea\n";
        assert!(!font_provenance_is_trustworthy(substituted));
    }

    #[test]
    fn missing_or_unparsable_provenance_is_not_trust() {
        assert!(!font_provenance_is_trustworthy(""));
        assert!(!font_provenance_is_trustworthy(
            "no arrows here\njust noise\n"
        ));
        // Too few families checked to be the real record.
        assert!(!font_provenance_is_trustworthy("Carlito => Carlito\n"));
    }
}
