//! Glyphs -> lines -> blocks, pdfminer's layout model (`docs/182` §5.2) with
//! its default parameters fixed in code ([`LayoutParams::PDFMINER`]).
//!
//! - A glyph joins the current line when the two overlap vertically by more
//!   than `line_overlap` of the smaller height and the horizontal gap is under
//!   `char_margin` times the wider glyph.
//! - A space is inserted when that gap exceeds `word_margin` times the glyph
//!   size and neither side is already whitespace.
//! - A line joins a block when it starts below the block's last line, overlaps
//!   it horizontally, and the vertical gap is under `line_margin` times the
//!   taller line.
//!
//! Complexity: O(g + l x b) for g glyphs, l lines and b open blocks on a page;
//! blocks are matched against a bounded window of the most recent
//! [`BLOCK_WINDOW`] blocks, so it is O(g + l) per page.

use casual_doc_pdf_read::{GlyphRunEvidence, RenderMode, Rgba8};

/// pdfminer's `LAParams`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutParams {
    /// Horizontal gap (in glyph widths) that still joins a line.
    pub char_margin: f64,
    /// Vertical overlap (fraction of the smaller height) that joins a line.
    pub line_overlap: f64,
    /// Gap (fraction of the glyph size) that inserts a space.
    pub word_margin: f64,
    /// Vertical gap (in line heights) that still joins a block.
    pub line_margin: f64,
}

impl LayoutParams {
    /// pdfminer.six's defaults (`docs/182` §5.2 sources).
    pub const PDFMINER: Self = Self {
        char_margin: 2.0,
        line_overlap: 0.5,
        word_margin: 0.1,
        line_margin: 0.5,
    };
}

/// How many recent blocks a new line is tested against.
pub const BLOCK_WINDOW: usize = 32;

/// Ascent and descent as fractions of the em (pdfminer uses the font bbox; the
/// importer has no reliable bbox for substituted fonts, so it uses these).
pub const ASCENT: f64 = 0.8;
/// See [`ASCENT`].
pub const DESCENT: f64 = 0.2;

/// What a run boundary is: any change of these (`docs/182` §5.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    /// Font index.
    pub font: usize,
    /// Size in half points.
    pub half_points: u32,
    /// Fill colour.
    pub fill: Option<Rgba8>,
    /// Render mode.
    pub mode: RenderMode,
}

/// One glyph placed on a line.
#[derive(Clone, Debug)]
pub struct Char {
    /// Its text (U+FFFD when unmapped).
    pub text: String,
    /// Left edge.
    pub x0: f64,
    /// Right edge.
    pub x1: f64,
    /// Baseline.
    pub baseline: f64,
    /// Em size.
    pub size: f64,
    /// Its run style.
    pub style: Style,
    /// The page stream's show operator that painted it
    /// ([`casual_doc_pdf_read::GlyphRunEvidence::show`]); an inserted space
    /// carries the preceding glyph's.
    pub show: Option<u32>,
}

/// A line of glyphs, spaces inserted.
#[derive(Clone, Debug)]
pub struct Line {
    /// Glyphs in order (including inserted spaces).
    pub chars: Vec<Char>,
    /// Left edge.
    pub x0: f64,
    /// Right edge.
    pub x1: f64,
    /// Top (baseline - ascent of the largest glyph).
    pub top: f64,
    /// Bottom (baseline + descent of the largest glyph).
    pub bottom: f64,
    /// The baseline of the first glyph.
    pub baseline: f64,
    /// Largest em size on the line.
    pub size: f64,
}

impl Line {
    fn new(c: Char) -> Self {
        Self {
            x0: c.x0,
            x1: c.x1,
            top: c.baseline - c.size * ASCENT,
            bottom: c.baseline + c.size * DESCENT,
            baseline: c.baseline,
            size: c.size,
            chars: vec![c],
        }
    }

    fn height(&self) -> f64 {
        self.bottom - self.top
    }

    fn push(&mut self, c: Char) {
        self.x1 = self.x1.max(c.x1);
        self.top = self.top.min(c.baseline - c.size * ASCENT);
        self.bottom = self.bottom.max(c.baseline + c.size * DESCENT);
        self.size = self.size.max(c.size);
        self.chars.push(c);
    }
}

/// A block of lines: one frame.
#[derive(Clone, Debug)]
pub struct Block {
    /// Lines top to bottom.
    pub lines: Vec<Line>,
    /// Left edge.
    pub x0: f64,
    /// Right edge.
    pub x1: f64,
    /// Top.
    pub top: f64,
    /// Bottom.
    pub bottom: f64,
}

/// Groups upright glyph runs (in paint order) into blocks.
///
/// `runs` must already be filtered to near-zero rotation; rotated runs are
/// framed one per run by the caller.
#[must_use]
pub fn blocks(runs: &[&GlyphRunEvidence], params: LayoutParams) -> Vec<Block> {
    blocks_counted(runs, params).0
}

/// [`blocks`], also returning how many line-against-block comparisons it made:
/// the work counter the doubling test reads (SKILL §8: count, do not time).
#[must_use]
pub fn blocks_counted(runs: &[&GlyphRunEvidence], params: LayoutParams) -> (Vec<Block>, u64) {
    let mut comparisons: u64 = 0;
    let mut lines: Vec<Line> = Vec::new();
    for run in runs {
        let style = Style {
            font: run.font,
            half_points: (run.size * 2.0).round().clamp(1.0, 65_534.0) as u32,
            fill: run.fill,
            mode: run.mode,
        };
        for glyph in &run.glyphs {
            let c = Char {
                text: glyph.text.clone().unwrap_or_else(|| "\u{fffd}".to_owned()),
                x0: glyph.origin.x,
                x1: glyph.origin.x + glyph.advance.max(0.0),
                baseline: glyph.origin.y,
                size: run.size,
                style,
                show: run.show,
            };
            let joins = lines.last().is_some_and(|line| {
                let top = c.baseline - c.size * ASCENT;
                let bottom = c.baseline + c.size * DESCENT;
                let overlap = line.bottom.min(bottom) - line.top.max(top);
                let min_height = line.height().min(bottom - top);
                let last = line.chars.last().expect("a line holds a glyph");
                let width = (last.x1 - last.x0).max(c.x1 - c.x0).max(c.size * 0.25);
                let gap = c.x0 - line.x1;
                overlap > params.line_overlap * min_height
                    && gap < params.char_margin * width
                    && c.x0 >= last.x0 - c.size * 0.05
            });
            if joins {
                let line = lines.last_mut().expect("checked");
                let last = line.chars.last().expect("a line holds a glyph");
                let gap = c.x0 - last.x1;
                let space = gap > params.word_margin * c.size
                    && !last.text.ends_with(char::is_whitespace)
                    && !c.text.starts_with(char::is_whitespace);
                if space {
                    let (style, show) = (last.style, last.show);
                    let (x0, baseline, size) = (last.x1, last.baseline, last.size);
                    line.chars.push(Char {
                        text: " ".to_owned(),
                        x0,
                        x1: c.x0,
                        baseline,
                        size,
                        style,
                        show,
                    });
                }
                line.push(c);
            } else {
                lines.push(Line::new(c));
            }
        }
    }
    let mut blocks: Vec<Block> = Vec::new();
    for line in lines {
        let start = blocks.len().saturating_sub(BLOCK_WINDOW);
        let target = blocks[start..].iter().rposition(|block| {
            comparisons += 1;
            let last = block.lines.last().expect("a block holds a line");
            let gap = line.top - last.bottom;
            let overlaps = line.x0 < block.x1 && line.x1 > block.x0;
            overlaps
                && line.top >= last.top
                && gap <= params.line_margin * last.height().max(line.height())
        });
        match target {
            Some(at) => {
                let block = &mut blocks[start + at];
                block.x0 = block.x0.min(line.x0);
                block.x1 = block.x1.max(line.x1);
                block.top = block.top.min(line.top);
                block.bottom = block.bottom.max(line.bottom);
                block.lines.push(line);
            }
            None => blocks.push(Block {
                x0: line.x0,
                x1: line.x1,
                top: line.top,
                bottom: line.bottom,
                lines: vec![line],
            }),
        }
    }
    (blocks, comparisons)
}
