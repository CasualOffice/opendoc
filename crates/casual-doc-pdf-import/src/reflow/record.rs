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
    PaintEvidence, PathCommand, PathEvidence, StructureTree,
};

use crate::group::{self, Block, LayoutParams};

/// Thickness (points) under which a filled rectangle counts as a rule.
pub const RULE_THICKNESS: f64 = 2.5;
/// Length (points) a rule must reach.
pub const RULE_MIN_LENGTH: f64 = 4.0;
/// Rotation (degrees) below which text counts as upright.
const UPRIGHT_DEGREES: f64 = 0.5;

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

/// The rule a path is, when it is one: a filled thin rectangle, or a stroked
/// axis-aligned segment (each segment of a stroked rectangle counts). O(path).
fn rules_of(path: &PathEvidence) -> Option<Vec<Rule>> {
    let b = path.bounds;
    let (w, h) = (b.width(), b.height());
    if path.fill.is_some() && path.stroke.is_none() {
        let thin = (h <= RULE_THICKNESS && w >= RULE_MIN_LENGTH)
            || (w <= RULE_THICKNESS && h >= RULE_MIN_LENGTH);
        let rect_like = path.commands.len() <= 6;
        return (thin && rect_like).then(|| {
            vec![Rule {
                x0: b.x0,
                y0: b.y0,
                x1: b.x1,
                y1: b.y1,
            }]
        });
    }
    let stroke = path.stroke.as_ref()?;
    if stroke.width > RULE_THICKNESS * 2.0 {
        return None;
    }
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
                    });
                } else if (a.x - p.x).abs() < 0.5 && (a.y - p.y).abs() >= RULE_MIN_LENGTH {
                    out.push(Rule {
                        x0: a.x - half,
                        y0: a.y.min(p.y),
                        x1: a.x + half,
                        y1: a.y.max(p.y),
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
                        });
                    } else {
                        out.push(Rule {
                            x0: x0 - half,
                            y0,
                            x1: x0 + half,
                            y1,
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
        for item in &page.items {
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
                PaintEvidence::Glyphs(run) => {
                    if run.rotation_degrees.abs() > UPRIGHT_DEGREES {
                        // Reflowed upright where it starts, as fixed layout
                        // frames it (`docs/182` G-3), and reported.
                        out.rotated_runs += 1;
                        let mut flat = run.clone();
                        let origin = flat.glyphs.first().map(|g| g.origin).unwrap_or_default();
                        let mut x = origin.x;
                        for glyph in &mut flat.glyphs {
                            glyph.origin = casual_doc_pdf_read::Point { x, y: origin.y };
                            x += glyph.advance;
                        }
                        flat.rotation_degrees = 0.0;
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
