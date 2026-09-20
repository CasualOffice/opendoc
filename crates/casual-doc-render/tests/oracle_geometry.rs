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
//! point, we by resolved face. The **vertical extent** the dropped lines occupy is
//! itself compared, within the same tolerance as every edge, so the filter cannot
//! silently swallow a line the oracle still measured.
//!
//! The extent rather than the line *count* is gated, and that is a measured
//! decision. Line grouping is per-renderer: on `real-producer-table-list`
//! LibreOffice's two bullet lines sit 0.8pt apart and group as two, while our list
//! marker's box is tall enough to overlap the following line, so the same two lines
//! group as one. The counts then read 1 vs 2 with no fidelity difference behind
//! them, whereas the extents are 557 and 571 twips — a 14-twip agreement. The
//! extent still moves by a whole line height (≈276 twips at body size) the moment
//! an exclusion eats content the other side measured, which is the property the
//! filter needs. `excludedLines` stays in the reference as review context and is
//! not compared.
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
//! # Known divergences are registered, not absorbed
//!
//! Three fixtures are genuinely out of tolerance against LibreOffice, and the
//! answer is neither to raise [`TOLERANCE_TWIPS`] nor to edit the reference: each
//! is listed in [`KNOWN_DIVERGENCES`] with its measured delta and the tracker row
//! that owns it. A registered edge is compared against `oracle + delta` within the
//! *same* 40-twip band, so the gate still fails if that edge moves at all — in
//! either direction, a fix included, which is what forces the entry to be revisited
//! rather than to rot. `known_divergences_are_still_divergent` additionally refuses
//! an entry small enough that the plain comparison would have passed, so the
//! registry cannot grow into a general slack allowance.
//!
//! # Blessing protocol
//!
//! The reference files are produced by running `scripts/oracle/extract-geometry.sh`
//! under a pinned LibreOffice ([`ORACLE_LIBREOFFICE_VERSION`]) — in CI by the
//! `.github/workflows/oracle-geometry.yml` re-bless job, which is deliberately not
//! part of the hermetic main CI. A fixture with no committed reference is
//! **skipped**, and one whose reference predates the current extraction semantics
//! ([`ORACLE_SCHEMA`]) is compared only on page count and page size — the two
//! quantities no schema bump has changed — until the re-bless job regenerates it.
//! Never hand-edit a blessed reference.
//!
//! `the_oracle_gate_is_armed` asserts that every fixture in [`ORACLE_FIXTURES`]
//! *does* have a trustworthy, current-schema reference, so the skip path can no
//! longer quietly become the normal path: deleting, staling or voiding a reference
//! fails the build instead of turning the gate back off. That is the docs/94 and
//! skill §9 rule — prose may only describe this as a CI gate while a test proves
//! it is armed — and it is the whole content of backlog row FID-P-01, which
//! existed because the workflow was written, merged, described as protecting
//! rendering fidelity, and never once run.

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
/// region of the font-parity lines only; schema 3 gates the excluded lines'
/// vertical *extent* instead of their grouping-sensitive count, and records the
/// faces the producing PDF actually embedded.
const ORACLE_SCHEMA: u64 = 3;

/// One page's oracle-comparable geometry, in twips: the page size, the bounding
/// box of its comparable text (`[x0, y0, x1, y1]`), and the vertical extent of the
/// lines dropped as unshapeable by the pinned faces. Page-level rather than
/// per-block so it survives the absence of a stable block correspondence between
/// the two renderers while still catching gross placement/extent errors.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PageGeom {
    size: [i32; 2],
    content_bbox: Option<[i32; 4]>,
    excluded_extent: i32,
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

/// The content-bbox edges, in the order they are stored.
const EDGES: [&str; 4] = ["x0", "y0", "x1", "y1"];

/// One content-bbox edge on which our layout is known to disagree with the oracle
/// by more than [`TOLERANCE_TWIPS`], recorded so the rest of that fixture can
/// still be gated.
///
/// This is **not** a tolerance. `delta` re-centres the comparison for exactly one
/// edge of one page of one fixture; the 40-twip band around it is unchanged, so
/// the edge is still pinned to ±2pt and any movement — including the divergence
/// being fixed — fails the gate and forces the entry to be re-reviewed.
#[derive(Clone, Copy, Debug)]
struct Divergence {
    fixture: &'static str,
    page: usize,
    edge: &'static str,
    /// `ours − oracle`, in twips, as measured when the entry was added.
    delta: i32,
    /// The tracker row that owns closing it.
    row: &'static str,
    /// What is actually different, in one line.
    reason: &'static str,
}

/// Every edge where our engine is knowingly out of tolerance against LibreOffice
/// 26.2.4.2 on the committed references.
///
/// These are engine findings, not harness noise: all three are the *bottom* edge
/// of the page's text, i.e. accumulated vertical drift through tables and lists,
/// and each was measured with both sides reducing the identical quantity over
/// pinned-parity faces. They are registered rather than tolerated so that arming
/// the gate does not require either inflating [`TOLERANCE_TWIPS`] (which would
/// blind the other twenty-one comparisons) or leaving the whole gate inert (which
/// is the state FID-P-01 exists to end).
///
/// Removing an entry is the goal. Do not add one without a measured delta, a
/// tracker row, and a sentence saying what diverges.
const KNOWN_DIVERGENCES: &[Divergence] = &[
    Divergence {
        fixture: "docx-real-producer-rich",
        page: 1,
        edge: "y1",
        delta: 263,
        row: "FID-L-21",
        reason: "the paragraph after the nested table sits one line lower than \
                 LibreOffice puts it, so the page's last baseline is ~1 line down",
    },
    Divergence {
        fixture: "docx-real-producer-table-merges",
        page: 1,
        edge: "y1",
        delta: -55,
        row: "FID-L-21",
        reason: "merged-cell row heights accumulate ~55 twips short of \
                 LibreOffice's by the paragraph below the table",
    },
    Divergence {
        fixture: "docx-real-producer-table-list",
        page: 1,
        edge: "y1",
        delta: -60,
        row: "FID-L-21 (second instance, found by arming this gate)",
        reason: "vertical drift accumulates monotonically down the page — table \
                 rows −10 then −25 twips, list items −65 — leaving the closing \
                 paragraph 60 twips above LibreOffice's",
    },
];

/// The registered divergences that apply to `fixture_id`.
fn divergences_for(fixture_id: &str) -> Vec<Divergence> {
    KNOWN_DIVERGENCES
        .iter()
        .filter(|d| d.fixture == fixture_id)
        .copied()
        .collect()
}

/// Every human-readable geometry discrepancy between `ours` and the `oracle`
/// reference beyond `tolerance`, after shifting each edge listed in
/// `divergences` by its registered delta. Empty ⇒ the two agree.
fn geometry_diffs(
    ours: &[PageGeom],
    oracle: &[PageGeom],
    tolerance: i32,
    gate: ContentGate,
    divergences: &[Divergence],
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
        if (a.excluded_extent - b.excluded_extent).abs() > tolerance {
            diffs.push(format!(
                "page {}: font-parity excluded extent ours={} oracle={} \
                 (the two renderers disagree about which text the pinned faces cover)",
                index + 1,
                a.excluded_extent,
                b.excluded_extent
            ));
        }
        match (a.content_bbox, b.content_bbox) {
            (Some(a_box), Some(b_box)) => {
                for (edge, (av, bv)) in a_box.iter().zip(&b_box).enumerate() {
                    let registered = divergences
                        .iter()
                        .find(|d| d.page == index + 1 && d.edge == EDGES[edge]);
                    let expected = bv + registered.map_or(0, |d| d.delta);
                    if (av - expected).abs() > tolerance {
                        let known = registered.map_or(String::new(), |d| {
                            format!(
                                " (registered divergence {:+} from {}, expected {expected}; \
                                 see KNOWN_DIVERGENCES)",
                                d.delta, d.row
                            )
                        });
                        diffs.push(format!(
                            "page {}: content {} ours={av} oracle={bv}{known}",
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
    let mut excluded_extent = 0;
    for line in group_into_lines(boxes) {
        if line.iter().any(|text_box| !text_box.pinned) {
            // The band the dropped line occupies, summed rather than counted:
            // grouping is per-renderer, the extent is not. See the module docs.
            let top = line.iter().map(|b| b.y0).min().unwrap_or(0);
            let bottom = line.iter().map(|b| b.y1).max().unwrap_or(0);
            excluded_extent += bottom - top;
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
        excluded_extent,
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
/// the faces the producing PDF embedded, plus its per-page geometry.
#[derive(Clone, Debug)]
struct OracleReference {
    schema: u64,
    fonts: Vec<String>,
    pages: Vec<PageGeom>,
}

/// The LibreOffice build a reference must have been produced by.
///
/// This is not decoration. `ubuntu-24.04` ships LibreOffice 24.2.7.2, and
/// LibreOffice's own text layout moved between that and 26.2.4.2 by **558-629
/// twips** on three of this corpus's fixtures — an order of magnitude more than
/// [`TOLERANCE_TWIPS`]. Measured directly: on `real-producer-hyperlinks` our
/// right edge is 4869, LibreOffice 26.2.4.2 says 4867 (2 twips, inside
/// tolerance), and 24.2.7.2 says 5468.
///
/// So "pinned LibreOffice" cannot mean "whatever the runner image happens to
/// ship" — at this tolerance that is not an oracle, it is a moving target. The
/// re-bless job records the build it used into `fixtures/oracle/.toolchain`, and
/// a reference produced by a different one is refused below rather than compared.
///
/// Raising this constant is a deliberate act: it means re-blessing every
/// reference against the new build and reviewing the geometry diff.
const ORACLE_LIBREOFFICE_VERSION: &str = "26.2.4.2";

/// Reads the committed oracle reference for `fixture_id`, or `None` if there is
/// no **trustworthy** one yet.
///
/// Three things must hold, and each has already been violated once:
///
/// 1. The reference exists (the re-bless job has run).
/// 2. Every face the producing PDF embedded is one this comparison can stand
///    behind — see [`embedded_fonts_are_pinned`]. A reference shaped with a
///    substitute measures the producing machine's font configuration rather than
///    our fidelity.
/// 3. `fixtures/oracle/.toolchain` names [`ORACLE_LIBREOFFICE_VERSION`]. See that
///    constant for why a version mismatch is worse than no reference.
///
/// An unverified reference is worse than none, because it fails for reasons that
/// have nothing to do with the engine and trains everyone to ignore the gate.
fn oracle_reference(fixture_id: &str) -> Option<OracleReference> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/oracle");

    let toolchain = std::fs::read_to_string(dir.join(".toolchain")).ok()?;
    if !toolchain_matches_the_pin(&toolchain) {
        return None;
    }

    let text = std::fs::read_to_string(dir.join(format!("{fixture_id}.geom.json"))).ok()?;
    let reference = parse_oracle_geometry(&text);
    // A pre-schema-3 reference carries no font record; it is still read, because
    // a stale reference is compared on page size only and that comparison does
    // not rest on font parity.
    if reference.schema >= 3 && !embedded_fonts_are_pinned(&reference.fonts) {
        return None;
    }
    Some(reference)
}

/// Faces that may legitimately appear in an oracle PDF without being one of the
/// pinned metric-compatible families.
///
/// `Symbol` is LibreOffice's list-bullet face. Its glyphs live in the private-use
/// area (`U+F0B7`), which the extraction script's coverage ranges exclude and our
/// side excludes by resolved face, so every line it touches is dropped from the
/// comparison on both sides before any advance is compared. Nothing else belongs
/// here: adding a face means asserting that its text is either metric-identical
/// to ours or excluded on both sides, and that claim needs review.
const ORACLE_NON_PARITY_FONTS: &[&str] = &["Symbol"];

/// Whether every face the producing PDF embedded is one the comparison can stand
/// behind: a pinned metric-compatible family, or a reviewed non-parity face whose
/// lines both sides exclude.
///
/// This reads provenance off the **artifact** rather than off the producing
/// environment, which is the point. The previous check parsed an `fc-match`
/// transcript, which (a) only describes the machine, not the document, and (b) is
/// meaningless on macOS, where LibreOffice ships and uses its own bundled copies
/// of exactly these families and fontconfig answers for the system's fonts
/// instead. A reference with no fonts recorded proves nothing and is not trusted.
fn embedded_fonts_are_pinned(fonts: &[String]) -> bool {
    /// PostScript-style names, as `pdffonts` prints them with the subset prefix
    /// stripped: the family with spaces removed, optionally `-<style>`.
    const PINNED_PREFIXES: [&str; 5] = [
        "LiberationSans",
        "LiberationSerif",
        "LiberationMono",
        "Carlito",
        "Caladea",
    ];
    if fonts.is_empty() {
        return false;
    }
    let mut pinned_seen = false;
    for font in fonts {
        let base = font.split('-').next().unwrap_or(font);
        if PINNED_PREFIXES.contains(&base) {
            pinned_seen = true;
        } else if !ORACLE_NON_PARITY_FONTS.contains(&base) {
            return false;
        }
    }
    pinned_seen
}

/// Whether the recorded `soffice --version` output names the pinned build.
///
/// Substring rather than equality: the real output carries a build hash and a
/// build id after the version (`LibreOffice 26.2.4.2 0229ac93...`), and neither
/// is stable enough to assert.
fn toolchain_matches_the_pin(toolchain: &str) -> bool {
    toolchain.contains(ORACLE_LIBREOFFICE_VERSION)
}

/// Parses the oracle geometry JSON (the shape `extract-geometry.sh` emits):
/// `{ "schema": 3, "fonts": [..], "pages": [ { "sizeTwips": [w,h],
/// "contentBboxTwips": [x0,y0,x1,y1] | null, "excludedLines": n,
/// "excludedExtentTwips": n } ] }`.
/// Kept dependency-free (no serde) so the harness stays light; a malformed file
/// is a hard error (a produced reference must be well-formed). Fields added by a
/// later schema are read leniently so an older reference still parses far enough
/// for the page-size comparison to stay live.
fn parse_oracle_geometry(text: &str) -> OracleReference {
    let value: serde_json::Value =
        serde_json::from_str(text).expect("oracle geometry is valid JSON");
    let schema = value["schema"].as_u64().unwrap_or(1);
    let fonts = value["fonts"]
        .as_array()
        .map(|fonts| {
            fonts
                .iter()
                .map(|font| font.as_str().expect("font names are strings").to_owned())
                .collect()
        })
        .unwrap_or_default();
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
                excluded_extent: page["excludedExtentTwips"].as_i64().unwrap_or(0) as i32,
            }
        })
        .collect();
    OracleReference {
        schema,
        fonts,
        pages,
    }
}

/// The fixtures this gate compares, as `(reference id, bytes)`.
///
/// **Every one is Latin-only, and that is a requirement, not a coincidence.**
///
/// H2's determinism rests on both renderers shaping with the same
/// metric-compatible faces, which only holds for code points those faces cover.
/// The line-level coverage filter (see `excluded_lines`) was built to drop
/// uncoverable text — and it is not sufficient on its own, which we learned by
/// running it: substituted text of a *different width* changes where the rest of
/// the paragraph WRAPS, so the lines after it differ too even when every word in
/// them is Latin. Excluding the offending line still left a downstream all-Latin
/// line 1014 twips wider in the oracle than in ours.
///
/// So `real-producer-libreoffice.docx` — the only corpus fixture containing CJK
/// and Arabic (`日本語`, `العربية`) — is deliberately **not** in this table. It is
/// not an oracle-comparable document, and pointing the gate at it measured which
/// fonts each environment happened to substitute rather than our fidelity. The
/// six Latin-only LibreOffice-produced fixtures below give H2 materially better
/// coverage than that one file did: footnotes, running content, hyperlinks, rich
/// formatting, lists in tables, and merged cells.
///
/// The coverage filter stays regardless: it is the guard that makes this
/// requirement enforced rather than merely documented, and it fails loudly if
/// uncoverable text ever appears in one of these fixtures.
///
/// Keep this list and the `FIXTURES` map in
/// `.github/workflows/oracle-geometry.yml` in step; a reference is only produced
/// for a fixture named there.
const ORACLE_FIXTURES: &[(&str, &[u8])] = &[
    (
        "docx-real-producer-footnotes",
        include_bytes!("../../../fixtures/corpus/real-producer-footnotes.docx"),
    ),
    (
        "docx-real-producer-header-footer",
        include_bytes!("../../../fixtures/corpus/real-producer-header-footer.docx"),
    ),
    (
        "docx-real-producer-hyperlinks",
        include_bytes!("../../../fixtures/corpus/real-producer-hyperlinks.docx"),
    ),
    (
        "docx-real-producer-rich",
        include_bytes!("../../../fixtures/corpus/real-producer-rich.docx"),
    ),
    (
        "docx-real-producer-table-list",
        include_bytes!("../../../fixtures/corpus/real-producer-table-list.docx"),
    ),
    (
        "docx-real-producer-table-merges",
        include_bytes!("../../../fixtures/corpus/real-producer-table-merges.docx"),
    ),
];

/// The mixed-script fixture, kept for the non-oracle tests below (extraction,
/// reduction semantics, the coverage filter) which do not compare against an
/// oracle and positively benefit from exercising substituted text.
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
    let mut compared = 0_usize;
    let mut failures: Vec<String> = Vec::new();

    for (fixture_id, bytes) in ORACLE_FIXTURES {
        let Some(oracle) = oracle_reference(fixture_id) else {
            // No committed reference for this fixture yet: the pinned-LibreOffice
            // re-bless job has not produced one. Inert, not red.
            continue;
        };
        let gate = if oracle.schema == ORACLE_SCHEMA {
            ContentGate::Live
        } else {
            eprintln!(
                "{fixture_id}: oracle reference is at schema {} but this test produces schema \
                 {ORACLE_SCHEMA}; comparing page count and size only. Run the 'Oracle geometry \
                 re-bless' workflow (.github/workflows/oracle-geometry.yml) to regenerate \
                 fixtures/oracle/*.geom.json.",
                oracle.schema
            );
            ContentGate::StaleReference
        };
        compared += 1;
        let ours = our_geometry(bytes);
        let divergences = divergences_for(fixture_id);
        for diff in geometry_diffs(&ours, &oracle.pages, TOLERANCE_TWIPS, gate, &divergences) {
            failures.push(format!("{fixture_id}: {diff}"));
        }
    }

    assert!(
        failures.is_empty(),
        "layout geometry diverged from the LibreOffice oracle beyond {TOLERANCE_TWIPS} twips \
         across {compared} fixture(s):\n  {}",
        failures.join("\n  ")
    );
}

/// The gate is **armed**: every fixture has a trustworthy, current-schema
/// reference committed, and CI runs this comparison on every pull request.
///
/// docs/94 made the content comparison skip when a reference is missing, so the
/// harness could land before the oracle had ever run. That is the right default
/// and it is also exactly how a gate stays inert for months while prose calls it
/// a protection (skill §9 rule 2; backlog row FID-P-01). This test is the
/// counterweight: once references exist, removing, staling or voiding one fails
/// the build rather than silently switching the gate off.
///
/// It deliberately does *not* skip on Windows. The geometry comparison does — our
/// text stack shapes differently there — but whether the references are committed
/// is a property of the repository, not of the platform reading it.
#[test]
fn the_oracle_gate_is_armed() {
    let missing: Vec<&str> = ORACLE_FIXTURES
        .iter()
        .filter(|(id, _)| !matches!(oracle_reference(id), Some(r) if r.schema == ORACLE_SCHEMA))
        .map(|(id, _)| *id)
        .collect();
    assert!(
        missing.is_empty(),
        "the oracle geometry gate is DISARMED for {}: no trustworthy schema-{ORACLE_SCHEMA} \
         reference under fixtures/oracle/. Re-bless with \
         .github/workflows/oracle-geometry.yml (or scripts/oracle/extract-geometry.sh under \
         LibreOffice {ORACLE_LIBREOFFICE_VERSION}) — do not delete the fixture to get green.",
        missing.join(", ")
    );

    // …and CI has to run it on pull requests, not only via workflow_dispatch.
    // `include_str!` returns CRLF on a Windows checkout, so normalise before
    // matching: a previous source-scanning guard in this repository passed
    // everywhere and failed only on Windows for exactly this reason.
    let ci = include_str!("../../../.github/workflows/ci.yml").replace('\r', "");
    for expected in [
        "pull_request:",
        "--test oracle_geometry",
        "Oracle geometry gate",
    ] {
        assert!(
            ci.contains(expected),
            ".github/workflows/ci.yml no longer contains {expected:?}; the oracle geometry \
             gate must run in CI on pull requests, not only on manual dispatch"
        );
    }
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
            excluded_extent: 0,
        }
    }

    /// No registered divergence: the plain comparison.
    const PLAIN: &[Divergence] = &[];

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
        assert!(geometry_diffs(&g, &g, TOLERANCE_TWIPS, ContentGate::Live, PLAIN).is_empty());
    }

    #[test]
    fn within_tolerance_is_accepted_but_beyond_is_reported() {
        let ours = vec![page(12240, 15840, Some([1440, 1440, 10800, 14400]))];
        // Every edge nudged by 30 twips (< 40 tolerance): still a match.
        let close = vec![page(12240, 15840, Some([1470, 1410, 10770, 14430]))];
        assert!(
            geometry_diffs(&ours, &close, TOLERANCE_TWIPS, ContentGate::Live, PLAIN).is_empty()
        );
        // One edge pushed 200 twips out: reported.
        let far = vec![page(12240, 15840, Some([1440, 1440, 11000, 14400]))];
        let diffs = geometry_diffs(&ours, &far, TOLERANCE_TWIPS, ContentGate::Live, PLAIN);
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].contains("content x1"), "got {diffs:?}");
    }

    #[test]
    fn page_count_and_size_mismatches_are_reported() {
        let one = vec![page(12240, 15840, None)];
        let two = vec![page(12240, 15840, None), page(12240, 15840, None)];
        assert_eq!(
            geometry_diffs(&one, &two, TOLERANCE_TWIPS, ContentGate::Live, PLAIN).len(),
            1
        );

        let wide = vec![page(15840, 15840, None)];
        let diffs = geometry_diffs(&one, &wide, TOLERANCE_TWIPS, ContentGate::Live, PLAIN);
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].contains("width"), "got {diffs:?}");
    }

    #[test]
    fn a_different_font_parity_excluded_extent_is_reported() {
        let ours = vec![page(12240, 15840, Some([1440, 1440, 10800, 14400]))];
        let mut oracle = ours.clone();
        // Grouping noise (the bullet-line case: 557 vs 571) stays inside the band…
        oracle[0].excluded_extent = 14;
        assert!(
            geometry_diffs(&ours, &oracle, TOLERANCE_TWIPS, ContentGate::Live, PLAIN).is_empty()
        );
        // …a whole excluded body line does not.
        oracle[0].excluded_extent = 276;
        let diffs = geometry_diffs(&ours, &oracle, TOLERANCE_TWIPS, ContentGate::Live, PLAIN);
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].contains("excluded extent"), "got {diffs:?}");
    }

    #[test]
    fn a_registered_divergence_recentres_one_edge_without_widening_the_band() {
        let registered: &[Divergence] = &[Divergence {
            fixture: "f",
            page: 1,
            edge: "y1",
            delta: 263,
            row: "FID-L-21",
            reason: "test",
        }];
        let oracle = vec![page(12240, 15840, Some([1440, 1440, 10800, 3501]))];
        // Exactly the registered divergence: accepted.
        let ours = vec![page(12240, 15840, Some([1440, 1440, 10800, 3764]))];
        assert!(
            geometry_diffs(
                &ours,
                &oracle,
                TOLERANCE_TWIPS,
                ContentGate::Live,
                registered
            )
            .is_empty()
        );
        // Unregistered (the raw comparison) the same numbers are a failure, so the
        // entry is load-bearing rather than decorative.
        assert_eq!(
            geometry_diffs(&ours, &oracle, TOLERANCE_TWIPS, ContentGate::Live, PLAIN).len(),
            1
        );
        // The divergence getting WORSE is still caught…
        let worse = vec![page(12240, 15840, Some([1440, 1440, 10800, 3900]))];
        assert_eq!(
            geometry_diffs(
                &worse,
                &oracle,
                TOLERANCE_TWIPS,
                ContentGate::Live,
                registered
            )
            .len(),
            1
        );
        // …and so is it being FIXED, which is the point: the entry must then be
        // deleted deliberately, not left to rot.
        let fixed = vec![page(12240, 15840, Some([1440, 1440, 10800, 3501]))];
        let diffs = geometry_diffs(
            &fixed,
            &oracle,
            TOLERANCE_TWIPS,
            ContentGate::Live,
            registered,
        );
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].contains("registered divergence"), "got {diffs:?}");
        // Only the registered edge moves; every other edge keeps the plain band.
        let other_edge = vec![page(12240, 15840, Some([1440, 1703, 10800, 3764]))];
        assert_eq!(
            geometry_diffs(
                &other_edge,
                &oracle,
                TOLERANCE_TWIPS,
                ContentGate::Live,
                registered
            )
            .len(),
            1
        );
    }

    #[test]
    fn known_divergences_are_still_divergent() {
        // An entry whose delta the plain band would have absorbed is stale slack,
        // not a finding: it would sit in the registry implying a defect that is
        // no longer there. Every entry must be a real, out-of-band divergence.
        for d in KNOWN_DIVERGENCES {
            assert!(
                d.delta.abs() > TOLERANCE_TWIPS,
                "KNOWN_DIVERGENCES entry {}/{} {} is {} twips, inside the {TOLERANCE_TWIPS}-twip \
                 band — delete it instead of registering it",
                d.fixture,
                d.page,
                d.edge,
                d.delta
            );
            assert!(EDGES.contains(&d.edge), "unknown edge {}", d.edge);
            assert!(!d.row.is_empty() && !d.reason.is_empty());
            assert!(
                ORACLE_FIXTURES.iter().any(|(id, _)| *id == d.fixture),
                "{} is not an oracle fixture",
                d.fixture
            );
        }
    }

    #[test]
    fn a_stale_reference_gates_only_page_size_and_count() {
        let ours = vec![page(12240, 15840, Some([1440, 1440, 10800, 14400]))];
        // A schema-1 reference's content bbox measures a different quantity, so
        // comparing it would be meaningless — but the page box still must match.
        let mut stale = vec![page(12240, 15840, Some([0, 0, 12240, 15840]))];
        stale[0].excluded_extent = 7000;
        assert!(
            geometry_diffs(
                &ours,
                &stale,
                TOLERANCE_TWIPS,
                ContentGate::StaleReference,
                PLAIN
            )
            .is_empty()
        );
        stale[0].size = [15840, 15840];
        let diffs = geometry_diffs(
            &ours,
            &stale,
            TOLERANCE_TWIPS,
            ContentGate::StaleReference,
            PLAIN,
        );
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].contains("width"), "got {diffs:?}");
    }

    #[test]
    fn a_schema_stamp_is_read_and_defaults_to_the_first_schema() {
        let v3 = parse_oracle_geometry(
            r#"{"schema":3,"fonts":["LiberationSerif"],"pages":[{"sizeTwips":[1,2],
               "contentBboxTwips":null,"excludedLines":2,"excludedExtentTwips":571}]}"#,
        );
        assert_eq!(v3.schema, 3);
        assert_eq!(v3.fonts, ["LiberationSerif"]);
        assert_eq!(v3.pages[0].excluded_extent, 571);
        // A reference written before the stamp existed is schema 1, and carries
        // neither font provenance nor an excluded extent.
        let v1 =
            parse_oracle_geometry(r#"{"pages":[{"sizeTwips":[1,2],"contentBboxTwips":null}]}"#);
        assert_eq!(v1.schema, 1);
        assert!(v1.fonts.is_empty());
        assert_eq!(v1.pages[0].excluded_extent, 0);
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
        // and must be dropped (and measured) rather than compared.
        let geom = our_geometry(LIBREOFFICE_CORPUS);
        assert!(
            geom[0].excluded_extent >= 200,
            "the CJK/Arabic line is outside the pinned faces' coverage and must be \
             excluded, but the excluded extent is only {} twips",
            geom[0].excluded_extent
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

/// Guards on reference PROVENANCE — the two ways a reference can look valid and
/// be worthless. Both have happened: a reference was produced with a substituted
/// font, and a reference was produced by a LibreOffice two years older than the
/// one the engine agrees with.
#[cfg(test)]
mod provenance_tests {
    use super::{ORACLE_LIBREOFFICE_VERSION, embedded_fonts_are_pinned, toolchain_matches_the_pin};

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_reference_shaped_only_with_pinned_faces_is_trusted() {
        assert!(embedded_fonts_are_pinned(&names(&[
            "LiberationSans-Bold",
            "LiberationSerif",
        ])));
        assert!(embedded_fonts_are_pinned(&names(&[
            "Caladea-BoldItalic",
            "Carlito",
            "LiberationMono",
        ])));
    }

    #[test]
    fn the_list_bullet_face_is_allowed_because_both_sides_exclude_its_lines() {
        // LibreOffice draws `w:numFmt="bullet"` markers from Symbol at U+F0B7.
        // The extractor's coverage ranges exclude that code point and our side
        // excludes the line by resolved face, so no Symbol advance is ever
        // compared — see ORACLE_NON_PARITY_FONTS.
        assert!(embedded_fonts_are_pinned(&names(&[
            "LiberationSerif",
            "Symbol"
        ])));
    }

    #[test]
    fn a_single_substituted_face_voids_the_whole_reference() {
        // What actually happened: the packages were installed, fontconfig
        // resolved something else, and nothing noticed until the geometry came
        // back ~17% wide. Reading the faces off the PDF catches it whatever the
        // producing machine's font configuration claimed.
        assert!(!embedded_fonts_are_pinned(&names(&[
            "DejaVuSans",
            "LiberationSerif",
        ])));
        assert!(!embedded_fonts_are_pinned(&names(&["TimesNewRomanPSMT"])));
        // Liberation Sans Narrow is a different family, not a narrow style of a
        // pinned one, and is not metric-compatible with anything we bundle.
        assert!(!embedded_fonts_are_pinned(&names(&[
            "LiberationSansNarrow-Bold"
        ])));
    }

    #[test]
    fn missing_font_provenance_is_not_trust() {
        // A reference recording no faces proves nothing about what produced it.
        assert!(!embedded_fonts_are_pinned(&[]));
        // Nor does one recording only the allowed non-parity face: every line it
        // touches is excluded, so there would be nothing left to compare.
        assert!(!embedded_fonts_are_pinned(&names(&["Symbol"])));
    }

    #[test]
    fn the_pinned_libreoffice_build_is_accepted() {
        let recorded = format!("LibreOffice {ORACLE_LIBREOFFICE_VERSION} 0229ac93fcf0d7cbc63\n");
        assert!(toolchain_matches_the_pin(&recorded));
    }

    #[test]
    fn a_different_libreoffice_build_voids_the_reference() {
        // The exact build `ubuntu-24.04` ships, which produced references
        // 558-629 twips wide on three fixtures.
        assert!(!toolchain_matches_the_pin(
            "LibreOffice 24.2.7.2 420(Build:2)\n"
        ));
        assert!(!toolchain_matches_the_pin(""));
    }
}
