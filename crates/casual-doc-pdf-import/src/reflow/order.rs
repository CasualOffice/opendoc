//! Reading order for untagged pages: XY-Cut++ (Liu et al., arXiv 2504.10258).
//! Tagged pages are ordered by their structure tree instead (`mod.rs`).
//!
//! **XY-Cut++, as built here.** The paper's three stages, over the page's
//! elements (text blocks, tables, pictures) as boxes:
//!
//! 1. *Pre-mask.* Cross-layout elements — a title or figure spanning several
//!    columns — are what defeat a plain XY-cut, because they bridge the gutter
//!    the column cut needs. An element is masked when it is wider than
//!    [`BETA`] times the median element width *and* overlaps at least two
//!    column bands formed by the narrower elements.
//! 2. *Recursive cut* on projection gaps. A region holding a masked element is
//!    first cut horizontally at the gaps beside it (a cross-layout element
//!    delimits sections). Otherwise the axis is chosen by density: the
//!    vertical (column) cut, computed without the masked elements, is taken
//!    when every part is tall (covers at least [`COLUMN_COVERAGE`] of the
//!    region's height), which is what text columns are; otherwise the
//!    horizontal cut. Each cut is multi-way (every gap on the chosen axis at
//!    once), so a column of `n` paragraphs is one level.
//! 3. *Re-insertion* of each masked element that no horizontal gap separated
//!    before the first element (in the order so far) that starts below it.
//!
//! What differs from the paper, deliberately: its stage 3 matches masked
//! elements by semantic label priority (title, figure, table, ...) produced by
//! a layout model; reflow has no such labels (no model is shipped, `docs/182`
//! P6), so every element has the same priority and the geometric constraint
//! alone decides. Its thresholds were tuned on its benchmark; these are fixed
//! in code and tested.
//!
//! A vertical cut whose parts have similar widths and are tall is also a
//! *column region*: the elements under it report the region, which becomes
//! the section's `SectionColumns` (`docs/182` §4.2).
//!
//! Complexity: O(n log n) per cut level; a multi-way cut makes stacked
//! content one level, so a page of `n` elements in `k` columns is O(n log n)
//! overall, and the depth is bounded by [`MAX_DEPTH`] (then the rest is
//! sorted top-to-bottom, left-to-right). Masking and re-insertion are
//! O(n log n) (band merge, binary search, suffix minima).

/// The width factor over the median that makes an element a mask candidate.
pub const BETA: f64 = 1.3;
/// A vertical gap narrower than this (points) is not a gutter.
pub const MIN_GUTTER: f64 = 6.0;
/// The share of a region's height each side of a column cut must cover.
pub const COLUMN_COVERAGE: f64 = 0.5;
/// Column widths may differ by this factor and still be one column region.
pub const COLUMN_WIDTH_RATIO: f64 = 1.35;
/// A text column is at least this wide (points): narrower "columns" are a
/// table's cells or a tab-aligned list, not a page set in columns (found by
/// `tools/opendoc-reflow-score`: two rows of a borderless table made a
/// three-column section).
pub const MIN_COLUMN_WIDTH: f64 = 72.0;
/// Recursion depth before the remainder is sorted geometrically.
pub const MAX_DEPTH: usize = 48;

/// An element's box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    /// Left.
    pub x0: f64,
    /// Top.
    pub y0: f64,
    /// Right.
    pub x1: f64,
    /// Bottom.
    pub y1: f64,
}

impl Rect {
    /// Width.
    #[must_use]
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    /// Height.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }
}

/// A column region found by a cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnRegion {
    /// Columns.
    pub count: u16,
    /// Mean column width (points).
    pub width: f64,
    /// Mean gutter (points).
    pub gutter: f64,
    /// Left edge of the first column.
    pub left: f64,
}

/// The order and the regions.
#[derive(Clone, Debug, Default)]
pub struct Ordered {
    /// Element indices in reading order.
    pub order: Vec<usize>,
    /// Per element: the column region it sits in, if any.
    pub region: Vec<Option<usize>>,
    /// The regions.
    pub regions: Vec<ColumnRegion>,
    /// How many elements were masked.
    pub masked: usize,
}

struct Cutter<'a> {
    rects: &'a [Rect],
    masked: &'a [bool],
    region: Vec<Option<usize>>,
    regions: Vec<ColumnRegion>,
    work: &'a mut u64,
}

/// The gaps between the merged intervals of `spans` (sorted by start):
/// `(gap_start, gap_end, index of the first span after the gap)`.
fn gaps(spans: &[(f64, f64, usize)], min: f64) -> Vec<(f64, f64, usize)> {
    let mut out = Vec::new();
    let mut end = f64::NEG_INFINITY;
    for (i, (s, e, _)) in spans.iter().enumerate() {
        if i > 0 && *s - end > min {
            out.push((end, *s, i));
        }
        end = end.max(*e);
    }
    out
}

/// The union length of intervals.
fn coverage(mut spans: Vec<(f64, f64)>) -> f64 {
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (mut total, mut cur) = (0.0, None::<(f64, f64)>);
    for (s, e) in spans {
        cur = match cur {
            Some((cs, ce)) if s <= ce => Some((cs, ce.max(e))),
            Some((cs, ce)) => {
                total += ce - cs;
                Some((s, e))
            }
            None => Some((s, e)),
        };
    }
    if let Some((cs, ce)) = cur {
        total += ce - cs;
    }
    total
}

/// The parts `sorted` falls into at `cuts`.
fn parts_of(cuts: &[(f64, f64, usize)], sorted: &[(f64, f64, usize)]) -> Vec<Vec<usize>> {
    let mut parts = Vec::with_capacity(cuts.len() + 1);
    let mut start = 0;
    for &(_, _, at) in cuts {
        parts.push(sorted[start..at].iter().map(|s| s.2).collect::<Vec<_>>());
        start = at;
    }
    parts.push(sorted[start..].iter().map(|s| s.2).collect::<Vec<_>>());
    parts
}

/// Stage 3: inserts each of `masked` into `order` before the first element
/// that starts below it. O((n + m) log n): elements sorted by top with a
/// suffix minimum of position make "the first element below y" one binary
/// search.
fn reinsert(
    rects: &[Rect],
    order: Vec<usize>,
    mut masked: Vec<usize>,
    work: &mut u64,
) -> Vec<usize> {
    if masked.is_empty() {
        return order;
    }
    masked.sort_by(|&a, &b| rects[a].y0.total_cmp(&rects[b].y0).then(a.cmp(&b)));
    let mut by_top: Vec<(usize, usize)> =
        order.iter().enumerate().map(|(p, &id)| (id, p)).collect();
    by_top.sort_by(|a, b| rects[a.0].y0.total_cmp(&rects[b.0].y0));
    let mut suffix = vec![usize::MAX; by_top.len() + 1];
    for k in (0..by_top.len()).rev() {
        suffix[k] = suffix[k + 1].min(by_top[k].1);
    }
    *work += (by_top.len() + masked.len()) as u64;
    let mut inserts: Vec<(usize, usize)> = masked
        .iter()
        .map(|&m| {
            let below = by_top.partition_point(|&(e, _)| rects[e].y0 < rects[m].y1 - 1.0);
            (suffix[below], m)
        })
        .collect();
    inserts.sort_unstable();
    let mut merged = Vec::with_capacity(order.len() + inserts.len());
    let mut next = inserts.iter().peekable();
    for (p, id) in order.iter().enumerate() {
        while let Some(&&(at, m)) = next.peek() {
            if at <= p {
                merged.push(m);
                next.next();
            } else {
                break;
            }
        }
        merged.push(*id);
    }
    merged.extend(next.map(|&(_, m)| m));
    merged
}

impl Cutter<'_> {
    /// Top to bottom by visual line, then left to right: boxes sharing half
    /// the smaller one's height are one line (a raised citation mark, a
    /// larger initial, pieces of a line written as separate objects), read
    /// in x order, so a superscript is not read before the text it follows.
    /// O(n log n).
    fn geometric(&mut self, mut ids: Vec<usize>, region: Option<usize>) -> Vec<usize> {
        let rects = self.rects;
        ids.sort_by(|&a, &b| {
            let (ra, rb) = (rects[a], rects[b]);
            ra.y0.total_cmp(&rb.y0).then(ra.x0.total_cmp(&rb.x0))
        });
        let mut out: Vec<usize> = Vec::with_capacity(ids.len());
        let mut row: Vec<usize> = Vec::new();
        let mut band = (f64::INFINITY, f64::NEG_INFINITY);
        let flush = |row: &mut Vec<usize>, out: &mut Vec<usize>| {
            row.sort_by(|&a, &b| rects[a].x0.total_cmp(&rects[b].x0).then(a.cmp(&b)));
            out.append(row);
        };
        for id in ids {
            let r = rects[id];
            let shared = band.1.min(r.y1) - band.0.max(r.y0);
            let smaller = (band.1 - band.0).min(r.height()).max(0.1);
            if row.is_empty() || shared >= 0.5 * smaller {
                band = (band.0.min(r.y0), band.1.max(r.y1));
                if row.is_empty() {
                    band = (r.y0, r.y1);
                }
                row.push(id);
            } else {
                flush(&mut row, &mut out);
                band = (r.y0, r.y1);
                row.push(id);
            }
        }
        flush(&mut row, &mut out);
        for &id in &out {
            self.region[id] = region;
        }
        out
    }

    fn cut(&mut self, ids: Vec<usize>, depth: usize, region: Option<usize>) -> Vec<usize> {
        *self.work += ids.len() as u64;
        if ids.len() <= 1 || depth >= MAX_DEPTH {
            return self.geometric(ids, region);
        }
        let rects = self.rects;
        let mut by_y: Vec<(f64, f64, usize)> =
            ids.iter().map(|&i| (rects[i].y0, rects[i].y1, i)).collect();
        by_y.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
        let horizontal = gaps(&by_y, 0.0);

        // A cross-layout element delimits sections: cut at the gaps beside
        // it. The span ending a gap's upper side is the running maximum, so
        // "the element above the gap" is the one whose bottom is its start.
        let mut ends_at: Vec<(f64, usize)> = Vec::with_capacity(by_y.len());
        let mut running = (f64::NEG_INFINITY, usize::MAX);
        for s in &by_y {
            if s.1 > running.0 {
                running = (s.1, s.2);
            }
            ends_at.push(running);
        }
        *self.work += horizontal.len() as u64;
        let beside_masked: Vec<(f64, f64, usize)> = horizontal
            .iter()
            .copied()
            .filter(|&(_, _, at)| self.masked[by_y[at].2] || self.masked[ends_at[at - 1].1])
            .collect();
        if !beside_masked.is_empty() {
            let mut out = Vec::with_capacity(ids.len());
            for part in parts_of(&beside_masked, &by_y) {
                out.extend(self.cut(part, depth + 1, region));
            }
            return out;
        }

        let (kept, masked): (Vec<usize>, Vec<usize>) = ids.iter().partition(|&&i| !self.masked[i]);
        let mut by_x: Vec<(f64, f64, usize)> = kept
            .iter()
            .map(|&i| (rects[i].x0, rects[i].x1, i))
            .collect();
        by_x.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
        let vertical = gaps(&by_x, MIN_GUTTER);
        let top = by_y.first().map_or(0.0, |s| s.0);
        let bottom = by_y.iter().map(|s| s.1).fold(f64::NEG_INFINITY, f64::max);
        let height = (bottom - top).max(1e-6);
        if !vertical.is_empty() {
            let parts = parts_of(&vertical, &by_x);
            let tall = parts.iter().all(|part| {
                coverage(part.iter().map(|&i| (rects[i].y0, rects[i].y1)).collect())
                    >= COLUMN_COVERAGE * height
            });
            if tall {
                let widths: Vec<f64> = parts
                    .iter()
                    .map(|part| {
                        let x0 = part
                            .iter()
                            .map(|&i| rects[i].x0)
                            .fold(f64::INFINITY, f64::min);
                        let x1 = part
                            .iter()
                            .map(|&i| rects[i].x1)
                            .fold(f64::NEG_INFINITY, f64::max);
                        x1 - x0
                    })
                    .collect();
                let (min, max) = widths
                    .iter()
                    .fold((f64::INFINITY, 0.0_f64), |(a, b), w| (a.min(*w), b.max(*w)));
                let mut child_region = region;
                if region.is_none() && min >= MIN_COLUMN_WIDTH && max / min <= COLUMN_WIDTH_RATIO {
                    let gutter =
                        vertical.iter().map(|g| g.1 - g.0).sum::<f64>() / vertical.len() as f64;
                    self.regions.push(ColumnRegion {
                        count: u16::try_from(parts.len()).unwrap_or(u16::MAX),
                        width: widths.iter().sum::<f64>() / widths.len() as f64,
                        gutter,
                        left: by_x.first().map_or(0.0, |s| s.0),
                    });
                    child_region = Some(self.regions.len() - 1);
                }
                let mut out = Vec::with_capacity(ids.len());
                for part in parts {
                    out.extend(self.cut(part, depth + 1, child_region));
                }
                for &m in &masked {
                    self.region[m] = region;
                }
                return reinsert(rects, out, masked, self.work);
            }
        }
        if !horizontal.is_empty() {
            let mut out = Vec::with_capacity(ids.len());
            for part in parts_of(&horizontal, &by_y) {
                out.extend(self.cut(part, depth + 1, region));
            }
            return out;
        }
        if !vertical.is_empty() {
            let mut out = Vec::with_capacity(ids.len());
            for part in parts_of(&vertical, &by_x) {
                out.extend(self.cut(part, depth + 1, region));
            }
            for &m in &masked {
                self.region[m] = region;
            }
            return reinsert(rects, out, masked, self.work);
        }
        // Overlapping boxes: no gap on either axis.
        self.geometric(ids, region)
    }
}

/// XY-Cut++ over `rects`. `work` counts element visits (for the doubling
/// test). O(n log n) for the layouts described in the module comment.
#[must_use]
pub fn xy_cut_pp(rects: &[Rect], work: &mut u64) -> Ordered {
    let n = rects.len();
    let mut masked = vec![false; n];
    if n >= 3 {
        let mut widths: Vec<f64> = rects.iter().map(Rect::width).collect();
        widths.sort_by(f64::total_cmp);
        let median = widths[n / 2];
        let wide = |r: &Rect| r.width() >= BETA * median;
        // Column bands from the narrower elements (sorted, merged).
        let mut narrow: Vec<(f64, f64)> = rects
            .iter()
            .filter(|r| !wide(r))
            .map(|r| (r.x0, r.x1))
            .collect();
        narrow.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut bands: Vec<(f64, f64)> = Vec::new();
        for (s, e) in narrow {
            match bands.last_mut() {
                Some(last) if s <= last.1 + MIN_GUTTER => last.1 = last.1.max(e),
                _ => bands.push((s, e)),
            }
        }
        *work += n as u64;
        if bands.len() >= 2 {
            for (i, r) in rects.iter().enumerate() {
                if !wide(r) {
                    continue;
                }
                // Bands are disjoint and sorted: the first band ending after
                // the element's left edge, then at most one more.
                let first = bands.partition_point(|b| b.1 <= r.x0);
                let spans = bands[first..].iter().take(2).filter(|b| b.0 < r.x1).count();
                masked[i] = spans >= 2;
            }
        }
    }
    let masked_count = masked.iter().filter(|m| **m).count();
    let mut cutter = Cutter {
        rects,
        masked: &masked,
        region: vec![None; n],
        regions: Vec::new(),
        work,
    };
    let order = cutter.cut((0..n).collect(), 0, None);
    Ordered {
        order,
        region: cutter.region,
        regions: cutter.regions,
        masked: masked_count,
    }
}

#[cfg(test)]
mod tests {
    use super::{Rect, xy_cut_pp};

    fn r(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
        Rect { x0, y0, x1, y1 }
    }

    #[test]
    fn two_columns_read_column_by_column_under_a_spanning_title() {
        // A title over both columns touching them (no horizontal gap), then
        // two columns of three paragraphs whose paragraph gaps line up: the
        // case that makes a horizontal-first cut read across the gutter.
        let rects = vec![
            r(72.0, 60.0, 540.0, 82.0),   // 0 title, overlapping the columns' tops
            r(72.0, 81.0, 290.0, 200.0),  // 1 left a
            r(72.0, 210.0, 290.0, 330.0), // 2 left b
            r(72.0, 340.0, 290.0, 460.0), // 3 left c
            r(322.0, 81.0, 540.0, 200.0), // 4 right a
            r(322.0, 210.0, 540.0, 330.0),
            r(322.0, 340.0, 540.0, 460.0),
        ];
        let mut work = 0;
        let ordered = xy_cut_pp(&rects, &mut work);
        assert_eq!(ordered.order, vec![0, 1, 2, 3, 4, 5, 6]);
        assert_eq!(ordered.masked, 1);
        assert_eq!(ordered.regions.len(), 1);
        assert_eq!(ordered.regions[0].count, 2);
        assert_eq!(ordered.region[0], None);
        assert_eq!(ordered.region[5], Some(0));
    }

    #[test]
    fn a_full_width_paragraph_between_two_column_bands_splits_them() {
        let rects = vec![
            r(72.0, 100.0, 290.0, 200.0), // 0 band 1 left
            r(322.0, 100.0, 540.0, 200.0),
            r(72.0, 220.0, 540.0, 260.0), // 2 wide
            r(72.0, 280.0, 290.0, 400.0), // 3 band 2 left
            r(322.0, 280.0, 540.0, 400.0),
        ];
        let mut work = 0;
        assert_eq!(xy_cut_pp(&rects, &mut work).order, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn a_single_column_reads_top_to_bottom() {
        let rects: Vec<Rect> = (0..6)
            .map(|i| {
                let y = 100.0 + 50.0 * f64::from(i);
                r(72.0, y, 300.0 + f64::from(i), y + 40.0)
            })
            .rev()
            .collect();
        let mut work = 0;
        assert_eq!(xy_cut_pp(&rects, &mut work).order, vec![5, 4, 3, 2, 1, 0]);
    }

    /// SKILL §8: the ordering pass is O(n log n), guarded by work doubling.
    #[test]
    fn ordering_work_doubles_when_the_elements_double() {
        let page = |n: usize| -> Vec<Rect> {
            let mut out = vec![r(72.0, 10.0, 540.0, 30.0)];
            for i in 0..n {
                let y = 40.0 + 12.0 * i as f64;
                out.push(r(72.0, y, 290.0, y + 10.0));
                out.push(r(322.0, y + 3.0, 540.0, y + 13.0));
            }
            out
        };
        let column = |n: usize| -> Vec<Rect> {
            (0..n)
                .map(|i| {
                    let y = 40.0 + 12.0 * i as f64;
                    r(72.0, y, 540.0, y + 10.0)
                })
                .collect()
        };
        for (name, shape) in [
            ("two columns", &page as &dyn Fn(usize) -> Vec<Rect>),
            ("one column", &column),
        ] {
            let (mut a, mut b) = (0, 0);
            let _ = xy_cut_pp(&shape(500), &mut a);
            let _ = xy_cut_pp(&shape(1000), &mut b);
            let ratio = b as f64 / a as f64;
            assert!(
                (1.5..2.3).contains(&ratio),
                "{name}: ratio {ratio:.2} ({a} -> {b})"
            );
        }
    }
}
