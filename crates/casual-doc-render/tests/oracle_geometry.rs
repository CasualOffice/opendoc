//! Phase H2 of the oracle visual-fidelity harness (docs/94): diff our page
//! geometry against a pinned **LibreOffice** reference.
//!
//! Unlike the self-referential H1 snapshot (which locks *our own* geometry), H2
//! compares against an independent oracle: LibreOffice renders each corpus
//! fixture to PDF, and `scripts/oracle/extract-geometry.sh` reduces the PDF word
//! boxes to a per-page reference committed under `fixtures/oracle/<id>.geom.json`.
//! This test imports the same fixture, paginates it, reduces our composed text to
//! the *same quantity*, and asserts the two agree within a tolerance band.
//!
//! # The compared quantity: the text region, not the block boxes
//!
//! Both sides reduce a page to the union, over every painted piece of text, of
//!
//! ```text
//! x: [pen start of the first non-space glyph, pen end of the last non-space glyph]
//! y: [baseline − face ascent,                 baseline + face descent]
//! ```
//!
//! That is exactly what `pdftotext -bbox` reports per *word*. Despite the
//! extraction script once calling it "the inked text region", poppler's word box
//! is **not** ink: `yMin`/`yMax` come from the PDF font descriptor's
//! ascent/descent scaled to the run's size, and `xMin`/`xMax` are pen positions,
//! not outline extents. Measured on the corpus fixture, LibreOffice's
//! `The quick brown fox …` word boxes are 13.284pt tall, and Liberation Serif's
//! ascent+descent at 12pt is (1824+443)/2048 × 12pt = 13.284pt — lowercase ink is
//! nowhere near that tall. Our shaped runs carry the same two numbers
//! (`GlyphRun::ascent`/`descent`, per run, from the same faces), so the oracle's
//! quantity is reproducible **exactly**; there is no ink-vs-layout fudge factor
//! and none is applied.
//!
//! This is the bug the gate first caught. Our side used to union
//! `page.placed[].rect` — paragraph **box** rects, a different quantity that
//! includes `w:spacing` before/after and spans the whole flowed column width. On
//! the corpus fixture that alone mis-measured `y0` by −241 twips (the heading's
//! `w:spacing@before="240"`), `y1` by +284 (the last paragraph's `after="283"`),
//! and `x1` by +4420 (the column width instead of the longest line).
//!
//! # Font-parity scoping: lines the pinned faces cannot shape are excluded
//!
//! docs/94's font-parity argument — both renderers shape with the bundled
//! metric-compatible faces (Liberation/Carlito/Caladea), so advances match — holds
//! only for the code points those faces **cover**. The corpus fixture also
//! contains CJK (`日本語`) and Arabic (`العربية`) runs, which none of them covers:
//! the oracle container substitutes whatever its base image happens to provide and
//! we intern a dynamic fallback face, so those runs' advances are unpinnable — and,
//! because a wider substitute shifts everything after it on the same line, so are
//! the positions of the Latin words that follow. That, and not a layout error, is
//! the whole of the remaining 1014-twip `x1` gap: our Latin geometry matches
//! LibreOffice's to the twip (the 24pt heading ends at 6919 in both).
//!
//! So both sides group their text boxes into lines (by vertical overlap) and drop
//! any line that contains text the pinned faces cannot shape — the oracle by code
//! point, we by resolved face. The count of dropped lines is itself compared
//! exactly, so the filter cannot silently swallow a line the oracle still measured.
//!
//! # Residual and tolerance
//!
//! With both sides measuring the text region over the pinned-parity lines, the
//! corpus fixture's residual is:
//!
//! | edge | ours | oracle | Δ | cause |
//! |------|------|--------|---|-------|
//! | `x0` | 1134 | 1136 | 2 | LibreOffice rounds the PDF text origin to 0.1pt |
//! | `y0` | 814 | 808 | 6 | half-leading (below) |
//! | `x1` | 6919 | 6919 | 0 | — |
//! | `y1` | 2472 | 2467 | 5 | half-leading (below) |
//!
//! The vertical residual is **not** an ink inset; it is leading distribution.
//! `parley` centers a face's line gap around the text box (half above the ascent),
//! LibreOffice puts all of it below the descent, so every baseline of ours sits
//! `lineGap / 2` lower. For the bundled faces `lineGap ≈ 0.0327 em`, i.e. ≈0.016 em
//! of offset: 4 twips at 12pt, 8 at 24pt, and it grows linearly with the largest
//! font on the page.
//!
//! [`TOLERANCE_TWIPS`] is therefore sized to bound that term over a realistic font
//! range rather than to clear today's numbers — see its comment.
//!
//! # Blessing protocol
//!
//! The reference files are produced by a separate, pinned-LibreOffice CI job
//! (`.github/workflows/oracle-geometry.yml`), not the hermetic main CI. A fixture
//! with no committed reference is **skipped** entirely, and one whose reference
//! predates the current extraction semantics ([`ORACLE_SCHEMA`]) is compared only
//! on page count and page size — the two quantities no schema bump has changed —
//! until the re-bless job regenerates it. Never hand-edit a blessed reference.

use std::path::PathBuf;

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::fonts::{
    CALADEA, CARLITO, LIBERATION_MONO, LIBERATION_SANS, LIBERATION_SERIF,
};
use casual_doc_layout::page::Page;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::{FontId, GlyphRun};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

/// The extraction semantics the committed references must have been produced
/// with; `scripts/oracle/extract-geometry.sh` stamps it as `"schema"`. Bump it
/// whenever the *meaning* of `contentBboxTwips` or `excludedLines` changes, so a
/// reference blessed under the old meaning is not silently compared against the
/// new one. Schema 1 unioned every word box on the page; schema 2 unions the text
/// region of the font-parity lines only.
const ORACLE_SCHEMA: u64 = 2;

/// One page's oracle-comparable geometry, in twips: the page size, the bounding
/// box of its comparable text (`[x0, y0, x1, y1]`), and how many lines were
/// dropped as unshapeable by the pinned faces. Page-level rather than per-block so
/// it survives the absence of a stable block correspondence between the two
/// renderers while still catching gross placement/extent errors.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PageGeom {
    size: [i32; 2],
    content_bbox: Option<[i32; 4]>,
    excluded_lines: u32,
}

/// Whether a committed reference still speaks the current extraction semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContentGate {
    /// The reference is at [`ORACLE_SCHEMA`]: the text region is compared.
    Live,
    /// The reference predates it: only page count and size are compared, because
    /// its `contentBboxTwips` measures a quantity this test no longer produces.
    StaleReference,
}

/// The placement tolerance (twips).
///
/// This is a bound on the *known* residual (see the module docs), not a number
/// picked to clear a failing case. Two terms contribute:
///
/// - **≈2 twips** of PDF coordinate rounding — LibreOffice writes text origins at
///   0.1pt resolution.
/// - **`lineGap / 2` of the largest face on the page** — `parley` splits a face's
///   line gap around the text box, LibreOffice puts it all below the descent. For
///   the bundled metric-compatible faces `lineGap ≈ 0.0327 em`, so this is
///   ≈0.016 em: 4 twips at 12pt, 8 at 24pt, 16 at 48pt.
///
/// 40 twips (2pt) therefore holds for any face in the bundle up to ≈115pt, with
/// ~5× headroom over the corpus fixture's observed 6-twip worst edge. It is *not*
/// enough slack to hide a real layout error: the mis-measurement this gate first
/// caught was 241–4420 twips, and a one-line vertical slip at body size is 276.
///
/// If a future fixture legitimately needs more than this, add the *reason* here —
/// do not raise the number to make a red edge green.
const TOLERANCE_TWIPS: i32 = 40;

/// Every human-readable geometry discrepancy between `ours` and the `oracle`
/// reference beyond `tolerance`. Empty ⇒ the two agree.
fn geometry_diffs(
    ours: &[PageGeom],
    oracle: &[PageGeom],
    tolerance: i32,
    gate: ContentGate,
) -> Vec<String> {
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
        if gate == ContentGate::StaleReference {
            continue;
        }
        if a.excluded_lines != b.excluded_lines {
            diffs.push(format!(
                "page {}: font-parity lines excluded ours={} oracle={} \
                 (the two renderers disagree about which text the pinned faces cover)",
                index + 1,
                a.excluded_lines,
                b.excluded_lines
            ));
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

/// One piece of painted text reduced to the box `pdftotext -bbox` reports per
/// word: pen extents horizontally, face ascent/descent about the baseline
/// vertically, in page-local twips.
#[derive(Clone, Copy, Debug)]
struct TextBox {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    /// Whether this text shaped with a face the oracle container also has, so its
    /// advances are pinned to the same metrics on both sides.
    pinned: bool,
}

/// Whether `font` is one of the bundled **metric-compatible** faces the oracle
/// container installs (Liberation Sans/Serif/Mono, Carlito, Caladea).
///
/// Roboto is bundled but deliberately excluded: it is our native *default*, not a
/// substitute for anything LibreOffice would pick, so a run that fell back to it
/// has no font parity either. A dynamic (interned fallback / host / system) face is
/// outside every bundled block and so is excluded by construction.
fn is_pinned_face(font: FontId) -> bool {
    [
        &CALADEA,
        &CARLITO,
        &LIBERATION_SANS,
        &LIBERATION_SERIF,
        &LIBERATION_MONO,
    ]
    .iter()
    .any(|family| family.contains(font))
}

/// Reduces one composed glyph run to its oracle-comparable box, or `None` when it
/// contributes no word to the PDF text layer — a run of pure whitespace, since
/// poppler emits *words* and a word carries no leading or trailing spaces.
///
/// A run with no face metrics cannot be measured vertically, so it is kept but
/// marked unpinned: that excludes its line (and shows up in the excluded-line
/// count) instead of silently vanishing from a region the oracle did measure.
fn run_text_box(run: &GlyphRun) -> Option<TextBox> {
    let has_metrics = run.ascent.raw() != 0 || run.descent.raw() != 0;
    let mut pen = run.origin.x.raw();
    let mut x0 = None;
    let mut x1 = pen;
    for glyph in &run.glyphs {
        if !glyph.is_whitespace {
            x0.get_or_insert(pen);
            x1 = pen + glyph.advance.raw();
        }
        pen += glyph.advance.raw();
    }
    Some(TextBox {
        x0: x0?,
        y0: run.origin.y.raw() - run.ascent.raw(),
        x1,
        y1: run.origin.y.raw() + run.descent.raw(),
        pinned: has_metrics && is_pinned_face(run.font),
    })
}

/// Groups text boxes into lines by vertical overlap, the same rule the extraction
/// script applies to the oracle's word boxes: sort by top edge, then start a new
/// line whenever a box begins at or below the running bottom of the current one.
///
/// Mixed font sizes on one line still overlap vertically, so they group together.
/// Side-by-side content (table cells, columns) can merge into one band — that is
/// deliberately conservative: it can only *widen* what a single unshapeable run
/// excludes, never narrow it, and the excluded-line count is compared so an
/// asymmetry between the two sides is reported rather than absorbed.
fn group_into_lines(mut boxes: Vec<TextBox>) -> Vec<Vec<TextBox>> {
    boxes.sort_by_key(|b| (b.y0, b.x0));
    let mut lines: Vec<Vec<TextBox>> = Vec::new();
    let mut bottom = i32::MIN;
    for text_box in boxes {
        match lines.last_mut() {
            Some(line) if text_box.y0 < bottom => {
                bottom = bottom.max(text_box.y1);
                line.push(text_box);
            }
            _ => {
                bottom = text_box.y1;
                lines.push(vec![text_box]);
            }
        }
    }
    lines
}

/// Reduces one laid-out page to the oracle-comparable shape.
///
/// The text is taken from [`compose_page`] — the same display list the renderer
/// paints — so body text, running headers/footers, footnote bodies, table cells,
/// inline and floating text boxes are all already flattened into absolute
/// page-local coordinates by the one implementation that owns those transforms.
fn page_geometry(page: &Page) -> PageGeom {
    let display = compose_page(page);
    let boxes = display
        .items
        .iter()
        .filter_map(|item| match item {
            PaintItem::Glyphs { run } => run_text_box(run),
            _ => None,
        })
        .collect();
    let mut content_bbox: Option<[i32; 4]> = None;
    let mut excluded_lines = 0;
    for line in group_into_lines(boxes) {
        if line.iter().any(|text_box| !text_box.pinned) {
            excluded_lines += 1;
            continue;
        }
        for b in line {
            content_bbox = Some(match content_bbox {
                None => [b.x0, b.y0, b.x1, b.y1],
                Some([x0, y0, x1, y1]) => [x0.min(b.x0), y0.min(b.y0), x1.max(b.x1), y1.max(b.y1)],
            });
        }
    }
    PageGeom {
        size: [page.page_size.width.raw(), page.page_size.height.raw()],
        content_bbox,
        excluded_lines,
    }
}

/// Our page geometry for a `.docx`: import → paginate → reduce each page to the
/// oracle-comparable text region.
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
    layout.pages.iter().map(page_geometry).collect()
}

/// A committed oracle reference: the extraction semantics it was produced with,
/// plus its per-page geometry.
#[derive(Clone, Debug)]
struct OracleReference {
    schema: u64,
    pages: Vec<PageGeom>,
}

/// Reads the committed oracle reference for `fixture_id`, or `None` if the
/// LibreOffice re-bless job has not produced it yet.
fn oracle_reference(fixture_id: &str) -> Option<OracleReference> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/oracle")
        .join(format!("{fixture_id}.geom.json"));
    let text = std::fs::read_to_string(path).ok()?;
    Some(parse_oracle_geometry(&text))
}

/// Parses the oracle geometry JSON (the shape `extract-geometry.sh` emits):
/// `{ "schema": 2, "pages": [ { "sizeTwips": [w,h],
/// "contentBboxTwips": [x0,y0,x1,y1] | null, "excludedLines": n } ] }`.
/// Kept dependency-free (no serde) so the harness stays light; a malformed file
/// is a hard error (a produced reference must be well-formed). Fields added by a
/// later schema are read leniently so an older reference still parses far enough
/// for the page-size comparison to stay live.
fn parse_oracle_geometry(text: &str) -> OracleReference {
    let value: serde_json::Value =
        serde_json::from_str(text).expect("oracle geometry is valid JSON");
    let schema = value["schema"].as_u64().unwrap_or(1);
    let pages = value["pages"]
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
                excluded_lines: page["excludedLines"].as_u64().unwrap_or(0) as u32,
            }
        })
        .collect();
    OracleReference { schema, pages }
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
#[allow(clippy::print_stderr)] // a stale-reference diagnostic, so a skip is not silent
fn our_geometry_matches_the_libreoffice_oracle_within_tolerance() {
    let Some(oracle) = oracle_reference("docx-real-producer-libreoffice") else {
        // No committed reference yet: the pinned-LibreOffice re-bless job
        // (.github/workflows/oracle-geometry.yml) has not run. Inert, not red.
        return;
    };
    let gate = if oracle.schema == ORACLE_SCHEMA {
        ContentGate::Live
    } else {
        eprintln!(
            "oracle reference is at schema {} but this test produces schema {ORACLE_SCHEMA}; \
             comparing page count and size only. Run the 'Oracle geometry re-bless' workflow \
             (.github/workflows/oracle-geometry.yml) to regenerate fixtures/oracle/*.geom.json.",
            oracle.schema
        );
        ContentGate::StaleReference
    };
    let ours = our_geometry(LIBREOFFICE_CORPUS);
    let diffs = geometry_diffs(&ours, &oracle.pages, TOLERANCE_TWIPS, gate);
    assert!(
        diffs.is_empty(),
        "layout geometry diverged from the LibreOffice oracle beyond {TOLERANCE_TWIPS} twips:\n  {}",
        diffs.join("\n  ")
    );
}

#[cfg(test)]
mod tests {
    use casual_doc_layout::text::{Decoration, Glyph};
    use casual_doc_layout::units::{Point, Twip};

    use super::*;

    fn page(w: i32, h: i32, bbox: Option<[i32; 4]>) -> PageGeom {
        PageGeom {
            size: [w, h],
            content_bbox: bbox,
            excluded_lines: 0,
        }
    }

    fn text_box(x0: i32, y0: i32, x1: i32, y1: i32, pinned: bool) -> TextBox {
        TextBox {
            x0,
            y0,
            x1,
            y1,
            pinned,
        }
    }

    #[test]
    fn identical_geometry_has_no_diffs() {
        let g = vec![page(12240, 15840, Some([1440, 1440, 10800, 14400]))];
        assert!(geometry_diffs(&g, &g, TOLERANCE_TWIPS, ContentGate::Live).is_empty());
    }

    #[test]
    fn within_tolerance_is_accepted_but_beyond_is_reported() {
        let ours = vec![page(12240, 15840, Some([1440, 1440, 10800, 14400]))];
        // Every edge nudged by 30 twips (< 40 tolerance): still a match.
        let close = vec![page(12240, 15840, Some([1470, 1410, 10770, 14430]))];
        assert!(geometry_diffs(&ours, &close, TOLERANCE_TWIPS, ContentGate::Live).is_empty());
        // One edge pushed 200 twips out: reported.
        let far = vec![page(12240, 15840, Some([1440, 1440, 11000, 14400]))];
        let diffs = geometry_diffs(&ours, &far, TOLERANCE_TWIPS, ContentGate::Live);
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].contains("content x1"), "got {diffs:?}");
    }

    #[test]
    fn page_count_and_size_mismatches_are_reported() {
        let one = vec![page(12240, 15840, None)];
        let two = vec![page(12240, 15840, None), page(12240, 15840, None)];
        assert_eq!(
            geometry_diffs(&one, &two, TOLERANCE_TWIPS, ContentGate::Live).len(),
            1
        );

        let wide = vec![page(15840, 15840, None)];
        let diffs = geometry_diffs(&one, &wide, TOLERANCE_TWIPS, ContentGate::Live);
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].contains("width"), "got {diffs:?}");
    }

    #[test]
    fn a_different_font_parity_line_count_is_reported() {
        let ours = vec![page(12240, 15840, Some([1440, 1440, 10800, 14400]))];
        let mut oracle = ours.clone();
        oracle[0].excluded_lines = 1;
        let diffs = geometry_diffs(&ours, &oracle, TOLERANCE_TWIPS, ContentGate::Live);
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].contains("font-parity lines"), "got {diffs:?}");
    }

    #[test]
    fn a_stale_reference_gates_only_page_size_and_count() {
        let ours = vec![page(12240, 15840, Some([1440, 1440, 10800, 14400]))];
        // A schema-1 reference's content bbox measures a different quantity, so
        // comparing it would be meaningless — but the page box still must match.
        let mut stale = vec![page(12240, 15840, Some([0, 0, 12240, 15840]))];
        stale[0].excluded_lines = 7;
        assert!(
            geometry_diffs(&ours, &stale, TOLERANCE_TWIPS, ContentGate::StaleReference).is_empty()
        );
        stale[0].size = [15840, 15840];
        let diffs = geometry_diffs(&ours, &stale, TOLERANCE_TWIPS, ContentGate::StaleReference);
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].contains("width"), "got {diffs:?}");
    }

    #[test]
    fn a_schema_stamp_is_read_and_defaults_to_the_first_schema() {
        let v2 = parse_oracle_geometry(
            r#"{"schema":2,"pages":[{"sizeTwips":[1,2],"contentBboxTwips":null,"excludedLines":3}]}"#,
        );
        assert_eq!(v2.schema, 2);
        assert_eq!(v2.pages[0].excluded_lines, 3);
        // A reference written before the stamp existed is schema 1.
        let v1 =
            parse_oracle_geometry(r#"{"pages":[{"sizeTwips":[1,2],"contentBboxTwips":null}]}"#);
        assert_eq!(v1.schema, 1);
        assert_eq!(v1.pages[0].excluded_lines, 0);
    }

    #[test]
    fn boxes_group_into_lines_by_vertical_overlap() {
        // Two boxes sharing a band (different sizes, so different tops) plus one
        // clearly below: two lines.
        let lines = group_into_lines(vec![
            text_box(0, 100, 50, 200, true),
            text_box(60, 120, 90, 190, true),
            text_box(0, 400, 50, 500, true),
        ]);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), 2);
        assert_eq!(lines[1].len(), 1);
    }

    /// A glyph run on a baseline at `(100, 1000)` whose glyphs are `(advance,
    /// is_whitespace)` pairs, with 200/50 twips of ascent/descent.
    fn run_of(glyphs: &[(i32, bool)]) -> GlyphRun {
        GlyphRun {
            font: LIBERATION_SERIF.face_id(false, false),
            size: Twip(240),
            ascent: Twip(200),
            descent: Twip(50),
            character_scale_percent: 100,
            color: [0, 0, 0, 255],
            origin: Point::new(Twip(100), Twip(1000)),
            bidi_level: 0,
            decoration: Decoration::default(),
            highlight: None,
            shading: None,
            glyphs: glyphs
                .iter()
                .enumerate()
                .map(|(index, &(advance, is_whitespace))| Glyph {
                    id: 1,
                    advance: Twip(advance),
                    cluster: index as u32,
                    is_whitespace,
                })
                .collect(),
            is_marker: false,
            is_leader: false,
        }
    }

    #[test]
    fn a_word_box_spans_the_non_whitespace_glyphs_and_the_face_metrics() {
        // Mirrors poppler: a word starts at the first non-space glyph's pen
        // position and ends at the last one's pen end, and is as tall as the
        // face's ascent+descent about the baseline — never the ink.
        let measured = run_text_box(&run_of(&[(30, true), (60, false), (40, false), (30, true)]))
            .expect("the run carries a word");
        assert_eq!(
            [measured.x0, measured.y0, measured.x1, measured.y1],
            [130, 800, 230, 1050]
        );
        assert!(measured.pinned);
        // Pure whitespace contributes no word at all.
        assert!(run_text_box(&run_of(&[(30, true), (30, true)])).is_none());
        // A run with no face metrics cannot be measured vertically, so it is
        // reported as unpinned (excluding and counting its line) rather than
        // dropped from a region the oracle did measure.
        let mut metricless = run_of(&[(60, false)]);
        metricless.ascent = Twip::ZERO;
        metricless.descent = Twip::ZERO;
        assert!(
            !run_text_box(&metricless)
                .expect("the run still carries a word")
                .pinned
        );
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
    fn our_reduction_measures_text_not_block_boxes() {
        // The defect this gate caught: reducing to `placed[].rect` reports the
        // paragraph *boxes* — full column width, and `w:spacing` before/after
        // included — where the oracle reports the text. Both properties are
        // asserted structurally against the page's own content area, so a revert
        // to block rects goes red without pinning our numbers (that is H1's job).
        let mut package = DocxPackage::open(LIBREOFFICE_CORPUS, PackageLimits::default()).unwrap();
        let document = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        let layout = paginate_document(&document, &ParleyShaper::new());
        let page = &layout.pages[0];
        let area = page.content_area;
        let bbox = page_geometry(page)
            .content_bbox
            .expect("the corpus page carries comparable text");
        assert!(
            bbox[2] < area.origin.x.raw() + area.size.width.raw(),
            "x1 {} must be the longest line's pen end, not the column's right edge",
            bbox[2]
        );
        assert!(
            bbox[1] > area.origin.y.raw(),
            "y0 {} must be the first baseline's ascent line, not the block top \
             (the heading carries w:spacing@before)",
            bbox[1]
        );
    }

    #[test]
    fn the_corpus_fixtures_unshapeable_line_is_excluded() {
        // The fixture's last paragraph mixes Latin with CJK and Arabic, which no
        // bundled metric-compatible face covers; that line is not oracle-comparable
        // and must be dropped (and counted) rather than compared.
        let geom = our_geometry(LIBREOFFICE_CORPUS);
        assert_eq!(
            geom[0].excluded_lines, 1,
            "exactly the CJK/Arabic line is outside the pinned faces' coverage"
        );
    }

    #[test]
    fn only_metric_compatible_bundled_faces_count_as_pinned() {
        assert!(is_pinned_face(LIBERATION_SERIF.face_id(false, false)));
        assert!(is_pinned_face(LIBERATION_SANS.face_id(true, false)));
        assert!(is_pinned_face(CARLITO.face_id(false, true)));
        assert!(is_pinned_face(CALADEA.face_id(true, true)));
        assert!(is_pinned_face(LIBERATION_MONO.face_id(false, false)));
        // Roboto is bundled but substitutes for nothing LibreOffice would choose.
        assert!(!is_pinned_face(
            casual_doc_layout::fonts::ROBOTO.face_id(false, false)
        ));
        // A dynamically interned fallback face is outside every bundled block.
        assert!(!is_pinned_face(FontId(
            casual_doc_layout::font_registry::DYNAMIC_FONT_BASE
        )));
    }
}
