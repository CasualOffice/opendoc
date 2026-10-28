//! OCR for raster-only regions and unrecoverable runs, merged under
//! native-first precedence (`docs/182` P5/P6, `docs/131` §10.1-10.2).
//!
//! Per page, in this order:
//!
//! 1. **Classify** from the native evidence alone ([`classify`]): the regions
//!    that need optical recognition are (a) pictures no valid native text
//!    covers and (b) the glyphs of a run whose Unicode is unrecoverable. A page
//!    with none is [`PageClass::Native`] and is never rasterised (PDFR-I2);
//!    with regions and no valid native text it is [`PageClass::Raster`];
//!    otherwise [`PageClass::Hybrid`].
//! 2. **Rasterise selectively**: each region alone, through hayro's renderer
//!    with a transform that maps the region to the raster's origin — the rest
//!    of the page is never drawn.
//! 3. **Recognise** with the host's [`OcrProvider`], under per-raster
//!    [`OcrLimits`] and import-wide page and pixel budgets, polling the host's
//!    [`Interrupt`] (cancel abandons the import, PDFR-I3; a spent time budget
//!    stops OCR and reports what was not read).
//! 4. **Merge**: an optical word that overlaps valid native glyphs is dropped —
//!    native text is never overwritten — and, when the two texts disagree, it
//!    is counted as `pdf.import.native_ocr_conflict`. A word that overlaps only
//!    unmapped glyphs replaces them. A word over pure raster is kept. Words
//!    under [`OcrOptions::low_confidence`] are kept and counted as
//!    `pdf.import.ocr_low_confidence`; under [`OcrOptions::min_confidence`]
//!    they are dropped and counted the same way (PDFR-I4: confidence alone
//!    does not authorise invention).
//! 5. **Hand to the fixed-layout builder** as ordinary glyph runs, one per
//!    optical line, appended after the page's own paint so the frames sit
//!    above the picture they were read from. Their font index is
//!    [`OCR_FONT`], which names no PDF font: the builder neither registers nor
//!    reports a substitution for it and draws it with its default face.
//!
//! With no provider, step 1 still runs and every region is reported as
//! `pdf.import.ocr_unavailable`: OCR is never silently skipped.
//!
//! The optional layout stage (P6) runs a [`LayoutProvider`] over the merged
//! page and returns its regions in [`crate::PdfImport::layout`] for the
//! reflow builder; the fixed-layout document does not depend on it.
//!
//! Complexity per page: O(e + w) for e evidence records and w optical words
//! (a 32-point grid indexes the native glyphs), plus the provider's own cost,
//! which is bounded by the pixel limits.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use casual_doc_ocr::{
    Interrupt, LayoutInput, LayoutProvider, LayoutRegion, Never, OcrBox, OcrError, OcrLimits,
    OcrPage, OcrProvider, ProviderManifest, Raster, Stop, TextLineHint,
};
use casual_doc_pdf_read::{
    Bounds, GlyphEvidence, GlyphRunEvidence, PageEvidence, PaintEvidence, Point, RenderMode,
};

use crate::PdfImportError;
use crate::group::{self, ASCENT, DESCENT, LayoutParams};
use crate::report::{self, Findings, PdfModelOutcome as Outcome};

/// The font index optical runs carry: no PDF font has it, so the builder
/// draws them with its default face and reports no substitution for them.
pub const OCR_FONT: usize = usize::MAX;

/// Glyphs of a word are spread evenly over its box; the em size is the line
/// box height times this. A line box's height is its ink's extent
/// (`Raster::ink_rows`), ascender to descender: measured on the prose fixture
/// at 0.97 em (Helvetica Bold heading) and 0.86 em (Times body), so 1.1 puts
/// both within 10% (`docs/182` §20.4).
pub const EM_PER_LINE_BOX: f64 = 1.1;

/// Smallest picture side (points) that is worth recognising.
const MIN_REGION_SIDE: f64 = 8.0;
/// Smallest picture area (square points) that is worth recognising.
const MIN_REGION_AREA: f64 = 400.0;
/// Valid native glyph boxes covering at least this share of a picture mean
/// the producer already supplied its text (a searchable scan, or a design
/// with live text over a background): the picture is not recognised.
const NATIVE_COVERAGE: f64 = 0.01;
/// An optical word and a native glyph overlap when their intersection is at
/// least this share of the smaller box.
const OVERLAP_SHARE: f64 = 0.3;
/// Padding around an unmapped run, in ems, so the renderer's antialiasing and
/// the detector's context are inside the raster.
const RUN_PADDING_EM: f64 = 0.3;
/// The lowest effective resolution a region is recognised at.
const MIN_SCALE: f64 = 100.0 / 72.0;
/// An unmapped run is rendered so its em is at least this many pixels.
const MIN_EM_PIXELS: f64 = 40.0;
/// The largest raster side the renderer can address.
const MAX_SIDE_PIXELS: f64 = 16_384.0;
/// The grid cell of the native-glyph index, in points.
const CELL: f64 = 32.0;
/// More pictures than this on one page are recognised as one region (their
/// union), so merging stays O(n) rather than O(n^2).
const MAX_MERGED_PICTURES: usize = 256;

/// How optical text is drawn in the fixed-layout document.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OcrTextPlacement {
    /// Visible, editable frames over the retained picture (the default: the
    /// point of import is editing). The picture stays below, so nothing is
    /// lost, and the scan's own ink shows through where the substitute face
    /// differs from it. Erasing the picture behind the text is not built.
    #[default]
    Visible,
    /// Hidden runs (`hidden`), the way a searchable scan carries its text
    /// layer: the page looks like the original and the text is searchable,
    /// selectable and readable by agents.
    Hidden,
}

/// The host's OCR configuration. `Default` has no provider: OCR is reported
/// unavailable, never run.
#[derive(Clone)]
pub struct OcrOptions {
    /// The recogniser. `None` means OCR is unavailable.
    pub provider: Option<Arc<dyn OcrProvider>>,
    /// The optional layout analyser (P6). `None` runs no layout stage.
    pub layout: Option<Arc<dyn LayoutProvider>>,
    /// Render resolution for pictures, dots per inch. 150 by default: the
    /// `ocrs` detector resizes its input to a fixed size, so a 300 dpi page
    /// reads worse than a 150 dpi one (measured, `docs/182` §20.4), and a
    /// scan rarely carries more than its own resolution.
    pub dpi: f64,
    /// Bounds on one raster.
    pub limits: OcrLimits,
    /// Most pages recognised in one import.
    pub max_pages: u32,
    /// Most pixels rasterised for recognition in one import.
    pub max_total_pixels: u64,
    /// Words under this confidence are kept and reported.
    pub low_confidence: f32,
    /// Words under this confidence are dropped and reported.
    pub min_confidence: f32,
    /// How optical text is drawn.
    pub placement: OcrTextPlacement,
    /// Cancel and time budget, polled between pages, regions and lines.
    pub interrupt: Arc<dyn Interrupt>,
}

impl Default for OcrOptions {
    /// No provider; 150 dpi; [`OcrLimits::default`]; 500 pages and 4 Gpx per
    /// import; low confidence under 0.8, dropped under 0.3; visible frames;
    /// never interrupted.
    fn default() -> Self {
        Self {
            provider: None,
            layout: None,
            dpi: 150.0,
            limits: OcrLimits::default(),
            max_pages: 500,
            max_total_pixels: 4_000_000_000,
            low_confidence: 0.8,
            min_confidence: 0.3,
            placement: OcrTextPlacement::Visible,
            interrupt: Arc::new(Never),
        }
    }
}

impl fmt::Debug for OcrOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OcrOptions")
            .field("provider", &self.provider.as_ref().map(|p| p.manifest()))
            .field("layout", &self.layout.as_ref().map(|p| p.manifest()))
            .field("dpi", &self.dpi)
            .field("limits", &self.limits)
            .field("max_pages", &self.max_pages)
            .field("max_total_pixels", &self.max_total_pixels)
            .field("low_confidence", &self.low_confidence)
            .field("min_confidence", &self.min_confidence)
            .field("placement", &self.placement)
            .finish_non_exhaustive()
    }
}

/// What a page is, from its native evidence (`docs/131` §10.1).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageClass {
    /// Every painted text is valid native text; no picture needs reading.
    Native,
    /// No valid native text, and something to read optically (a scan).
    Raster,
    /// Valid native text and something to read optically.
    Hybrid,
}

/// Why a region needs optical recognition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionReason {
    /// A picture no valid native text covers.
    Picture,
    /// Glyphs whose Unicode is unrecoverable.
    UnmappedText,
}

/// A region to recognise, in page points (top left origin, y down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OcrRegion {
    /// Where.
    pub bounds: Bounds,
    /// Why.
    pub reason: RegionReason,
    /// The em size of the unmapped text, for [`RegionReason::UnmappedText`].
    pub em: f64,
}

/// What OCR did, for the host and for tests. Carries no recognised text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OcrSummary {
    /// Each page's class, in page order.
    pub classes: Vec<Option<PageClass>>,
    /// The recogniser that read pages, when one did (PDFR-I8).
    pub provider: Option<ProviderManifest>,
    /// The layout provider, when one ran.
    pub layout: Option<ProviderManifest>,
    /// Regions recognised.
    pub regions_recognized: u32,
    /// Regions that needed OCR but were not recognised (no provider, or a
    /// budget).
    pub regions_skipped: u32,
    /// Optical words kept in the document.
    pub words_kept: u32,
    /// Optical words dropped because native text covers them.
    pub words_shadowed: u32,
    /// Pixels rasterised for recognition.
    pub pixels: u64,
}

/// One page's layout regions (P6), page space.
#[derive(Clone, Debug, PartialEq)]
pub struct PageLayout {
    /// Zero-based page index.
    pub page: usize,
    /// The regions, sorted top to bottom.
    pub regions: Vec<LayoutRegion>,
}

fn area(b: &Bounds) -> f64 {
    b.width().max(0.0) * b.height().max(0.0)
}

fn union(a: &Bounds, b: &Bounds) -> Bounds {
    Bounds {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    }
}

/// A native glyph's box, from its origin, advance and the run's em.
fn glyph_box(run: &GlyphRunEvidence, glyph: &GlyphEvidence) -> Bounds {
    let size = run.size.abs().max(0.1);
    let advance = glyph.advance.max(size * 0.25);
    Bounds {
        x0: glyph.origin.x,
        y0: glyph.origin.y - size * ASCENT,
        x1: glyph.origin.x + advance,
        y1: glyph.origin.y + size * DESCENT,
    }
}

fn is_valid(glyph: &GlyphEvidence) -> bool {
    glyph
        .text
        .as_deref()
        .is_some_and(|t| t.chars().any(|c| !c.is_whitespace() && c != '\u{fffd}'))
}

/// A uniform grid over the page's native glyph boxes. Lookups are by cell,
/// so a query costs the glyphs in the cells it touches, not the page.
struct GlyphIndex {
    cells: BTreeMap<(i64, i64), Vec<usize>>,
    /// (bounds, valid, run item index, glyph index, text)
    glyphs: Vec<(Bounds, bool, usize, usize)>,
}

fn cell_range(b: &Bounds) -> (i64, i64, i64, i64) {
    (
        (b.x0 / CELL).floor() as i64,
        (b.y0 / CELL).floor() as i64,
        (b.x1 / CELL).floor() as i64,
        (b.y1 / CELL).floor() as i64,
    )
}

impl GlyphIndex {
    fn new(page: &PageEvidence) -> Self {
        let mut index = Self {
            cells: BTreeMap::new(),
            glyphs: Vec::new(),
        };
        for (item, paint) in page.items.iter().enumerate() {
            let PaintEvidence::Glyphs(run) = paint else {
                continue;
            };
            for (g, glyph) in run.glyphs.iter().enumerate() {
                let whitespace = glyph
                    .text
                    .as_deref()
                    .is_some_and(|t| !t.is_empty() && t.chars().all(char::is_whitespace));
                if whitespace {
                    continue;
                }
                let bounds = glyph_box(run, glyph);
                let id = index.glyphs.len();
                index.glyphs.push((bounds, is_valid(glyph), item, g));
                let (cx0, cy0, cx1, cy1) = cell_range(&bounds);
                // A glyph box spans one or two cells; a huge one is clamped so
                // a hostile em size cannot fill the grid.
                for cy in cy0..=cy1.min(cy0 + 64) {
                    for cx in cx0..=cx1.min(cx0 + 64) {
                        index.cells.entry((cx, cy)).or_default().push(id);
                    }
                }
            }
        }
        index
    }

    /// The glyphs whose box overlaps `b` by [`OVERLAP_SHARE`] of the smaller,
    /// in id order (paint order).
    fn overlapping(&self, b: &Bounds) -> Vec<usize> {
        let (cx0, cy0, cx1, cy1) = cell_range(b);
        let mut out = Vec::new();
        for cy in cy0..=cy1.min(cy0 + 64) {
            for cx in cx0..=cx1.min(cx0 + 64) {
                if let Some(ids) = self.cells.get(&(cx, cy)) {
                    for &id in ids {
                        let g = &self.glyphs[id].0;
                        if let Some(i) = g.intersect(b)
                            && area(&i) >= OVERLAP_SHARE * area(g).min(area(b))
                        {
                            out.push(id);
                        }
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The share of `region` covered by valid glyph boxes (overlaps between
    /// glyphs are counted twice; the threshold it is compared with is small).
    fn valid_coverage(&self, region: &Bounds) -> f64 {
        let (cx0, cy0, cx1, cy1) = cell_range(region);
        let mut seen = Vec::new();
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                if let Some(ids) = self.cells.get(&(cx, cy)) {
                    seen.extend(ids.iter().copied().filter(|&id| self.glyphs[id].1));
                }
            }
        }
        seen.sort_unstable();
        seen.dedup();
        let covered: f64 = seen
            .iter()
            .filter_map(|&id| self.glyphs[id].0.intersect(region))
            .map(|i| area(&i))
            .sum();
        covered / area(region).max(f64::MIN_POSITIVE)
    }
}

/// The picture's footprint on the page: its clip, else its bounds, rotated
/// about its centre, clipped to the page.
fn picture_footprint(image: &casual_doc_pdf_read::ImageEvidence, page: &Bounds) -> Option<Bounds> {
    let radians = image.rotation_degrees.to_radians();
    let (w, h) = image.size;
    let (sin, cos) = (radians.sin().abs(), radians.cos().abs());
    let (half_w, half_h) = ((w * cos + h * sin) / 2.0, (w * sin + h * cos) / 2.0);
    let rotated = Bounds {
        x0: image.center.x - half_w,
        y0: image.center.y - half_h,
        x1: image.center.x + half_w,
        y1: image.center.y + half_h,
    };
    let visible = match &image.clip {
        Some(clip) => rotated.intersect(clip)?,
        None => rotated,
    };
    visible.intersect(page)
}

/// Merges overlapping or touching boxes. O(n^2) up to
/// [`MAX_MERGED_PICTURES`]; beyond it, one union.
fn merge(mut boxes: Vec<Bounds>) -> Vec<Bounds> {
    if boxes.len() > MAX_MERGED_PICTURES {
        let first = boxes[0];
        return vec![boxes.iter().fold(first, |a, b| union(&a, b))];
    }
    let touching = |a: &Bounds, b: &Bounds| {
        a.x0 <= b.x1 + 1.0 && b.x0 <= a.x1 + 1.0 && a.y0 <= b.y1 + 1.0 && b.y0 <= a.y1 + 1.0
    };
    let mut changed = true;
    while changed {
        changed = false;
        let mut out: Vec<Bounds> = Vec::with_capacity(boxes.len());
        for b in boxes {
            if let Some(existing) = out.iter_mut().find(|e| touching(e, &b)) {
                *existing = union(existing, &b);
                changed = true;
            } else {
                out.push(b);
            }
        }
        boxes = out;
    }
    boxes.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
    boxes
}

/// Classifies a page and lists the regions it needs recognised
/// (`docs/131` §10.1). Pure function of the native evidence. O(e).
#[must_use]
pub fn classify(page: &PageEvidence) -> (PageClass, Vec<OcrRegion>) {
    let index = GlyphIndex::new(page);
    classify_indexed(page, &index)
}

fn classify_indexed(page: &PageEvidence, index: &GlyphIndex) -> (PageClass, Vec<OcrRegion>) {
    let page_box = Bounds {
        x0: 0.0,
        y0: 0.0,
        x1: page.width,
        y1: page.height,
    };
    let valid_native = index.glyphs.iter().any(|g| g.1);
    let pictures: Vec<Bounds> = page
        .items
        .iter()
        .filter_map(|item| match item {
            PaintEvidence::Image(image) => picture_footprint(image, &page_box),
            _ => None,
        })
        .collect();
    let mut regions: Vec<OcrRegion> = merge(pictures)
        .into_iter()
        .filter(|b| {
            b.width() >= MIN_REGION_SIDE
                && b.height() >= MIN_REGION_SIDE
                && area(b) >= MIN_REGION_AREA
        })
        .filter(|b| index.valid_coverage(b) < NATIVE_COVERAGE)
        .map(|bounds| OcrRegion {
            bounds,
            reason: RegionReason::Picture,
            em: 0.0,
        })
        .collect();
    for item in &page.items {
        let PaintEvidence::Glyphs(run) = item else {
            continue;
        };
        let mut current: Option<Bounds> = None;
        let flush = |current: &mut Option<Bounds>, regions: &mut Vec<OcrRegion>| {
            if let Some(b) = current.take() {
                let pad = run.size.abs() * RUN_PADDING_EM;
                let padded = Bounds {
                    x0: b.x0 - pad,
                    y0: b.y0 - pad,
                    x1: b.x1 + pad,
                    y1: b.y1 + pad,
                };
                let Some(bounds) = padded.intersect(&page_box) else {
                    return;
                };
                if regions
                    .iter()
                    .any(|r| r.reason == RegionReason::Picture && r.bounds.contains(&bounds, 0.0))
                {
                    return;
                }
                regions.push(OcrRegion {
                    bounds,
                    reason: RegionReason::UnmappedText,
                    em: run.size.abs(),
                });
            }
        };
        for glyph in &run.glyphs {
            if glyph.text.is_none() && run.mode != RenderMode::Invisible {
                let b = glyph_box(run, glyph);
                current = Some(current.map_or(b, |c| union(&c, &b)));
            } else {
                flush(&mut current, &mut regions);
            }
        }
        flush(&mut current, &mut regions);
    }
    let class = match (regions.is_empty(), valid_native) {
        (true, _) => PageClass::Native,
        (false, false) => PageClass::Raster,
        (false, true) => PageClass::Hybrid,
    };
    (class, regions)
}

/// The scale (pixels per point) a region renders at, or `None` when its
/// pixel bound would push it under [`MIN_SCALE`].
fn region_scale(region: &OcrRegion, dpi: f64, limits: &OcrLimits) -> Option<f64> {
    let mut scale = (dpi / 72.0).max(MIN_SCALE);
    if region.reason == RegionReason::UnmappedText && region.em > 0.0 {
        scale = scale.max(MIN_EM_PIXELS / region.em).min(8.0);
    }
    let a = area(&region.bounds).max(f64::MIN_POSITIVE);
    let fit = (limits.max_pixels as f64 / a).sqrt();
    let side = region.bounds.width().max(region.bounds.height());
    let fit_side = MAX_SIDE_PIXELS / side.max(f64::MIN_POSITIVE);
    scale = scale.min(fit).min(fit_side);
    (scale >= MIN_SCALE * 0.999).then_some(scale)
}

/// Renders the page's `region` alone at `scale`, composited over white.
/// O(region pixels + page operators).
fn render_region(
    page: &hayro::hayro_syntax::page::Page<'_>,
    region: &Bounds,
    scale: f64,
) -> Result<Raster, OcrError> {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::hayro_interpret::util::TransformExt;
    use hayro::kurbo::Affine;
    use hayro::vello_cpu;

    let width = (region.width() * scale).ceil().clamp(1.0, MAX_SIDE_PIXELS) as u16;
    let height = (region.height() * scale).ceil().clamp(1.0, MAX_SIDE_PIXELS) as u16;
    let mut ctx = vello_cpu::RenderContext::new(width, height);
    let transform = Affine::translate((-region.x0 * scale, -region.y0 * scale))
        * Affine::scale(scale)
        * page.initial_transform(true).to_kurbo();
    let settings = InterpreterSettings {
        render_annotations: false,
        ..InterpreterSettings::default()
    };
    let cache = hayro::RenderCache::new();
    hayro::render_into(
        page,
        &cache,
        &settings,
        &hayro::RenderSettings::default(),
        &mut ctx,
        transform,
    );
    ctx.flush();
    let mut pixmap = vello_cpu::Pixmap::new(ctx.width(), ctx.height());
    ctx.render_with(
        &mut pixmap,
        &mut vello_cpu::Resources::default(),
        vello_cpu::RasterizerSettings {
            target_init: vello_cpu::TargetInit::Clear(vello_cpu::color::AlphaColor::from_rgba8(
                255, 255, 255, 255,
            )),
            ..Default::default()
        },
    );
    Raster::from_premultiplied_rgba_over_white(
        u32::from(pixmap.width()),
        u32::from(pixmap.height()),
        pixmap.data_as_u8_slice(),
    )
}

/// The letters and digits of `text`, lower-cased: what two readings of the
/// same word are compared on.
fn normalised(text: &str) -> Vec<char> {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Levenshtein distance, O(a x b) on two words.
fn distance(a: &[char], b: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous + usize::from(ca != cb);
            previous = row[j + 1];
            row[j + 1] = substitution.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    row[b.len()]
}

/// Whether a native and an optical reading of the same place disagree: more
/// than a third of the longer one's letters differ (one OCR slip in a short
/// word is not a conflict).
fn conflicts(native: &str, optical: &str) -> bool {
    let (a, b) = (normalised(native), normalised(optical));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    distance(&a, &b) * 3 > a.len().max(b.len()).max(3)
}

/// One optical word in page space.
struct PageWord {
    text: String,
    bounds: Bounds,
}

/// Builds the glyph run for one optical line. Each word's characters share
/// its box evenly; words are separated by an explicit space glyph so the
/// builder's word-gap rule cannot merge them.
fn optical_run(words: &[PageWord], line: &Bounds, placement: OcrTextPlacement) -> GlyphRunEvidence {
    let em = (line.height() * EM_PER_LINE_BOX).max(1.0);
    let baseline = line.y1 - em * DESCENT;
    let mut glyphs = Vec::new();
    for (i, word) in words.iter().enumerate() {
        if i > 0 {
            let previous = &words[i - 1].bounds;
            glyphs.push(GlyphEvidence {
                text: Some(" ".to_owned()),
                origin: Point {
                    x: previous.x1,
                    y: baseline,
                },
                advance: (word.bounds.x0 - previous.x1).max(0.0),
            });
        }
        let count = word.text.chars().count().max(1);
        let advance = word.bounds.width().max(0.0) / count as f64;
        for (k, c) in word.text.chars().enumerate() {
            glyphs.push(GlyphEvidence {
                text: Some(c.to_string()),
                origin: Point {
                    x: word.bounds.x0 + advance * k as f64,
                    y: baseline,
                },
                advance,
            });
        }
    }
    GlyphRunEvidence {
        font: OCR_FONT,
        size: em,
        rotation_degrees: 0.0,
        fill: Some([0, 0, 0, 255]),
        mode: match placement {
            OcrTextPlacement::Visible => RenderMode::Fill,
            OcrTextPlacement::Hidden => RenderMode::Invisible,
        },
        glyphs,
        mcid: None,
        artifact: false,
    }
}

/// Joins optical lines that sit on the same row: a detector splits a line at
/// a wide word gap (`ocrs` does at about two ems), and two halves of one line
/// would become two frames. A line joins an earlier one when they overlap
/// vertically by half the smaller height and it starts to the right within
/// [`ROW_GAP_EMS`] ems of it. Lines are sorted by top, then left, first, so
/// the result does not depend on the provider's order (PDFR-I8). O(l log l +
/// l x [`ROW_WINDOW`]).
fn join_rows(mut lines: Vec<(Bounds, Vec<PageWord>)>) -> Vec<(Bounds, Vec<PageWord>)> {
    lines.sort_by(|a, b| a.0.y0.total_cmp(&b.0.y0).then(a.0.x0.total_cmp(&b.0.x0)));
    let mut out: Vec<(Bounds, Vec<PageWord>)> = Vec::with_capacity(lines.len());
    for (bounds, words) in lines {
        let start = out.len().saturating_sub(ROW_WINDOW);
        let target = out[start..].iter().rposition(|(row, _)| {
            let overlap = row.y1.min(bounds.y1) - row.y0.max(bounds.y0);
            let smaller = row.height().min(bounds.height());
            let em = row.height().max(bounds.height());
            let gap = bounds.x0 - row.x1;
            overlap >= 0.5 * smaller && gap >= -0.5 * em && gap <= ROW_GAP_EMS * em
        });
        match target {
            Some(at) => {
                let row = &mut out[start + at];
                row.0 = union(&row.0, &bounds);
                row.1.extend(words);
            }
            None => out.push((bounds, words)),
        }
    }
    out.sort_by(|a, b| a.0.y0.total_cmp(&b.0.y0).then(a.0.x0.total_cmp(&b.0.x0)));
    out
}

/// How far right (in line heights) a split half of a line may start.
const ROW_GAP_EMS: f64 = 4.0;
/// How many recent rows a line is tested against.
const ROW_WINDOW: usize = 8;

/// The OCR stage of one import: the host's options, a lazily opened renderer
/// over the same bytes, and the import-wide budgets.
pub(crate) struct OcrStage {
    options: OcrOptions,
    renderer: Option<hayro::hayro_syntax::Pdf>,
    pages_recognized: u32,
    out_of_time: bool,
    pub(crate) summary: OcrSummary,
    pub(crate) layout: Vec<PageLayout>,
}

impl OcrStage {
    pub(crate) fn new(options: OcrOptions) -> Self {
        let summary = OcrSummary {
            provider: options.provider.as_ref().map(|p| p.manifest()),
            layout: options.layout.as_ref().map(|p| p.manifest()),
            ..OcrSummary::default()
        };
        Self {
            options,
            renderer: None,
            pages_recognized: 0,
            out_of_time: false,
            summary,
            layout: Vec::new(),
        }
    }

    fn renderer(&mut self, bytes: &Arc<Vec<u8>>) -> Result<&hayro::hayro_syntax::Pdf, OcrError> {
        if self.renderer.is_none() {
            // The bytes were admitted by the reader (limits, encryption
            // refused); this is a second, lazily parsed view of them.
            let pdf = hayro::hayro_syntax::Pdf::new(bytes.clone()).map_err(|_| {
                OcrError::Model("hayro cannot reopen the admitted bytes".to_owned())
            })?;
            self.renderer = Some(pdf);
        }
        Ok(self.renderer.as_ref().expect("opened above"))
    }

    fn poll(&mut self) -> Result<(), PdfImportError> {
        match self.options.interrupt.poll() {
            Some(Stop::Cancelled) => Err(PdfImportError::Cancelled),
            Some(Stop::OutOfTime) => {
                self.out_of_time = true;
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// Classifies, recognises and merges one page in place, recording
    /// findings. O(page evidence + optical words) plus the provider.
    ///
    /// # Errors
    ///
    /// [`PdfImportError::Cancelled`] when the host cancels; nothing else: a
    /// provider failure on a region is reported as a skipped region.
    pub(crate) fn page(
        &mut self,
        page: &mut PageEvidence,
        bytes: &Arc<Vec<u8>>,
        findings: &mut Findings,
    ) -> Result<(), PdfImportError> {
        let at = Some(page.index);
        let index = GlyphIndex::new(page);
        let (class, regions) = classify_indexed(page, &index);
        if self.summary.classes.len() <= page.index {
            self.summary.classes.resize(page.index + 1, None);
        }
        self.summary.classes[page.index] = Some(class);
        let mut recognised: Vec<(OcrRegion, f64, OcrPage)> = Vec::new();
        if !regions.is_empty() {
            self.poll()?;
            let provider = self.options.provider.clone();
            let mut skipped = 0u32;
            match provider {
                None => {
                    findings.record(
                        report::OCR_UNAVAILABLE,
                        Outcome::Omitted,
                        regions.len() as u32,
                        at,
                    );
                    skipped = regions.len() as u32;
                }
                Some(provider) => {
                    let page_budget = self.pages_recognized < self.options.max_pages;
                    let mut any = false;
                    for region in &regions {
                        self.poll()?;
                        let scale = region_scale(region, self.options.dpi, &self.options.limits);
                        let pixels = scale.map_or(0, |s| {
                            ((region.bounds.width() * s).ceil()
                                * (region.bounds.height() * s).ceil())
                                as u64
                        });
                        let within = page_budget
                            && !self.out_of_time
                            && self.summary.pixels.saturating_add(pixels)
                                <= self.options.max_total_pixels;
                        let Some(scale) = scale.filter(|_| within) else {
                            skipped += 1;
                            continue;
                        };
                        let raster = {
                            let pdf = match self.renderer(bytes) {
                                Ok(pdf) => pdf,
                                Err(_) => {
                                    skipped += 1;
                                    continue;
                                }
                            };
                            let pages = pdf.pages();
                            let Some(hayro_page) = pages.get(page.index) else {
                                skipped += 1;
                                continue;
                            };
                            render_region(hayro_page, &region.bounds, scale)
                        };
                        let Ok(raster) = raster else {
                            skipped += 1;
                            continue;
                        };
                        self.summary.pixels = self.summary.pixels.saturating_add(raster.pixels());
                        match provider.recognize(
                            &raster,
                            &self.options.limits,
                            self.options.interrupt.as_ref(),
                        ) {
                            Ok(result) => {
                                if result.out_of_time {
                                    self.out_of_time = true;
                                }
                                if result.lines_skipped > 0 {
                                    findings.record(
                                        report::OCR_LIMIT_REACHED,
                                        Outcome::Omitted,
                                        result.lines_skipped,
                                        at,
                                    );
                                }
                                any = true;
                                recognised.push((*region, scale, result));
                            }
                            Err(OcrError::Cancelled) => return Err(PdfImportError::Cancelled),
                            Err(_) => skipped += 1,
                        }
                    }
                    if any {
                        self.pages_recognized += 1;
                    }
                    findings.record(report::OCR_LIMIT_REACHED, Outcome::Omitted, skipped, at);
                }
            }
            self.summary.regions_skipped += skipped;
            self.summary.regions_recognized += recognised.len() as u32;
            findings.record(
                report::OCR_APPLIED,
                Outcome::Mapped,
                recognised.len() as u32,
                at,
            );
        }
        self.merge(page, &index, &recognised, findings);
        self.layout_stage(page, bytes)?;
        Ok(())
    }

    /// Native-first merge (`docs/131` §10.2), then the optical runs.
    fn merge(
        &mut self,
        page: &mut PageEvidence,
        index: &GlyphIndex,
        recognised: &[(OcrRegion, f64, OcrPage)],
        findings: &mut Findings,
    ) {
        let at = Some(page.index);
        let mut low = 0u32;
        let mut conflicts_found = 0u32;
        // (item, glyph) pairs of unmapped glyphs an optical word replaces.
        let mut replaced: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let mut lines: Vec<(Bounds, Vec<PageWord>)> = Vec::new();
        for (region, scale, result) in recognised {
            for line in &result.lines {
                let mut kept: Vec<PageWord> = Vec::new();
                for word in &line.words {
                    if word.confidence < self.options.min_confidence {
                        low += 1;
                        continue;
                    }
                    let bounds = to_page(&word.bounds, &region.bounds, *scale);
                    let hits = index.overlapping(&bounds);
                    let (valid, unmapped): (Vec<usize>, Vec<usize>) =
                        hits.into_iter().partition(|&id| index.glyphs[id].1);
                    if !valid.is_empty() {
                        self.summary.words_shadowed += 1;
                        let native: String = valid
                            .iter()
                            .filter_map(|&id| {
                                let (_, _, item, g) = index.glyphs[id];
                                match &page.items[item] {
                                    PaintEvidence::Glyphs(run) => run.glyphs[g].text.clone(),
                                    _ => None,
                                }
                            })
                            .collect();
                        if conflicts(&native, &word.text) {
                            conflicts_found += 1;
                        }
                        continue;
                    }
                    for id in unmapped {
                        let (_, _, item, g) = index.glyphs[id];
                        replaced.entry(item).or_default().push(g);
                    }
                    if word.confidence < self.options.low_confidence {
                        low += 1;
                    }
                    kept.push(PageWord {
                        text: word.text.clone(),
                        bounds,
                    });
                }
                if kept.is_empty() {
                    continue;
                }
                self.summary.words_kept += kept.len() as u32;
                let first = kept[0].bounds;
                let line_box = kept.iter().fold(first, |b, w| union(&b, &w.bounds));
                lines.push((line_box, kept));
            }
        }
        let runs: Vec<GlyphRunEvidence> = join_rows(lines)
            .iter()
            .map(|(line_box, words)| optical_run(words, line_box, self.options.placement))
            .collect();
        findings.record(report::OCR_LOW_CONFIDENCE, Outcome::Degraded, low, at);
        findings.record(
            report::NATIVE_OCR_CONFLICT,
            Outcome::Mapped,
            conflicts_found,
            at,
        );
        if !replaced.is_empty() {
            // The replaced glyphs were invalid native text: still reported as
            // such (the producer's defect), now recovered by OCR.
            let count: usize = replaced.values().map(Vec::len).sum();
            findings.record(
                report::NATIVE_TEXT_INVALID,
                Outcome::Degraded,
                count as u32,
                at,
            );
            for (item, mut glyphs) in replaced {
                glyphs.sort_unstable();
                glyphs.dedup();
                if let PaintEvidence::Glyphs(run) = &mut page.items[item] {
                    let mut g = 0usize;
                    run.glyphs.retain(|_| {
                        let keep = glyphs.binary_search(&g).is_err();
                        g += 1;
                        keep
                    });
                }
            }
            page.items.retain(
                |item| !matches!(item, PaintEvidence::Glyphs(run) if run.glyphs.is_empty()),
            );
        }
        page.items
            .extend(runs.into_iter().map(PaintEvidence::Glyphs));
    }

    /// P6: the layout provider over the merged page, when the host asked.
    fn layout_stage(
        &mut self,
        page: &PageEvidence,
        bytes: &Arc<Vec<u8>>,
    ) -> Result<(), PdfImportError> {
        let Some(provider) = self.options.layout.clone() else {
            return Ok(());
        };
        self.poll()?;
        if self.out_of_time {
            return Ok(());
        }
        let upright: Vec<&GlyphRunEvidence> = page
            .items
            .iter()
            .filter_map(|item| match item {
                PaintEvidence::Glyphs(run) if run.rotation_degrees.abs() <= 0.5 => Some(run),
                _ => None,
            })
            .collect();
        let lines: Vec<TextLineHint> = group::blocks(&upright, LayoutParams::PDFMINER)
            .iter()
            .flat_map(|block| block.lines.iter())
            .map(|line| TextLineHint {
                bounds: OcrBox {
                    x0: line.x0 as f32,
                    y0: line.top as f32,
                    x1: line.x1 as f32,
                    y1: line.bottom as f32,
                },
                size: line.size as f32,
            })
            .collect();
        let page_box = Bounds {
            x0: 0.0,
            y0: 0.0,
            x1: page.width,
            y1: page.height,
        };
        let figures: Vec<OcrBox> = page
            .items
            .iter()
            .filter_map(|item| match item {
                PaintEvidence::Image(image) => picture_footprint(image, &page_box),
                _ => None,
            })
            .map(|b| OcrBox {
                x0: b.x0 as f32,
                y0: b.y0 as f32,
                x1: b.x1 as f32,
                y1: b.y1 as f32,
            })
            .collect();
        let raster = if provider.needs_raster() {
            // The whole page at the layout model's working resolution: heron
            // reads 640 x 640, so 96 dpi is ample and bounded.
            let scale = 96.0 / 72.0;
            let pdf = self.renderer(bytes).ok();
            let pages = pdf.map(hayro::hayro_syntax::Pdf::pages);
            pages
                .as_ref()
                .and_then(|pages| pages.get(page.index))
                .and_then(|p| render_region(p, &page_box, scale).ok())
                .map(|r| (r, scale as f32))
        } else {
            None
        };
        if provider.needs_raster() && raster.is_none() {
            return Ok(());
        }
        let input = LayoutInput {
            width: page.width as f32,
            height: page.height as f32,
            lines: &lines,
            figures: &figures,
            raster: raster.as_ref().map(|(r, s)| (r, *s)),
        };
        match provider.regions(&input, self.options.interrupt.as_ref()) {
            Ok(regions) => self.layout.push(PageLayout {
                page: page.index,
                regions,
            }),
            Err(OcrError::Cancelled) => return Err(PdfImportError::Cancelled),
            Err(_) => {}
        }
        Ok(())
    }
}

/// Maps a raster box back to page points.
fn to_page(b: &OcrBox, region: &Bounds, scale: f64) -> Bounds {
    Bounds {
        x0: region.x0 + f64::from(b.x0) / scale,
        y0: region.y0 + f64::from(b.y0) / scale,
        x1: region.x0 + f64::from(b.x1) / scale,
        y1: region.y0 + f64::from(b.y1) / scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_one_letter_slip_is_not_a_conflict_and_a_different_word_is() {
        assert!(!conflicts("quick", "qu1ck"));
        assert!(!conflicts("Quick,", "quick"));
        assert!(conflicts("quick", "brown"));
        assert!(!conflicts("", "brown"));
    }
}
