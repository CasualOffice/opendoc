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
//!    reports a substitution for it and draws it with its default face. A
//!    line the provider read turned (a skewed scan, an upside-down line) is a
//!    run turned by the same angle, so the builder frames it rotated (G-3).
//! 6. **Cover the scan under visible text** ([`ScanBehindText::Cover`], the
//!    default for visible frames): each optical line read from a picture gets
//!    a filled shape in the paper's colour, sampled from the raster around
//!    the ink, painted directly above the picture and below everything drawn
//!    after it. The picture's bytes are never changed; the shapes are
//!    separate objects (one group per run of them), so deleting them shows
//!    the scan again. Reported as `pdf.import.ocr_scan_covered`.
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
    OcrLine, OcrPage, OcrProvider, ProviderManifest, Raster, Stop, TextLineHint,
};
use casual_doc_pdf_read::{
    Bounds, GlyphEvidence, GlyphRunEvidence, PageEvidence, PaintEvidence, PathCommand,
    PathEvidence, Point, RenderMode,
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
    /// lost; [`OcrOptions::scan_behind_text`] decides whether its ink under
    /// the text is covered.
    #[default]
    Visible,
    /// Hidden runs (`hidden`), the way a searchable scan carries its text
    /// layer: the page looks like the original and the text is searchable,
    /// selectable and readable by agents.
    Hidden,
}

/// What happens to the scanned ink under a visible optical line.
///
/// **Default `Cover`, and why** (`docs/182` §20a): visible frames are the
/// editing output, and a substitute face drawn over the scan's own glyphs
/// doubles every letter wherever the two faces differ (always, in practice),
/// which reads as a defect rather than as the original. Acrobat's "Edit
/// text" on a scan does the same and replaces the ink. Covering is the one
/// choice that keeps both properties the design requires: the page reads
/// once, and nothing is destroyed (the picture's bytes are untouched and the
/// covers are ordinary, deletable shapes). `ShowThrough` keeps the earlier
/// behaviour for a host that wants the scan visible under its text.
/// Hidden placement never covers: its page must look like the original.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScanBehindText {
    /// A filled shape in the paper's colour over each visible line's ink.
    #[default]
    Cover,
    /// Nothing over the scan: its ink shows through.
    ShowThrough,
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
    /// Whether the scan's ink under visible optical text is covered.
    pub scan_behind_text: ScanBehindText,
    /// Cancel and time budget, polled between pages, regions and lines.
    pub interrupt: Arc<dyn Interrupt>,
}

impl Default for OcrOptions {
    /// No provider; 150 dpi; [`OcrLimits::default`]; 500 pages and 4 Gpx per
    /// import; low confidence under 0.8, dropped under 0.3; visible frames
    /// with the scan covered under them; never interrupted.
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
            scan_behind_text: ScanBehindText::Cover,
            interrupt: Arc::new(Never),
        }
    }
}

impl OcrOptions {
    /// The default options with PP-OCRv5 over the host's `models` (with its
    /// orientation classifier when supplied) and, when this build has the
    /// `layout-heron` feature and the models include one, the heron layout
    /// provider. A layout model in a build without that feature is not
    /// loaded: [`OcrSummary::layout`] then stays `None`. O(model bytes).
    ///
    /// # Errors
    ///
    /// [`OcrError::Model`] when a model does not load.
    #[cfg(feature = "paddle")]
    pub fn from_models(models: &casual_doc_ocr::OcrModels) -> Result<Self, OcrError> {
        let provider: Arc<dyn OcrProvider> = Arc::new(models.recognizer()?);
        #[cfg(feature = "layout-heron")]
        let layout: Option<Arc<dyn LayoutProvider>> = match models.layout() {
            Some(heron) => Some(Arc::new(heron?)),
            None => None,
        };
        #[cfg(not(feature = "layout-heron"))]
        let layout: Option<Arc<dyn LayoutProvider>> = None;
        Ok(Self {
            provider: Some(provider),
            layout,
            ..Self::default()
        })
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
            .field("scan_behind_text", &self.scan_behind_text)
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
    /// Optical lines whose scanned ink is covered ([`ScanBehindText::Cover`]).
    pub lines_covered: u32,
    /// Optical lines read turned (skewed, or upside down).
    pub lines_turned: u32,
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
    use hayro::hayro_interpret::util::TransformExt;
    use hayro::kurbo::Affine;
    use hayro::vello_cpu;

    let width = (region.width() * scale).ceil().clamp(1.0, MAX_SIDE_PIXELS) as u16;
    let height = (region.height() * scale).ceil().clamp(1.0, MAX_SIDE_PIXELS) as u16;
    let mut ctx = vello_cpu::RenderContext::new(width, height);
    let transform = Affine::translate((-region.x0 * scale, -region.y0 * scale))
        * Affine::scale(scale)
        * page.initial_transform(true).to_kurbo();
    // The reader's own faces and CMaps (`casual_doc_pdf_read::provision`), so a
    // region is drawn with what the evidence was read with.
    let settings = hayro::hayro_interpret::InterpreterSettings {
        render_annotations: false,
        ..casual_doc_pdf_read::provision::interpreter_settings()
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

/// One optical word: its box in its line's upright frame (page points).
struct PageWord {
    text: String,
    bounds: Bounds,
}

/// One optical line in page points. `bounds` and the words' boxes are in the
/// line's upright frame: the page turned by `-rotation` about `pivot`.
struct PageLine {
    rotation: f64,
    pivot: Point,
    bounds: Bounds,
    words: Vec<PageWord>,
    /// The paper's colour around the line, when its ink is to be covered.
    cover: Option<[u8; 3]>,
}

/// Turns `point` clockwise (y down) by `degrees` about `pivot`.
fn turn_point(point: Point, pivot: Point, degrees: f64) -> Point {
    if degrees == 0.0 {
        return point;
    }
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (dx, dy) = (point.x - pivot.x, point.y - pivot.y);
    Point {
        x: pivot.x + dx * cos - dy * sin,
        y: pivot.y + dx * sin + dy * cos,
    }
}

/// The four corners of an upright-frame box, turned onto the page.
fn corners(b: &Bounds, pivot: Point, degrees: f64) -> [Point; 4] {
    [(b.x0, b.y0), (b.x1, b.y0), (b.x1, b.y1), (b.x0, b.y1)]
        .map(|(x, y)| turn_point(Point { x, y }, pivot, degrees))
}

/// The page-space box holding an upright-frame box once turned.
fn turned_bounds(b: &Bounds, pivot: Point, degrees: f64) -> Bounds {
    if degrees == 0.0 {
        return *b;
    }
    let c = corners(b, pivot, degrees);
    let (xs, ys) = (c.map(|p| p.x), c.map(|p| p.y));
    Bounds {
        x0: xs.iter().copied().fold(f64::INFINITY, f64::min),
        y0: ys.iter().copied().fold(f64::INFINITY, f64::min),
        x1: xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        y1: ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    }
}

/// Builds the glyph run for one optical line. Each word's characters share
/// its box evenly; words are separated by an explicit space glyph so the
/// builder's word-gap rule cannot merge them. A turned line's origins are
/// turned onto the page and the run carries its angle.
fn optical_run(line: &PageLine, placement: OcrTextPlacement) -> GlyphRunEvidence {
    let em = (line.bounds.height() * EM_PER_LINE_BOX).max(1.0);
    let baseline = line.bounds.y1 - em * DESCENT;
    let place = |x: f64| turn_point(Point { x, y: baseline }, line.pivot, line.rotation);
    let words = &line.words;
    let mut glyphs = Vec::new();
    for (i, word) in words.iter().enumerate() {
        if i > 0 {
            let previous = &words[i - 1].bounds;
            glyphs.push(GlyphEvidence {
                text: Some(" ".to_owned()),
                origin: place(previous.x1),
                advance: (word.bounds.x0 - previous.x1).max(0.0),
                glyph: None,
            });
        }
        let count = word.text.chars().count().max(1);
        let advance = word.bounds.width().max(0.0) / count as f64;
        for (k, c) in word.text.chars().enumerate() {
            glyphs.push(GlyphEvidence {
                text: Some(c.to_string()),
                origin: place(word.bounds.x0 + advance * k as f64),
                advance,
                glyph: None,
            });
        }
    }
    GlyphRunEvidence {
        font: OCR_FONT,
        size: em,
        rotation_degrees: line.rotation,
        oblique: false,
        fill: Some([0, 0, 0, 255]),
        mode: match placement {
            OcrTextPlacement::Visible => RenderMode::Fill,
            OcrTextPlacement::Hidden => RenderMode::Invisible,
        },
        glyphs,
        mcid: None,
        artifact: false,
        // OCR text was never a content-stream show operator, so it has no range a
        // byte-preserving save (P4) could rewrite: a frame holding it is not
        // "complete" and an edit there falls back to the semantic writer.
        show: None,
    }
}

/// Padding of a cover around a line's ink, in line heights: CTC places
/// characters to about a frame (8 px at the recogniser's scale), so the
/// horizontal margin is wider than the vertical one.
const COVER_PAD_X: f64 = 0.3;
const COVER_PAD_Y: f64 = 0.15;

/// The filled shape that covers a line's scanned ink: its padded upright box
/// turned onto the page, in the paper's colour.
fn cover_path(line: &PageLine, colour: [u8; 3]) -> PathEvidence {
    let h = line.bounds.height().max(0.0);
    let padded = Bounds {
        x0: line.bounds.x0 - h * COVER_PAD_X,
        y0: line.bounds.y0 - h * COVER_PAD_Y,
        x1: line.bounds.x1 + h * COVER_PAD_X,
        y1: line.bounds.y1 + h * COVER_PAD_Y,
    };
    let [a, b, c, d] = corners(&padded, line.pivot, line.rotation);
    PathEvidence {
        commands: vec![
            PathCommand::MoveTo(a),
            PathCommand::LineTo(b),
            PathCommand::LineTo(c),
            PathCommand::LineTo(d),
            PathCommand::Close,
        ],
        fill: Some(Some([colour[0], colour[1], colour[2], 255])),
        even_odd: false,
        stroke: None,
        bounds: turned_bounds(&padded, line.pivot, line.rotation),
        clipped: false,
    }
}

/// The paper's colour around an optical line: the per-channel median of the
/// raster's non-ink pixels inside the line's padded box (sampled on a grid
/// of about 16 x 64 points). White when too few are found. O(1) samples per
/// line.
fn paper_colour(raster: &Raster, line: &OcrLine) -> [u8; 3] {
    let b = &line.bounds;
    let h = b.height().max(1.0);
    let (x0, x1) = (b.x0 - h * 0.3, b.x1 + h * 0.3);
    let (y0, y1) = (b.y0 - h * 0.15, b.y1 + h * 0.15);
    let (w, rh) = (raster.width() as i64, raster.height() as i64);
    let rgb = raster.rgb();
    let mut channels: [Vec<u8>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for j in 0..16 {
        let y = y0 + (y1 - y0) * (j as f32 + 0.5) / 16.0;
        for i in 0..64 {
            let x = x0 + (x1 - x0) * (i as f32 + 0.5) / 64.0;
            let (rx, ry) = line.to_raster(x, y);
            let (px, py) = (rx.floor() as i64, ry.floor() as i64);
            if px < 0 || py < 0 || px >= w || py >= rh {
                continue;
            }
            let at = ((py * w + px) * 3) as usize;
            let p = [rgb[at], rgb[at + 1], rgb[at + 2]];
            if p.iter().map(|&c| u32::from(c)).sum::<u32>() < 3 * 128 {
                continue;
            }
            for (c, v) in p.into_iter().enumerate() {
                channels[c].push(v);
            }
        }
    }
    if channels[0].len() < 16 {
        return [255, 255, 255];
    }
    channels.map(|mut c| {
        c.sort_unstable();
        c[c.len() / 2]
    })
}

/// Joins optical lines that sit on the same row: a detector splits a line at
/// a wide word gap (`ocrs` does at about two ems), and two halves of one line
/// would become two frames. A line joins an earlier one when they overlap
/// vertically by half the smaller height and it starts to the right within
/// [`ROW_GAP_EMS`] ems of it. Only upright lines join (a turned line's frame
/// is its own). Lines are sorted by angle, then in reading order **as the
/// fixed-layout builder sees them**: a turned line's top left corner turned
/// back upright about the page `centre`, the pivot the builder turns rotated
/// text about. So an upside-down page's lines reach the builder top line
/// first and group into paragraphs. The order does not depend on the
/// provider's (PDFR-I8). O(l log l + l x [`ROW_WINDOW`]).
fn join_rows(mut lines: Vec<PageLine>, centre: Point) -> Vec<PageLine> {
    let key = |l: &PageLine| {
        let corner = turn_point(
            Point {
                x: l.bounds.x0,
                y: l.bounds.y0,
            },
            l.pivot,
            l.rotation,
        );
        let upright = turn_point(corner, centre, -l.rotation);
        ((l.rotation * 100.0).round() as i64, upright.y, upright.x)
    };
    let order = |a: &PageLine, b: &PageLine| {
        let (a, b) = (key(a), key(b));
        a.0.cmp(&b.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.total_cmp(&b.2))
    };
    lines.sort_by(order);
    let mut out: Vec<PageLine> = Vec::with_capacity(lines.len());
    for line in lines {
        let start = out.len().saturating_sub(ROW_WINDOW);
        let target = (line.rotation == 0.0)
            .then(|| {
                out[start..].iter().rposition(|row| {
                    let (row, bounds) = (&row.bounds, &line.bounds);
                    let overlap = row.y1.min(bounds.y1) - row.y0.max(bounds.y0);
                    let smaller = row.height().min(bounds.height());
                    let em = row.height().max(bounds.height());
                    let gap = bounds.x0 - row.x1;
                    overlap >= 0.5 * smaller && gap >= -0.5 * em && gap <= ROW_GAP_EMS * em
                })
            })
            .flatten()
            .filter(|&at| out[start + at].rotation == 0.0);
        match target {
            Some(at) => {
                let row = &mut out[start + at];
                row.bounds = union(&row.bounds, &line.bounds);
                row.words.extend(line.words);
            }
            None => out.push(line),
        }
    }
    out.sort_by(order);
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
        let mut recognised: Vec<Recognised> = Vec::new();
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
                                let paper = result
                                    .lines
                                    .iter()
                                    .map(|l| paper_colour(&raster, l))
                                    .collect();
                                recognised.push(Recognised {
                                    region: *region,
                                    scale,
                                    page: result,
                                    paper,
                                });
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

    /// Native-first merge (`docs/131` §10.2), then the optical runs and,
    /// under [`ScanBehindText::Cover`], the covers over their scanned ink.
    fn merge(
        &mut self,
        page: &mut PageEvidence,
        index: &GlyphIndex,
        recognised: &[Recognised],
        findings: &mut Findings,
    ) {
        let at = Some(page.index);
        let mut low = 0u32;
        let mut conflicts_found = 0u32;
        // (item, glyph) pairs of unmapped glyphs an optical word replaces.
        let mut replaced: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let mut lines: Vec<PageLine> = Vec::new();
        let cover = self.options.placement == OcrTextPlacement::Visible
            && self.options.scan_behind_text == ScanBehindText::Cover;
        for item in recognised {
            let (region, scale) = (&item.region, item.scale);
            for (line, paper) in item.page.lines.iter().zip(&item.paper) {
                let rotation = f64::from(line.rotation_degrees);
                let pivot = to_page_point(line.pivot[0], line.pivot[1], &region.bounds, scale);
                let mut kept: Vec<PageWord> = Vec::new();
                for word in &line.words {
                    if word.confidence < self.options.min_confidence {
                        low += 1;
                        continue;
                    }
                    let bounds = to_page(&word.bounds, &region.bounds, scale);
                    let hits = index.overlapping(&turned_bounds(&bounds, pivot, rotation));
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
                if rotation != 0.0 {
                    self.summary.lines_turned += 1;
                }
                let first = kept[0].bounds;
                let line_box = kept.iter().fold(first, |b, w| union(&b, &w.bounds));
                lines.push(PageLine {
                    rotation,
                    pivot,
                    bounds: line_box,
                    words: kept,
                    cover: (cover && region.reason == RegionReason::Picture).then_some(*paper),
                });
            }
        }
        let centre = Point {
            x: page.width / 2.0,
            y: page.height / 2.0,
        };
        let lines = join_rows(lines, centre);
        let runs: Vec<GlyphRunEvidence> = lines
            .iter()
            .map(|line| optical_run(line, self.options.placement))
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
        let covered = self.cover(page, &lines);
        self.summary.lines_covered += covered;
        findings.record(report::OCR_SCAN_COVERED, Outcome::Mapped, covered, at);
        page.items
            .extend(runs.into_iter().map(PaintEvidence::Glyphs));
    }

    /// Inserts each line's cover directly above the last picture it lies on,
    /// so it hides the scan and nothing painted after the picture. Returns
    /// the lines covered. O(lines x pictures + items).
    fn cover(&self, page: &mut PageEvidence, lines: &[PageLine]) -> u32 {
        let page_box = Bounds {
            x0: 0.0,
            y0: 0.0,
            x1: page.width,
            y1: page.height,
        };
        let pictures: Vec<(usize, Bounds)> = page
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| match item {
                PaintEvidence::Image(image) => picture_footprint(image, &page_box).map(|b| (i, b)),
                _ => None,
            })
            .collect();
        let mut inserts: BTreeMap<usize, Vec<PathEvidence>> = BTreeMap::new();
        for line in lines {
            let Some(colour) = line.cover else {
                continue;
            };
            let path = cover_path(line, colour);
            let Some(&(at, _)) = pictures
                .iter()
                .rev()
                .find(|(_, b)| b.intersect(&path.bounds).is_some())
            else {
                continue;
            };
            inserts.entry(at).or_default().push(path);
        }
        let mut covered = 0u32;
        for (at, paths) in inserts.into_iter().rev() {
            covered += paths.len() as u32;
            let tail = page.items.split_off(at + 1);
            page.items
                .extend(paths.into_iter().map(PaintEvidence::Path));
            page.items.extend(tail);
        }
        covered
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

/// One region's recognition: where, at what scale, what was read, and the
/// paper's colour around each line.
struct Recognised {
    region: OcrRegion,
    scale: f64,
    page: OcrPage,
    paper: Vec<[u8; 3]>,
}

/// Maps a raster point back to page points.
fn to_page_point(x: f32, y: f32, region: &Bounds, scale: f64) -> Point {
    Point {
        x: region.x0 + f64::from(x) / scale,
        y: region.y0 + f64::from(y) / scale,
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
