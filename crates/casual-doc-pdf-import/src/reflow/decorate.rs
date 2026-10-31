//! Text decorations a PDF draws as paths, put back on the text: a thin rule
//! under a run is its underline, one through its middle its strikethrough;
//! a flat fill behind a run is its shading (a highlight), one behind whole
//! lines with room to spare is their paragraph's shading; a rule under a
//! paragraph's last line, as wide as the line or wider, is the paragraph's
//! bottom border. A PDF has no such constructs: Word, LibreOffice, Chromium
//! and our own writer all paint them as rectangles beside the glyphs, and
//! before this pass the reflow dropped the rules and placed the fills as
//! drawings above or below the text they belonged to.
//!
//! Complexity: O((rules + fills) log lines + characters covered), by a
//! binary search over the page's lines sorted by baseline.

use std::collections::BTreeSet;

use casual_doc_pdf_read::Bounds;

use super::ILine;
use super::record::{Fill, RULE_THICKNESS, Rule};

/// How far below the baseline, in ems of the line, an underline sits
/// (Word draws it about 0.1 em below; a thick one reaches 0.3).
const UNDERLINE_BELOW: (f64, f64) = (-0.05, 0.4);
/// How far above the baseline, in ems, a strikethrough sits (through the
/// lowercase letters' middle, about 0.25 em up).
const STRIKE_ABOVE: (f64, f64) = (0.12, 0.5);
/// A fill is a run's shading when it covers this share of a line's height.
const COVER: f64 = 0.6;
/// A rule under a paragraph is its border within this many of its ems.
const BORDER_GAP: f64 = 1.2;

/// What was put on the text: the rules and fills used (indices into the
/// page's), so the caller places nothing else for them.
#[derive(Clone, Debug, Default)]
pub struct Decorated {
    /// Rules made underlines, strikethroughs and borders.
    pub rules: BTreeSet<usize>,
    /// Fills made run or paragraph shading.
    pub fills: BTreeSet<usize>,
}

/// One line's place: its element, its index there, and its baseline.
type Place = (usize, usize, f64);

/// The lines whose baselines lie in `low..=high`. O(log lines + found).
fn near(index: &[Place], low: f64, high: f64) -> &[Place] {
    let from = index.partition_point(|p| p.2 < low);
    let to = index.partition_point(|p| p.2 <= high);
    &index[from..to.max(from)]
}

fn rgb(color: Option<[u8; 4]>) -> Option<[u8; 3]> {
    color.map(|[r, g, b, _]| [r, g, b])
}

/// Decorates `elements` (a page's text, element by element) from the page's
/// `rules` and `fills`, skipping rules a table took (`taken`) and fills
/// inside `tables` (a table's shading is its cells'). The page's `area`
/// bounds a fill that is the page's background. O((rules + fills) log lines
/// + characters covered).
pub fn decorate(
    elements: &mut [Vec<ILine>],
    rules: &[Rule],
    taken: &BTreeSet<usize>,
    fills: &[Fill],
    tables: &[(f64, f64, f64, f64)],
    area: f64,
    work: &mut u64,
) -> Decorated {
    let mut index: Vec<Place> = elements
        .iter()
        .enumerate()
        .flat_map(|(e, lines)| {
            lines
                .iter()
                .enumerate()
                .map(move |(l, line)| (e, l, line.baseline))
        })
        .collect();
    index.sort_by(|a, b| a.2.total_cmp(&b.2));
    let mut out = Decorated::default();

    for (r, rule) in rules.iter().enumerate() {
        if taken.contains(&r) || !rule.horizontal() || rule.y1 - rule.y0 > RULE_THICKNESS {
            continue;
        }
        let y = (rule.y0 + rule.y1) / 2.0;
        // Under or through a run: the run's own line.
        let mut used = false;
        for &(e, l, _) in near(&index, y - 40.0, y + 40.0) {
            *work += 1;
            let line = &mut elements[e][l];
            let em = line.size.max(1.0);
            let below = (y - line.baseline) / em;
            let underline = (UNDERLINE_BELOW.0..=UNDERLINE_BELOW.1).contains(&below);
            let strike = (STRIKE_ABOVE.0..=STRIKE_ABOVE.1).contains(&-below);
            if !(underline || strike) || rule.x1 < line.x0 || rule.x0 > line.x1 {
                continue;
            }
            let mut any = false;
            for c in &mut line.chars {
                let centre = (c.x0 + c.x1) / 2.0;
                if centre >= rule.x0 - 0.5 && centre <= rule.x1 + 0.5 {
                    // A space inside the decorated run is decorated with it.
                    if underline {
                        c.underline = true;
                    } else {
                        c.strike = true;
                    }
                    any = true;
                }
            }
            used |= any;
            if any {
                break;
            }
        }
        if used {
            out.rules.insert(r);
            continue;
        }
        // Under a paragraph: the nearest line above it, if the rule is at
        // least as wide as that line and close under it.
        let above = near(&index, y - 60.0, y)
            .iter()
            .rev()
            .find(|&&(e, l, _)| {
                let line = &elements[e][l];
                line.bottom <= y + 0.5 && rule.x1 >= line.x0 && rule.x0 <= line.x1
            })
            .copied();
        if let Some((e, l, _)) = above {
            let line = &mut elements[e][l];
            let em = line.size.max(1.0);
            if y - line.bottom <= BORDER_GAP * em
                && rule.x0 <= line.x0 + em
                && rule.x1 >= line.x1 - em
            {
                line.border_below = Some((rule.y1 - rule.y0, rgb(rule.color)));
                out.rules.insert(r);
            }
        }
    }

    for (f, fill) in fills.iter().enumerate() {
        let b: Bounds = fill.bounds;
        let [red, green, blue, _] = fill.color;
        if (red, green, blue) == (255, 255, 255)
            || b.width() * b.height() > 0.5 * area
            || tables.iter().any(|&(x0, y0, x1, y1)| {
                b.x0 >= x0 - 12.0 && b.x1 <= x1 + 12.0 && b.y0 >= y0 - 12.0 && b.y1 <= y1 + 12.0
            })
        {
            continue;
        }
        let shade = [red, green, blue];
        let mut covered: Vec<(usize, usize)> = Vec::new();
        for &(e, l, _) in near(&index, b.y0, b.y1 + 40.0) {
            *work += 1;
            let line = &elements[e][l];
            let height = (line.bottom - line.top).max(0.1);
            let overlap = line.bottom.min(b.y1) - line.top.max(b.y0);
            if overlap >= COVER * height && line.x1 > b.x0 && line.x0 < b.x1 {
                covered.push((e, l));
            }
        }
        if covered.is_empty() {
            continue;
        }
        // Behind whole lines with room to spare (padding, or a box wider
        // than its text): the paragraph's shading. Tight behind a run (one
        // line's height, ending where its glyphs do): the run's.
        let em = covered
            .iter()
            .map(|&(e, l)| elements[e][l].size)
            .fold(1.0_f64, f64::max);
        let one_line = covered.len() == 1 && b.height() <= 1.8 * em;
        let tight = covered.iter().all(|&(e, l)| {
            let line = &elements[e][l];
            b.x0 >= line.x0 - 0.5 * em && b.x1 <= line.x1 + 0.5 * em
        });
        if one_line && tight {
            let (e, l) = covered[0];
            for c in &mut elements[e][l].chars {
                let centre = (c.x0 + c.x1) / 2.0;
                if centre >= b.x0 && centre <= b.x1 {
                    c.shade = Some(shade);
                }
            }
        } else if covered.iter().all(|&(e, l)| {
            let line = &elements[e][l];
            b.x0 <= line.x0 + 1.0 && b.x1 >= line.x1 - 1.0
        }) && b.height()
            <= covered
                .iter()
                .map(|&(e, l)| elements[e][l].bottom)
                .fold(f64::NEG_INFINITY, f64::max)
                - covered
                    .iter()
                    .map(|&(e, l)| elements[e][l].top)
                    .fold(f64::INFINITY, f64::min)
                + 1.5 * em
        {
            // Paragraph shading spans its lines, with no more than a line's
            // padding: a panel that stops inside the text, or one much
            // taller than it, is a drawing behind it.
            for &(e, l) in &covered {
                elements[e][l].shade = Some(shade);
            }
        } else {
            continue;
        }
        out.fills.insert(f);
    }
    out
}
