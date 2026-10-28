//! Page-layout regions (`docs/182` P6, `docs/131` PDFR-004/005): the seam an
//! optional ML layout model plugs into, and the heuristic that is the
//! supported baseline when none is supplied.
//!
//! Regions are **evidence for reconstruction, never structure by
//! themselves** (PDFR-I4): the reflow builder (P3) decides what a `Title`
//! region becomes and reports the inference. Fixed-layout import does not
//! need regions at all (`docs/182` §5.3).
//!
//! Coordinates are the caller's page space (the PDF importer passes points,
//! origin top left, y down). A raster, when the provider wants one, comes with
//! its scale (raster pixels per page unit).

use crate::{Determinism, Interrupt, OcrBox, OcrError, ProviderManifest, Raster, Stop};

/// What a region is. The union of Docling's and PP-DocLayout's label sets
/// that a word processor can represent; anything else is [`RegionKind::Other`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RegionKind {
    /// The document title.
    Title,
    /// A section heading.
    SectionHeader,
    /// Body text.
    Text,
    /// One list item.
    ListItem,
    /// A table.
    Table,
    /// A picture or chart.
    Figure,
    /// A caption.
    Caption,
    /// A footnote.
    Footnote,
    /// A display formula.
    Formula,
    /// Repeated top-of-page furniture.
    PageHeader,
    /// Repeated bottom-of-page furniture.
    PageFooter,
    /// Source code.
    Code,
    /// A form, checkbox or key-value region.
    Form,
    /// Anything else (a table of contents, an unrecognised label).
    Other,
}

/// One region.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutRegion {
    /// What it is.
    pub kind: RegionKind,
    /// Where, in page space.
    pub bounds: OcrBox,
    /// The provider's score in `0.0..=1.0` (a heuristic states a fixed one).
    pub confidence: f32,
}

/// A text line the caller already knows about (native or OCR), for providers
/// that work from geometry rather than pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLineHint {
    /// The line's box in page space.
    pub bounds: OcrBox,
    /// Its em size in page units.
    pub size: f32,
}

/// What a layout provider reads.
#[derive(Clone, Copy, Debug)]
pub struct LayoutInput<'a> {
    /// Page width in page units.
    pub width: f32,
    /// Page height in page units.
    pub height: f32,
    /// Text lines in page space.
    pub lines: &'a [TextLineHint],
    /// Pictures in page space.
    pub figures: &'a [OcrBox],
    /// The page rendered, and raster pixels per page unit, for pixel models.
    pub raster: Option<(&'a Raster, f32)>,
}

/// A layout analyser. Deterministic for its declared [`Determinism`]; never
/// touches the network; polls `interrupt` between model runs.
pub trait LayoutProvider: Send + Sync {
    /// What this provider is.
    fn manifest(&self) -> ProviderManifest;

    /// Whether [`LayoutProvider::regions`] needs `LayoutInput::raster`.
    fn needs_raster(&self) -> bool;

    /// The page's regions, sorted top to bottom, then left to right.
    ///
    /// # Errors
    ///
    /// [`OcrError::Cancelled`], [`OcrError::Model`], or a limit.
    fn regions(
        &self,
        input: &LayoutInput<'_>,
        interrupt: &dyn Interrupt,
    ) -> Result<Vec<LayoutRegion>, OcrError>;
}

/// Sorts regions top to bottom, left to right, with a total tie-break, so
/// the order never depends on a model's output order (PDFR-I8).
pub(crate) fn sort_regions(regions: &mut [LayoutRegion]) {
    regions.sort_by(|a, b| {
        a.bounds
            .y0
            .total_cmp(&b.bounds.y0)
            .then(a.bounds.x0.total_cmp(&b.bounds.x0))
            .then(a.kind.cmp(&b.kind))
            .then(a.bounds.y1.total_cmp(&b.bounds.y1))
            .then(a.bounds.x1.total_cmp(&b.bounds.x1))
    });
}

/// The baseline layout: geometry only, no model, no raster.
///
/// - every picture is a [`RegionKind::Figure`];
/// - lines join a block when the gap to the block's last line is under half
///   the larger em size and they overlap horizontally (pdfminer's
///   `line_margin`, the importer's own grouping rule);
/// - a block whose em size is at least [`HeuristicLayout::TITLE_RATIO`] times
///   the page's median line size and that starts in the top third of the page
///   is the [`RegionKind::Title`] (the first such block only); one of at most
///   two lines at [`HeuristicLayout::HEADING_RATIO`] times the median is a
///   [`RegionKind::SectionHeader`];
/// - a single-line block wholly within the top or bottom
///   [`HeuristicLayout::FURNITURE_BAND`] of the page is a page header or
///   footer;
/// - everything else is [`RegionKind::Text`].
///
/// It does **not** infer tables, lists, captions, footnotes or formulas: a
/// geometry-only guess at those is what `docs/131` PDFR-I4 forbids
/// presenting as structure. Those need the ruling-line analysis of P3 or a
/// model.
///
/// Complexity: O(l log l + l × b) for l lines and b blocks open in a window of
/// 32, so O(l log l). Determinism: [`Determinism::Exact`] (f32 add, compare
/// and multiply only, total orders).
#[derive(Clone, Copy, Debug, Default)]
pub struct HeuristicLayout;

impl HeuristicLayout {
    /// Em-size ratio over the median that makes a title.
    pub const TITLE_RATIO: f32 = 1.5;
    /// Em-size ratio over the median that makes a section heading.
    pub const HEADING_RATIO: f32 = 1.2;
    /// Fraction of the page height at the top and bottom treated as furniture.
    pub const FURNITURE_BAND: f32 = 0.06;
    /// The confidence a heuristic region states: it is a rule, not a score.
    pub const CONFIDENCE: f32 = 0.5;
    /// Most recent blocks a line is tested against.
    const WINDOW: usize = 32;
}

struct Block {
    bounds: OcrBox,
    last: OcrBox,
    sizes: Vec<f32>,
    lines: usize,
}

fn median(values: &mut [f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
}

impl LayoutProvider for HeuristicLayout {
    fn manifest(&self) -> ProviderManifest {
        ProviderManifest {
            provider: "heuristic-layout",
            version: env!("CARGO_PKG_VERSION"),
            models: Vec::new(),
            decoding: "rules",
            determinism: Determinism::Exact,
        }
    }

    fn needs_raster(&self) -> bool {
        false
    }

    fn regions(
        &self,
        input: &LayoutInput<'_>,
        interrupt: &dyn Interrupt,
    ) -> Result<Vec<LayoutRegion>, OcrError> {
        if interrupt.poll() == Some(Stop::Cancelled) {
            return Err(OcrError::Cancelled);
        }
        let mut lines: Vec<TextLineHint> = input.lines.to_vec();
        lines.sort_by(|a, b| {
            a.bounds
                .y0
                .total_cmp(&b.bounds.y0)
                .then(a.bounds.x0.total_cmp(&b.bounds.x0))
        });
        let mut blocks: Vec<Block> = Vec::new();
        for line in &lines {
            let start = blocks.len().saturating_sub(Self::WINDOW);
            let joined = blocks[start..].iter_mut().rev().find(|block| {
                let gap = line.bounds.y0 - block.last.y1;
                let size = line
                    .size
                    .max(block.sizes.last().copied().unwrap_or(line.size));
                let overlap =
                    line.bounds.x1.min(block.bounds.x1) - line.bounds.x0.max(block.bounds.x0);
                gap > -0.5 * size && gap < 0.5 * size && overlap > 0.0
            });
            if let Some(block) = joined {
                block.bounds = block.bounds.union(&line.bounds);
                block.last = line.bounds;
                block.sizes.push(line.size);
                block.lines += 1;
            } else {
                blocks.push(Block {
                    bounds: line.bounds,
                    last: line.bounds,
                    sizes: vec![line.size],
                    lines: 1,
                });
            }
        }
        let page_median = median(&mut lines.iter().map(|l| l.size).collect::<Vec<_>>());
        let band = Self::FURNITURE_BAND * input.height;
        let mut title_taken = false;
        let mut regions: Vec<LayoutRegion> = input
            .figures
            .iter()
            .map(|bounds| LayoutRegion {
                kind: RegionKind::Figure,
                bounds: *bounds,
                confidence: Self::CONFIDENCE,
            })
            .collect();
        for mut block in blocks {
            let size = median(&mut block.sizes);
            let kind = if block.lines == 1 && block.bounds.y1 <= band {
                RegionKind::PageHeader
            } else if block.lines == 1 && block.bounds.y0 >= input.height - band {
                RegionKind::PageFooter
            } else if !title_taken
                && page_median > 0.0
                && size >= Self::TITLE_RATIO * page_median
                && block.bounds.y0 < input.height / 3.0
            {
                title_taken = true;
                RegionKind::Title
            } else if page_median > 0.0
                && size >= Self::HEADING_RATIO * page_median
                && block.lines <= 2
            {
                RegionKind::SectionHeader
            } else {
                RegionKind::Text
            };
            regions.push(LayoutRegion {
                kind,
                bounds: block.bounds,
                confidence: Self::CONFIDENCE,
            });
        }
        sort_regions(&mut regions);
        Ok(regions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Never;

    fn line(x0: f32, y0: f32, x1: f32, size: f32) -> TextLineHint {
        TextLineHint {
            bounds: OcrBox {
                x0,
                y0,
                x1,
                y1: y0 + size,
            },
            size,
        }
    }

    #[test]
    fn a_large_first_line_is_the_title_and_the_paragraph_one_text_block() {
        let lines = [
            line(72.0, 100.0, 400.0, 24.0),
            line(72.0, 150.0, 540.0, 12.0),
            line(72.0, 164.0, 540.0, 12.0),
            line(72.0, 178.0, 500.0, 12.0),
            line(300.0, 760.0, 320.0, 10.0),
        ];
        let figure = [OcrBox {
            x0: 72.0,
            y0: 300.0,
            x1: 300.0,
            y1: 500.0,
        }];
        let input = LayoutInput {
            width: 612.0,
            height: 792.0,
            lines: &lines,
            figures: &figure,
            raster: None,
        };
        let regions = HeuristicLayout.regions(&input, &Never).unwrap();
        let kinds: Vec<RegionKind> = regions.iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds,
            [
                RegionKind::Title,
                RegionKind::Text,
                RegionKind::Figure,
                RegionKind::PageFooter
            ],
            "{regions:?}"
        );
        assert_eq!(regions[1].bounds.y0, 150.0);
        assert_eq!(regions[1].bounds.y1, 190.0);
    }

    #[test]
    fn the_heuristic_is_a_pure_function_of_its_input() {
        let lines: Vec<TextLineHint> = (0..200)
            .map(|i| line(72.0 + (i % 3) as f32, 60.0 + i as f32 * 3.5, 540.0, 12.0))
            .collect();
        let mut reversed = lines.clone();
        reversed.reverse();
        let input = |lines| LayoutInput {
            width: 612.0,
            height: 792.0,
            lines,
            figures: &[],
            raster: None,
        };
        let a = HeuristicLayout.regions(&input(&lines), &Never).unwrap();
        let b = HeuristicLayout.regions(&input(&reversed), &Never).unwrap();
        assert_eq!(a, b, "input order must not change the regions");
    }

    #[test]
    fn a_cancelled_layout_returns_no_regions() {
        let cancel = crate::CancelToken::new();
        cancel.cancel();
        let input = LayoutInput {
            width: 612.0,
            height: 792.0,
            lines: &[],
            figures: &[],
            raster: None,
        };
        assert_eq!(
            HeuristicLayout.regions(&input, &cancel),
            Err(OcrError::Cancelled)
        );
    }
}
