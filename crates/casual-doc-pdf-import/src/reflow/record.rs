//! The PDF source state reflow reads (`docs/182` §3 `PdfSourceState`, the I4
//! sidecar): what the import job keeps of each page's evidence after the
//! fixed-layout builder has used it, so "Convert to flowing document" is a
//! pure function of the import and needs no second parse.
//!
//! Kept per page: text grouped into blocks *within* each marked-content
//! sequence (so a block never straddles two structure elements), ruling lines
//! (thin axis-aligned paths, the evidence for table grids and footnote
//! separators), the placed pictures by the part the fixed build stored them
//! under, the vector art (each maximal run of consecutive non-rule paths, as
//! the fixed build groups them), the page's annotations, and counts of what
//! reflow cannot place (masks painted over a picture, rotated text). Kept per
//! document: the fonts, the programs the fixed build registered, the
//! structure tree, and the active content inventory.
//!
//! Memory is O(glyphs + rules + path commands + annotations); pictures are
//! referenced, not copied.

use std::collections::BTreeMap;

use casual_doc_model::v1::{EmbeddedFontProgram, MediaReference};
use casual_doc_pdf_read::{
    ActiveContent, AnnotationEvidence, Bounds, DocumentFonts, GlyphRunEvidence, PageEvidence,
    PaintEvidence, PathCommand, PathEvidence, Rgba8, StructureTree,
};

use crate::group::{self, Block, LayoutParams};

/// Thickness (points) under which a filled rectangle counts as a rule.
pub const RULE_THICKNESS: f64 = 2.5;
/// Length (points) a rule must reach.
pub const RULE_MIN_LENGTH: f64 = 4.0;
/// Rotation (degrees) below which text counts as upright.
const UPRIGHT_DEGREES: f64 = 0.5;
/// A run turned at most this far is a skewed line (a scan fed a degree or
/// two off square, read by OCR), not rotated text: it is laid level along
/// its own baseline and grouped with the lines around it.
pub const SKEW_DEGREES: f64 = 5.0;

/// `run` laid level: its glyphs placed along a horizontal baseline from its
/// first glyph, at their advances. O(glyphs).
fn level(run: &GlyphRunEvidence) -> GlyphRunEvidence {
    let mut flat = run.clone();
    let origin = flat.glyphs.first().map(|g| g.origin).unwrap_or_default();
    let mut x = origin.x;
    for glyph in &mut flat.glyphs {
        glyph.origin = casual_doc_pdf_read::Point { x, y: origin.y };
        x += glyph.advance;
    }
    flat.rotation_degrees = 0.0;
    flat
}

/// A block of text and the marked-content sequence it sits in.
#[derive(Clone, Debug)]
pub struct SourceBlock {
    /// The lines, grouped by pdfminer's margins (`group.rs`).
    pub block: Block,
    /// The `MCID` of the sequence, when tagged.
    pub mcid: Option<i32>,
    /// Whether it sits in `/Artifact` content (page furniture).
    pub artifact: bool,
}

/// An axis-aligned ruling line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rule {
    /// Left.
    pub x0: f64,
    /// Top.
    pub y0: f64,
    /// Right.
    pub x1: f64,
    /// Bottom.
    pub y1: f64,
    /// Its colour (the fill of a filled rule, the stroke of a stroked one),
    /// when a flat one: what a table's borders are drawn in.
    pub color: Option<Rgba8>,
}

/// A filled axis-aligned rectangle wider than a rule, in a flat colour: the
/// evidence for table cell shading. It is also vector art (`vectors`); a
/// table that takes it as shading takes it out of the art.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fill {
    /// Where it is.
    pub bounds: Bounds,
    /// Its colour.
    pub color: Rgba8,
}

impl Rule {
    /// Horizontal (wider than tall).
    #[must_use]
    pub fn horizontal(&self) -> bool {
        self.x1 - self.x0 >= self.y1 - self.y0
    }
}

/// A picture the fixed build placed.
#[derive(Clone, Debug)]
pub struct SourceImage {
    /// Where it sits on the page (after the clip).
    pub bounds: Bounds,
    /// The media reference (part name and type) of its bytes.
    pub media: MediaReference,
    /// A page-sized picture's ink bands (`casual_doc_pdf_read::InkBands`):
    /// where a letterhead's logo band ends and its footer band starts.
    pub ink: Option<casual_doc_pdf_read::InkBands>,
    /// The opacity it was drawn at (`ImageEvidence::opacity`): a faint
    /// picture repeated on the pages is a watermark (`watermark`).
    pub opacity: f32,
}

/// A glyph run that may be a watermark's text (`watermark`): turned well
/// off level (a diagonal stamp), or light or translucent and large. It is
/// kept out of the text blocks until the watermark pass has looked at every
/// page; one that is no watermark goes back as the text it was. O(1) each.
#[derive(Clone, Debug)]
pub struct Stamp {
    /// Its text.
    pub text: String,
    /// Clockwise rotation of its baseline, in degrees (page space, y down:
    /// Word's diagonal stamp, rising left to right, is -45).
    pub rotation: f64,
    /// Its em size, in points.
    pub size: f64,
    /// Its fill, when flat.
    pub fill: Option<Rgba8>,
    /// The middle of its baseline, in page space.
    pub center: casual_doc_pdf_read::Point,
    /// Where its baseline starts, for ordering pieces of one stamp.
    pub origin: casual_doc_pdf_read::Point,
    /// Its font (an index into the document's fonts).
    pub font: usize,
    /// The run as text, laid level, for when it is no watermark.
    pub blocks: Vec<SourceBlock>,
    /// Whether it was rotated text (reported as such if it goes back).
    pub rotated: bool,
}

/// Turned at least this far (and at most its complement to 90) is a
/// diagonal stamp's angle.
pub const STAMP_DEGREES: (f64, f64) = (20.0, 70.0);
/// A light stamp's size floor, in points.
pub const STAMP_LIGHT_SIZE: f64 = 24.0;
/// A diagonal stamp's size floor, in points.
pub const STAMP_DIAGONAL_SIZE: f64 = 14.0;

/// Whether a fill is light or translucent: alpha at most 70%, or a
/// luminance of at least 160 of 255 (silver, a pale tint). O(1).
#[must_use]
pub fn light(fill: Option<Rgba8>) -> bool {
    fill.is_some_and(|[r, g, b, a]| {
        let luminance = 0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b);
        f64::from(a) <= 0.7 * 255.0 || luminance >= 160.0
    })
}

/// Whether a rotation (degrees) is a diagonal: 20 to 70 off level either
/// way, upside down included. O(1).
#[must_use]
pub fn diagonal(rotation: f64) -> bool {
    let turned = rotation.rem_euclid(180.0);
    let off = turned.min(180.0 - turned);
    (STAMP_DEGREES.0..=STAMP_DEGREES.1).contains(&off)
}

/// Whether a run may be a watermark's text. O(1).
fn stamp_candidate(run: &GlyphRunEvidence) -> bool {
    !run.glyphs.is_empty()
        && run.mode != casual_doc_pdf_read::RenderMode::Invisible
        && ((diagonal(run.rotation_degrees) && run.size >= STAMP_DIAGONAL_SIZE)
            || (light(run.fill) && run.size >= STAMP_LIGHT_SIZE))
}

/// Vector art: a maximal run of consecutive paths that are not rules, in
/// paint order (what `fixed::build` draws as one group).
#[derive(Clone, Debug)]
pub struct SourceVector {
    /// The union of the paths' bounds.
    pub bounds: Bounds,
    /// The paths.
    pub paths: Vec<PathEvidence>,
}

impl SourceVector {
    fn push(run: &mut Option<Self>, path: &PathEvidence) {
        match run {
            Some(v) => {
                v.bounds = Bounds {
                    x0: v.bounds.x0.min(path.bounds.x0),
                    y0: v.bounds.y0.min(path.bounds.y0),
                    x1: v.bounds.x1.max(path.bounds.x1),
                    y1: v.bounds.y1.max(path.bounds.y1),
                };
                v.paths.push(path.clone());
            }
            None => {
                *run = Some(Self {
                    bounds: path.bounds,
                    paths: vec![path.clone()],
                });
            }
        }
    }

    /// A mask over the picture painted just before it: fills only, all
    /// inside that picture (OCR's scan covers, `OcrOptions::scan_behind_text`,
    /// and redaction-style boxes). It hides the picture in fixed layout; in a
    /// flowing document the picture no longer sits under text, so the mask is
    /// not placed. O(paths).
    fn masks(&self, picture: Option<Bounds>) -> bool {
        const SLACK: f64 = 0.5;
        picture.is_some_and(|p| {
            self.paths.iter().all(|path| {
                path.fill.is_some()
                    && path.stroke.is_none()
                    && path.bounds.x0 >= p.x0 - SLACK
                    && path.bounds.y0 >= p.y0 - SLACK
                    && path.bounds.x1 <= p.x1 + SLACK
                    && path.bounds.y1 <= p.y1 + SLACK
            })
        })
    }
}

/// One page as reflow sees it.
#[derive(Clone, Debug, Default)]
pub struct SourcePage {
    /// Zero-based index.
    pub index: usize,
    /// Width in points.
    pub width: f64,
    /// Height in points.
    pub height: f64,
    /// Text blocks in paint order.
    pub blocks: Vec<SourceBlock>,
    /// Ruling lines.
    pub rules: Vec<Rule>,
    /// Filled rectangles that are not rules (cell shading evidence).
    pub fills: Vec<Fill>,
    /// Pictures.
    pub images: Vec<SourceImage>,
    /// Annotations.
    pub annotations: Vec<AnnotationEvidence>,
    /// Vector art: runs of paths that are not rules, in paint order.
    pub vectors: Vec<SourceVector>,
    /// Runs of paths that only mask the picture under them (not placed).
    pub masks: u32,
    /// Rotated glyph runs (reflowed upright, reported).
    pub rotated_runs: u32,
    /// Runs that may be a watermark's text, out of `blocks` until the
    /// watermark pass decides (`watermark`).
    pub stamps: Vec<Stamp>,
    /// Where each skewed run was laid level, and the angle it had: what an
    /// image-text line box needs to sit over its ink again (`docs/182`
    /// §20b). In paint order.
    pub skews: Vec<Skew>,
}

/// A skewed run's start (its first glyph's origin, the pivot it was laid
/// level about) and its clockwise angle in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skew {
    /// The first glyph's origin, x.
    pub x: f64,
    /// The first glyph's origin, y (the levelled baseline).
    pub baseline: f64,
    /// Clockwise rotation, degrees.
    pub degrees: f64,
}

impl SourcePage {
    /// The angle of the skewed run that starts at the start of a levelled
    /// line (`x`, `baseline`), or 0 when none does. O(skews on the page).
    #[must_use]
    pub fn skew_at(&self, x: f64, baseline: f64) -> f64 {
        const NEAR: f64 = 1.5;
        self.skews
            .iter()
            .find(|s| (s.x - x).abs() <= NEAR && (s.baseline - baseline).abs() <= NEAR)
            .map_or(0.0, |s| s.degrees)
    }
}

/// The source state of a whole import.
#[derive(Clone, Debug, Default)]
pub struct PdfSourceState {
    /// Pages in order.
    pub pages: Vec<SourcePage>,
    /// Fonts by the index glyph runs use.
    pub fonts: DocumentFonts,
    /// Font index -> the family its registered program draws under.
    pub font_family: BTreeMap<usize, String>,
    /// The programs the fixed build registered (their bytes are resources).
    pub font_programs: Vec<EmbeddedFontProgram>,
    /// The tagged structure, when the PDF has one.
    pub structure: Option<StructureTree>,
    /// Active content found at admission.
    pub active: ActiveContent,
}

/// The axis-aligned rectangles a path is made of, one per subpath, when it
/// is nothing else: each subpath a move and at most four straight lines
/// (closed or not) along the axes, or a thin bar of more corners
/// (`thin_polygon`). `x y w h re` is one such subpath; a
/// producer that paints a table's borders as one path of many `re`
/// (LibreOffice, Excel, report writers) gives one per border. O(path).
#[must_use]
pub fn rectangles(path: &PathEvidence) -> Option<Vec<Bounds>> {
    let mut out = Vec::new();
    let mut points: Vec<casual_doc_pdf_read::Point> = Vec::new();
    let finish = |points: &mut Vec<casual_doc_pdf_read::Point>, out: &mut Vec<Bounds>| -> bool {
        if points.is_empty() {
            return true;
        }
        // Repeated points (a closing point equal to the start, a zero-length
        // segment) add no corner.
        points.dedup_by(|b, a| (a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() < 0.01);
        if points.len() > 1
            && let (Some(first), Some(last)) = (points.first().copied(), points.last().copied())
            && (first.x - last.x).abs() < 0.01
            && (first.y - last.y).abs() < 0.01
        {
            points.pop();
        }
        if points.len() > 5 && !thin_polygon(points) {
            return false;
        }
        // Every segment along an axis, the closing one included.
        let n = points.len();
        for (k, &a) in points.iter().enumerate() {
            let b = points[(k + 1) % n];
            if (a.x - b.x).abs() > 0.5 && (a.y - b.y).abs() > 0.5 {
                return false;
            }
        }
        let x0 = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let x1 = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        let y0 = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let y1 = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
        out.push(Bounds { x0, y0, x1, y1 });
        points.clear();
        true
    };
    for command in &path.commands {
        match *command {
            PathCommand::MoveTo(p) => {
                if !finish(&mut points, &mut out) {
                    return None;
                }
                points.push(p);
            }
            PathCommand::LineTo(p) => {
                if points.is_empty() {
                    return None;
                }
                points.push(p);
            }
            PathCommand::Close => {
                if !finish(&mut points, &mut out) {
                    return None;
                }
            }
            PathCommand::QuadTo(..) | PathCommand::CubicTo(..) => return None,
        }
    }
    if !finish(&mut points, &mut out) || out.is_empty() {
        return None;
    }
    Some(out)
}

/// Whether a polygon of more than four corners is a bar: every point within
/// a rule's thickness of one axis-aligned line. LibreOffice paints each table
/// border as a filled six-cornered bar with bevelled ends (`m`, five `l`,
/// `h`, `f*`), so its tables had no rules and reflowed as loose lines with
/// stray strokes through them. Any polygon that thin is a bar whatever its
/// corners, and its bounds are the rule. O(points).
fn thin_polygon(points: &[casual_doc_pdf_read::Point]) -> bool {
    let x0 = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let x1 = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let y0 = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let y1 = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    (x1 - x0).min(y1 - y0) <= RULE_THICKNESS
}

/// Whether a rectangle is thin enough, and long enough, to be a rule.
fn thin(b: &Bounds) -> bool {
    let (w, h) = (b.width(), b.height());
    (h <= RULE_THICKNESS && w >= RULE_MIN_LENGTH) || (w <= RULE_THICKNESS && h >= RULE_MIN_LENGTH)
}

/// The rule a path is, when it is one: filled thin rectangles (one or many
/// in one path: Word paints each border as its own `re f`, LibreOffice and
/// Excel often paint many in one), or stroked axis-aligned segments (each
/// segment of a stroked rectangle counts). O(path).
fn rules_of(path: &PathEvidence) -> Option<Vec<Rule>> {
    if let Some(fill) = path.fill
        && path.stroke.is_none()
    {
        let rects = rectangles(path)?;
        return rects.iter().all(thin).then(|| {
            rects
                .iter()
                .map(|b| Rule {
                    x0: b.x0,
                    y0: b.y0,
                    x1: b.x1,
                    y1: b.y1,
                    color: fill,
                })
                .collect()
        });
    }
    let stroke = path.stroke.as_ref()?;
    if stroke.width > RULE_THICKNESS * 2.0 {
        return None;
    }
    let color = stroke.color;
    let half = stroke.width / 2.0;
    let mut out = Vec::new();
    let mut start = None;
    let mut last = None;
    for command in &path.commands {
        match *command {
            PathCommand::MoveTo(p) => {
                start = Some(p);
                last = Some(p);
            }
            PathCommand::LineTo(p) => {
                let a = last?;
                if (a.y - p.y).abs() < 0.5 && (a.x - p.x).abs() >= RULE_MIN_LENGTH {
                    out.push(Rule {
                        x0: a.x.min(p.x),
                        y0: a.y - half,
                        x1: a.x.max(p.x),
                        y1: a.y + half,
                        color,
                    });
                } else if (a.x - p.x).abs() < 0.5 && (a.y - p.y).abs() >= RULE_MIN_LENGTH {
                    out.push(Rule {
                        x0: a.x - half,
                        y0: a.y.min(p.y),
                        x1: a.x + half,
                        y1: a.y.max(p.y),
                        color,
                    });
                } else if (a.x - p.x).abs() >= 0.5 || (a.y - p.y).abs() >= 0.5 {
                    return None;
                }
                last = Some(p);
            }
            PathCommand::Close => {
                if let (Some(a), Some(s)) = (last, start)
                    && ((a.y - s.y).abs() < 0.5 || (a.x - s.x).abs() < 0.5)
                    && (a.x != s.x || a.y != s.y)
                {
                    let (x0, x1) = (a.x.min(s.x), a.x.max(s.x));
                    let (y0, y1) = (a.y.min(s.y), a.y.max(s.y));
                    if y1 - y0 < 0.5 {
                        out.push(Rule {
                            x0,
                            y0: y0 - half,
                            x1,
                            y1: y0 + half,
                            color,
                        });
                    } else {
                        out.push(Rule {
                            x0: x0 - half,
                            y0,
                            x1: x0 + half,
                            y1,
                            color,
                        });
                    }
                }
                last = start;
            }
            PathCommand::QuadTo(..) | PathCommand::CubicTo(..) => return None,
        }
    }
    (!out.is_empty()).then_some(out)
}

impl PdfSourceState {
    /// Records one page's evidence. O(page evidence).
    pub(crate) fn record(
        &mut self,
        page: &PageEvidence,
        annotations: Vec<AnnotationEvidence>,
        params: LayoutParams,
    ) -> u64 {
        let mut comparisons = 0;
        let mut out = SourcePage {
            index: page.index,
            width: page.width,
            height: page.height,
            annotations,
            ..SourcePage::default()
        };
        // Skewed runs laid level first, so they group as the upright ones do
        // (every line of a scan fed off square had been text of its own,
        // reported rotated: 404 lines on one owner scan, and a page count
        // four times the PDF's).
        let levelled: Vec<Option<GlyphRunEvidence>> = page
            .items
            .iter()
            .map(|item| match item {
                PaintEvidence::Glyphs(run)
                    if run.rotation_degrees.abs() > UPRIGHT_DEGREES
                        && run.rotation_degrees.abs() <= SKEW_DEGREES =>
                {
                    Some(level(run))
                }
                _ => None,
            })
            .collect();
        for (item, flat) in page.items.iter().zip(&levelled) {
            if let (PaintEvidence::Glyphs(run), Some(flat)) = (item, flat)
                && let Some(first) = flat.glyphs.first()
            {
                out.skews.push(Skew {
                    x: first.origin.x,
                    baseline: first.origin.y,
                    degrees: run.rotation_degrees,
                });
            }
        }
        // Upright runs, segmented by marked-content sequence.
        let mut rotated: Vec<SourceBlock> = Vec::new();
        let mut segment: Vec<&GlyphRunEvidence> = Vec::new();
        let mut key: Option<(Option<i32>, bool)> = None;
        // The vector run being collected, and the picture painted directly
        // before it began.
        let mut art: Option<SourceVector> = None;
        let mut art_over: Option<Bounds> = None;
        let mut last_picture: Option<Bounds> = None;
        let close = |run: &mut Option<SourceVector>, over: Option<Bounds>, out: &mut SourcePage| {
            if let Some(vector) = run.take() {
                if vector.masks(over) {
                    out.masks += 1;
                } else {
                    out.vectors.push(vector);
                }
            }
        };
        let flush = |segment: &mut Vec<&GlyphRunEvidence>,
                     key: Option<(Option<i32>, bool)>,
                     out: &mut SourcePage|
         -> u64 {
            let mut n = 0;
            if let Some((mcid, artifact)) = key
                && !segment.is_empty()
            {
                let (blocks, count) = group::blocks_counted(segment, params);
                n = count;
                out.blocks
                    .extend(blocks.into_iter().map(|block| SourceBlock {
                        block,
                        mcid,
                        artifact,
                    }));
            }
            segment.clear();
            n
        };
        for (at, item) in page.items.iter().enumerate() {
            // Glyphs or a picture end the run; only the item directly before
            // a run can be the picture it masks.
            match item {
                PaintEvidence::Path(_) => {}
                PaintEvidence::Image(image) => {
                    close(&mut art, art_over, &mut out);
                    last_picture = Some(image.clip.unwrap_or(image.bounds));
                }
                PaintEvidence::Glyphs(_) => {
                    close(&mut art, art_over, &mut out);
                    last_picture = None;
                }
            }
            match item {
                PaintEvidence::Glyphs(run) if stamp_candidate(run) => {
                    let rotated = run.rotation_degrees.abs() > UPRIGHT_DEGREES;
                    let flat = level(run);
                    let (blocks, n) = group::blocks_counted(&[&flat], params);
                    comparisons += n;
                    let origin = run.glyphs.first().map(|g| g.origin).unwrap_or_default();
                    let end = run
                        .glyphs
                        .last()
                        .map(|g| {
                            let (sin, cos) = run.rotation_degrees.to_radians().sin_cos();
                            casual_doc_pdf_read::Point {
                                x: g.origin.x + g.advance * cos,
                                y: g.origin.y + g.advance * sin,
                            }
                        })
                        .unwrap_or(origin);
                    out.stamps.push(Stamp {
                        text: run
                            .glyphs
                            .iter()
                            .filter_map(|g| g.text.as_deref())
                            .collect(),
                        rotation: run.rotation_degrees,
                        size: run.size,
                        fill: run.fill,
                        center: casual_doc_pdf_read::Point {
                            x: (origin.x + end.x) / 2.0,
                            y: (origin.y + end.y) / 2.0,
                        },
                        origin,
                        font: run.font,
                        blocks: blocks
                            .into_iter()
                            .map(|block| SourceBlock {
                                block,
                                mcid: run.mcid,
                                artifact: run.artifact,
                            })
                            .collect(),
                        rotated,
                    });
                }
                PaintEvidence::Glyphs(run) => {
                    let run = levelled[at].as_ref().unwrap_or(run);
                    if run.rotation_degrees.abs() > UPRIGHT_DEGREES {
                        // Reflowed upright where it starts, as fixed layout
                        // frames it (`docs/182` G-3), and reported.
                        out.rotated_runs += 1;
                        let flat = level(run);
                        let (blocks, n) = group::blocks_counted(&[&flat], params);
                        comparisons += n;
                        rotated.extend(blocks.into_iter().map(|block| SourceBlock {
                            block,
                            mcid: run.mcid,
                            artifact: run.artifact,
                        }));
                        continue;
                    }
                    let k = (run.mcid, run.artifact);
                    if key != Some(k) {
                        comparisons += flush(&mut segment, key, &mut out);
                        key = Some(k);
                    }
                    segment.push(run);
                }
                PaintEvidence::Path(path) => match rules_of(path) {
                    Some(rules) => {
                        out.rules.extend(rules);
                        if art.is_none() {
                            last_picture = None;
                        }
                    }
                    None => {
                        // A flat filled rectangle is shading evidence too.
                        if let Some(Some(color)) = path.fill
                            && path.stroke.is_none()
                            && let Some(rects) = rectangles(path)
                        {
                            out.fills
                                .extend(rects.into_iter().map(|bounds| Fill { bounds, color }));
                        }
                        if art.is_none() {
                            art_over = last_picture;
                        }
                        SourceVector::push(&mut art, path);
                    }
                },
                PaintEvidence::Image(_) => {}
            }
        }
        close(&mut art, art_over, &mut out);
        comparisons += flush(&mut segment, key, &mut out);
        out.blocks.extend(rotated);
        self.pages.push(out);
        comparisons
    }
}
