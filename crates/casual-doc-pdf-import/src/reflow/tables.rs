//! Tables from ruling lines, and from whitespace-aligned columns above a
//! confidence threshold (`docs/182` §4.2; `docs/131` §10.6).
//!
//! **Ruled grids.** Horizontal and vertical rules that touch form connected
//! components (union-find over rule intersections). A component with at least
//! two of each, and at least two cells, is a grid: its columns are the
//! distinct vertical-rule positions, its rows the distinct horizontal ones. A
//! missing inner vertical boundary in a row is a horizontal merge
//! (`grid_span`); a missing inner horizontal boundary in a column is a
//! vertical merge (`vertical_merge`). Each character inside the grid goes to
//! the cell holding its centre, so a line the grouping joined across a cell
//! border is split back. A single ruled box (one cell) is a frame, not a
//! table, and is left alone.
//!
//! **Whitespace tables.** Lines that share a baseline but sit in different,
//! horizontally disjoint blocks are an aligned row; blocks linked by aligned
//! rows are a candidate. Two-column prose has aligned rows too (a common
//! leading), so the candidate is scored:
//! `confidence = aligned share x (1 - prose share) x min(1, rows / 3)`, where
//! the prose share is the fraction of lines with at least
//! [`PROSE_WORDS`] words. Above [`TABLE_CONFIDENCE`] it is a table; between
//! [`TABLE_CANDIDATE`] and the threshold it stays paragraphs and is reported
//! `table_topology_degraded`; below, it is not a table candidate at all.
//!
//! Complexity: O(r log r + i) for `r` rules and `i` rule intersections (a
//! grid's intersections are its cell corners, so i is O(cells)), plus
//! O(c log c) to place `c` characters; whitespace candidates are
//! O(l log l) in lines (one sort by baseline, union-find).

use std::collections::BTreeMap;

use super::record::{Fill, Rule, SourceVector};
use super::{EKind, Element};

/// A fill covering at least this share of a cell is the cell's shading.
pub const SHADING_COVER: f64 = 0.9;
/// Two tables on consecutive pages continue one another when their columns
/// agree within this many points.
pub const CONTINUATION_TOLERANCE: f64 = 3.0;
use super::{ILine, PChar};

/// Rules closer than this (points) meet.
pub const RULE_TOLERANCE: f64 = 2.0;
/// The confidence above which aligned columns become a table.
pub const TABLE_CONFIDENCE: f64 = 0.75;
/// The confidence above which a rejected candidate is reported.
pub const TABLE_CANDIDATE: f64 = 0.4;
/// Words that make a line prose.
pub const PROSE_WORDS: usize = 6;
/// Baselines within this distance (points) are one row.
pub const BASELINE_TOLERANCE: f64 = 1.5;

/// One cell.
#[derive(Clone, Debug, Default)]
pub struct ICell {
    /// Its lines (characters split at cell borders).
    pub lines: Vec<ILine>,
    /// Columns spanned.
    pub span: u32,
    /// `Some(true)` restarts a vertical merge, `Some(false)` continues one.
    pub vmerge: Option<bool>,
    /// Left edge.
    pub x0: f64,
    /// Right edge.
    pub x1: f64,
    /// Top edge (its row's), when the table knows it.
    pub y0: f64,
    /// Bottom edge.
    pub y1: f64,
    /// Background, from a filled rectangle under the cell.
    pub shading: Option<[u8; 3]>,
    /// Where its text sits in a row taller than it (`w:vAlign`): read from
    /// a tall row whose cells stand at its top, middle and bottom.
    pub valign: Option<casual_doc_model::v1::CellVerticalAlignment>,
}

/// One table.
#[derive(Clone, Debug, Default)]
pub struct ITable {
    /// Rows of cells.
    pub rows: Vec<Vec<ICell>>,
    /// Column widths (points).
    pub columns: Vec<f64>,
    /// Whether it came from rules (else from whitespace).
    pub ruled: bool,
    /// Bounds: left, top, right, bottom.
    pub bounds: (f64, f64, f64, f64),
    /// The colour of its rules, when they agree on one (else black).
    pub border_color: Option<[u8; 3]>,
    /// The thickness of its rules, in points (the most common one).
    pub border_width: f64,
    /// A borderless table's cell padding before its text, in points: the
    /// distance from the fill behind its rows to its first column's text
    /// (zero without one). Its cells' margins are this on the start side and
    /// nothing on the end, so each column's text starts where the PDF's did.
    pub pad: f64,
    /// Leading rows that repeat as a header on every page the table spans
    /// (`w:tblHeader`): the row a table continued across a page repeated.
    pub header_rows: usize,
}

struct UnionFind(Vec<usize>);

impl UnionFind {
    fn find(&mut self, mut i: usize) -> usize {
        while self.0[i] != i {
            self.0[i] = self.0[self.0[i]];
            i = self.0[i];
        }
        i
    }
    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a.max(b)] = a.min(b);
        }
    }
}

/// Distinct positions (merged within the tolerance), sorted.
fn positions(mut values: Vec<f64>) -> Vec<f64> {
    values.sort_by(f64::total_cmp);
    let mut out: Vec<f64> = Vec::new();
    for v in values {
        match out.last() {
            Some(last) if v - last <= RULE_TOLERANCE => {}
            _ => out.push(v),
        }
    }
    out
}

/// The ruled grids on a page. O(r log r + i).
#[must_use]
pub fn grids(rules: &[Rule], work: &mut u64) -> Vec<(Vec<f64>, Vec<f64>, Vec<Rule>)> {
    let t = RULE_TOLERANCE;
    let mut h: Vec<usize> = (0..rules.len())
        .filter(|&i| rules[i].horizontal())
        .collect();
    let v: Vec<usize> = (0..rules.len())
        .filter(|&i| !rules[i].horizontal())
        .collect();
    h.sort_by(|&a, &b| rules[a].y0.total_cmp(&rules[b].y0));
    let mut uf = UnionFind((0..rules.len()).collect());
    for &vi in &v {
        let vr = rules[vi];
        let from = h.partition_point(|&hi| rules[hi].y1 < vr.y0 - t);
        for &hi in &h[from..] {
            *work += 1;
            let hr = rules[hi];
            if hr.y0 > vr.y1 + t {
                break;
            }
            if hr.y1 >= vr.y0 - t && hr.x0 <= vr.x1 + t && hr.x1 >= vr.x0 - t {
                uf.union(vi, hi);
            }
        }
    }
    let mut components: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..rules.len() {
        components.entry(uf.find(i)).or_default().push(i);
    }
    let mut out = Vec::new();
    for members in components.into_values() {
        let hs: Vec<Rule> = members
            .iter()
            .map(|&i| rules[i])
            .filter(Rule::horizontal)
            .collect();
        let vs: Vec<Rule> = members
            .iter()
            .map(|&i| rules[i])
            .filter(|r| !r.horizontal())
            .collect();
        if hs.len() < 2 || vs.len() < 2 {
            continue;
        }
        let ys = positions(hs.iter().map(|r| (r.y0 + r.y1) / 2.0).collect());
        let xs = positions(vs.iter().map(|r| (r.x0 + r.x1) / 2.0).collect());
        if ys.len() < 2 || xs.len() < 2 || (ys.len() - 1) * (xs.len() - 1) < 2 {
            continue;
        }
        out.push((xs, ys, members.iter().map(|&i| rules[i]).collect()));
    }
    out
}

/// Splits `line` at the column borders `xs`: one piece per column index the
/// characters' centres fall in.
fn split_line(line: &ILine, xs: &[f64]) -> Vec<(usize, ILine)> {
    let mut pieces: Vec<(usize, Vec<PChar>)> = Vec::new();
    for c in &line.chars {
        let centre = (c.x0 + c.x1) / 2.0;
        let column = xs.partition_point(|x| *x <= centre).saturating_sub(1);
        match pieces.last_mut() {
            Some((col, chars)) if *col == column => chars.push(c.clone()),
            _ => pieces.push((column, vec![c.clone()])),
        }
    }
    pieces
        .into_iter()
        .filter(|(_, chars)| chars.iter().any(|c| !c.text.trim().is_empty()))
        .map(|(column, chars)| (column, line.with_chars(chars)))
        .collect()
}

/// A grid's rules indexed by the grid line they lie on: per column border
/// (`xs` index) the vertical intervals, per row border (`ys` index) the
/// horizontal ones, each sorted, so a border query is a binary search plus
/// the few intervals it overlaps.
struct Borders {
    vertical: Vec<Vec<(f64, f64)>>,
    horizontal: Vec<Vec<(f64, f64)>>,
    /// The rules' colour (when they agree) and most common thickness.
    style: (Option<[u8; 3]>, f64),
}

/// The colour all `rules` share (`None` when they disagree or none says)
/// and their most common thickness in points (to a tenth). O(r log r).
fn rule_style(rules: &[Rule]) -> (Option<[u8; 3]>, f64) {
    let mut colors = rules.iter().map(|r| r.color.map(|[r, g, b, _]| [r, g, b]));
    let first = colors.next().flatten();
    let color = first.filter(|c| colors.all(|other| other == Some(*c)));
    let mut widths: BTreeMap<u32, usize> = BTreeMap::new();
    for r in rules {
        let thickness = (r.x1 - r.x0).min(r.y1 - r.y0).max(0.0);
        *widths.entry((thickness * 10.0).round() as u32).or_default() += 1;
    }
    let width = widths
        .into_iter()
        .max_by_key(|(w, n)| (*n, std::cmp::Reverse(*w)))
        .map_or(0.5, |(w, _)| f64::from(w) / 10.0);
    (color, width)
}

/// A cell's lines with the pieces of one visual line put back together: a
/// line the grid split at a column border, in a cell spanning that border,
/// is two pieces on one baseline. Sorted top to bottom, then left to right.
/// O(l log l).
#[must_use]
pub fn rejoin(mut lines: Vec<ILine>) -> Vec<ILine> {
    lines.sort_by(|a, b| {
        a.baseline
            .total_cmp(&b.baseline)
            .then(a.x0.total_cmp(&b.x0))
    });
    let mut out: Vec<ILine> = Vec::with_capacity(lines.len());
    for line in lines {
        match out.last_mut() {
            Some(last) if (last.baseline - line.baseline).abs() <= BASELINE_TOLERANCE => {
                let mut chars = std::mem::take(&mut last.chars);
                let gap = line.x0 - chars.last().map_or(line.x0, |c| c.x1);
                let spaced = chars
                    .last()
                    .is_some_and(|c| c.text.ends_with(char::is_whitespace))
                    || line
                        .chars
                        .first()
                        .is_some_and(|c| c.text.starts_with(char::is_whitespace));
                if gap > 0.25 * line.size.max(1.0)
                    && !spaced
                    && let Some(previous) = chars.last()
                {
                    let mut space = previous.clone();
                    space.text = " ".to_owned();
                    space.x0 = previous.x1;
                    space.x1 = line.x0;
                    space.note = None;
                    space.comments = Vec::new();
                    space.sup = false;
                    chars.push(space);
                }
                chars.extend(line.chars.iter().cloned());
                let mut joined = last.with_chars(chars);
                joined.top = joined.top.min(line.top);
                joined.bottom = joined.bottom.max(line.bottom);
                *last = joined;
            }
            _ => out.push(line),
        }
    }
    out
}

/// The index of the position in `at` nearest `v`, within the tolerance.
fn nearest(at: &[f64], v: f64) -> Option<usize> {
    let i = at.partition_point(|x| *x < v);
    [i.checked_sub(1), Some(i)]
        .into_iter()
        .flatten()
        .filter(|&k| k < at.len() && (at[k] - v).abs() <= RULE_TOLERANCE)
        .min_by(|&a, &b| (at[a] - v).abs().total_cmp(&(at[b] - v).abs()))
}

impl Borders {
    /// O(r log r).
    fn new(xs: &[f64], ys: &[f64], rules: &[Rule]) -> Self {
        let mut out = Self {
            vertical: vec![Vec::new(); xs.len()],
            horizontal: vec![Vec::new(); ys.len()],
            style: rule_style(rules),
        };
        for r in rules {
            if r.horizontal() {
                if let Some(k) = nearest(ys, (r.y0 + r.y1) / 2.0) {
                    out.horizontal[k].push((r.x0, r.x1));
                }
            } else if let Some(k) = nearest(xs, (r.x0 + r.x1) / 2.0) {
                out.vertical[k].push((r.y0, r.y1));
            }
        }
        for list in out.vertical.iter_mut().chain(out.horizontal.iter_mut()) {
            list.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        out
    }

    /// Whether the border's rules cover `from..to` for at least 60%.
    /// O(log r + overlapping intervals).
    fn covers(list: &[(f64, f64)], from: f64, to: f64, work: &mut u64) -> bool {
        let need = 0.6 * (to - from);
        // Intervals are sorted by start; one that starts before `from` can
        // still reach into the span, so step back over the ones that do.
        let mut k = list.partition_point(|s| s.0 < from);
        while k > 0 && list[k - 1].1 > from {
            k -= 1;
            *work += 1;
        }
        let mut covered = 0.0;
        for &(s, e) in &list[k..] {
            *work += 1;
            if s >= to {
                break;
            }
            covered += (e.min(to) - s.max(from)).max(0.0);
        }
        covered >= need
    }
}

/// Builds the ruled table of every grid and takes the lines inside each out
/// of `lines` (the page's text elements). A line belongs to the grid holding
/// its centre; grids are sorted by top with a running maximum of bottoms, so
/// finding it visits only the grids spanning the line's height.
/// O((lines + cells) log grids).
pub fn ruled_tables(
    grids: &[(Vec<f64>, Vec<f64>, Vec<Rule>)],
    lines: &mut [Vec<ILine>],
    work: &mut u64,
) -> Vec<ITable> {
    let mut order: Vec<usize> = (0..grids.len()).collect();
    order.sort_by(|&a, &b| grids[a].1[0].total_cmp(&grids[b].1[0]));
    let mut reach = Vec::with_capacity(order.len());
    let mut deepest = f64::NEG_INFINITY;
    for &g in &order {
        deepest = deepest.max(*grids[g].1.last().expect("a grid has rows"));
        reach.push(deepest);
    }
    let mut contents: Vec<Vec<Vec<Vec<ILine>>>> = grids
        .iter()
        .map(|(xs, ys, _)| vec![vec![Vec::new(); xs.len() - 1]; ys.len() - 1])
        .collect();
    for element in lines.iter_mut() {
        let mut kept = Vec::with_capacity(element.len());
        for line in element.drain(..) {
            *work += 1;
            let cy = (line.top + line.bottom) / 2.0;
            let mut k = order.partition_point(|&g| grids[g].1[0] < cy);
            let mut home = None;
            while k > 0 && reach[k - 1] > cy {
                k -= 1;
                *work += 1;
                let (xs, ys, _) = &grids[order[k]];
                let (left, right) = (xs[0], *xs.last().expect("columns"));
                if cy > ys[0] && cy < *ys.last().expect("rows") && line.x1 > left && line.x0 < right
                {
                    home = Some(order[k]);
                    break;
                }
            }
            match home {
                Some(g) => {
                    let (xs, ys, _) = &grids[g];
                    let (rows, cols) = (ys.len() - 1, xs.len() - 1);
                    let row = ys
                        .partition_point(|y| *y <= cy)
                        .saturating_sub(1)
                        .min(rows - 1);
                    for (column, piece) in split_line(&line, xs) {
                        contents[g][row][column.min(cols - 1)].push(piece);
                    }
                }
                None => kept.push(line),
            }
        }
        *element = kept;
    }
    grids
        .iter()
        .zip(contents)
        .map(|((xs, ys, rules), contents)| {
            build(xs, ys, &Borders::new(xs, ys, rules), contents, work)
        })
        .collect()
}

/// One grid's table from its cell contents. O(cells log rules).
fn build(
    xs: &[f64],
    ys: &[f64],
    borders: &Borders,
    mut contents: Vec<Vec<Vec<ILine>>>,
    work: &mut u64,
) -> ITable {
    let (rows, cols) = (ys.len() - 1, xs.len() - 1);
    let (left, top, right, bottom) = (xs[0], ys[0], xs[cols], ys[rows]);
    // Merges: a missing inner vertical border in a row spans; a missing inner
    // horizontal border continues a vertical merge.
    let (border_color, border_width) = borders.style;
    let mut table = ITable {
        rows: Vec::with_capacity(rows),
        columns: xs.windows(2).map(|w| w[1] - w[0]).collect(),
        ruled: true,
        bounds: (left, top, right, bottom),
        border_color,
        border_width,
        header_rows: 0,
        pad: 0.0,
    };
    let mut owner: Vec<Vec<usize>> = vec![vec![usize::MAX; cols]; rows];
    for r in 0..rows {
        let mut row: Vec<ICell> = Vec::new();
        let mut c = 0;
        while c < cols {
            let start = c;
            while c + 1 < cols && !Borders::covers(&borders.vertical[c + 1], ys[r], ys[r + 1], work)
            {
                c += 1;
            }
            let mut cell = ICell {
                lines: Vec::new(),
                span: u32::try_from(c - start + 1).unwrap_or(1),
                vmerge: None,
                x0: xs[start],
                x1: xs[c + 1],
                y0: ys[r],
                y1: ys[r + 1],
                shading: None,
                valign: None,
            };
            for k in start..=c {
                cell.lines.append(&mut contents[r][k]);
                owner[r][k] = row.len();
            }
            cell.lines = rejoin(std::mem::take(&mut cell.lines));
            row.push(cell);
            c += 1;
        }
        table.rows.push(row);
    }
    // `root[r][c]`: the cell a vertically merged cell's text belongs to.
    let mut root: Vec<Vec<(usize, usize)>> = table
        .rows
        .iter()
        .enumerate()
        .map(|(r, row)| (0..row.len()).map(|c| (r, c)).collect())
        .collect();
    for r in 1..rows {
        for ci in 0..table.rows[r].len() {
            let (x0, x1, span) = {
                let cell = &table.rows[r][ci];
                (cell.x0, cell.x1, cell.span)
            };
            let open = !Borders::covers(&borders.horizontal[r], x0, x1, work);
            let start = xs.partition_point(|x| *x < x0 - RULE_TOLERANCE);
            let above = owner[r - 1].get(start).copied().unwrap_or(usize::MAX);
            let aligned = above != usize::MAX
                && table.rows[r - 1]
                    .get(above)
                    .is_some_and(|a| (a.x0 - x0).abs() <= RULE_TOLERANCE && a.span == span);
            if open && aligned {
                if table.rows[r - 1][above].vmerge.is_none() {
                    table.rows[r - 1][above].vmerge = Some(true);
                }
                table.rows[r][ci].vmerge = Some(false);
                root[r][ci] = root[r - 1][above];
            }
        }
    }
    // A merged cell's text lives in its first cell, top to bottom.
    for (r, row_roots) in root.iter().enumerate().skip(1) {
        for (ci, &(rr, rc)) in row_roots.iter().enumerate() {
            if (rr, rc) != (r, ci) {
                let moved = std::mem::take(&mut table.rows[r][ci].lines);
                table.rows[rr][rc].lines.extend(moved);
            }
        }
    }
    table
}

/// Gives each cell of `table` the colour of the last flat fill (in paint
/// order) covering at least [`SHADING_COVER`] of it; a white fill over an
/// earlier colour clears it, as on the page. Only fills that meet the table
/// are weighed. O(cells x fills meeting the table).
pub fn shade(table: &mut ITable, fills: &[Fill], work: &mut u64) {
    let (left, top, right, bottom) = table.bounds;
    let meeting: Vec<&Fill> = fills
        .iter()
        .filter(|f| {
            f.bounds.x1 > left && f.bounds.x0 < right && f.bounds.y1 > top && f.bounds.y0 < bottom
        })
        .collect();
    if meeting.is_empty() {
        return;
    }
    let ruled = table.ruled;
    for cell in table.rows.iter_mut().flatten() {
        // A ruled cell is its rules' box; a borderless cell's box is its
        // text (its band runs to the middle of the gap to the next row,
        // which a row's fill does not reach).
        let (x0, y0, x1, y1) = if ruled || cell.lines.is_empty() {
            (cell.x0, cell.y0, cell.x1, cell.y1)
        } else {
            (
                cell.lines
                    .iter()
                    .map(|l| l.x0)
                    .fold(f64::INFINITY, f64::min),
                cell.lines
                    .iter()
                    .map(|l| l.top)
                    .fold(f64::INFINITY, f64::min),
                cell.lines
                    .iter()
                    .map(|l| l.x1)
                    .fold(f64::NEG_INFINITY, f64::max),
                cell.lines
                    .iter()
                    .map(|l| l.bottom)
                    .fold(f64::NEG_INFINITY, f64::max),
            )
        };
        let area = (x1 - x0) * (y1 - y0);
        if area <= 0.0 {
            continue;
        }
        for fill in &meeting {
            *work += 1;
            let b = fill.bounds;
            let w = (b.x1.min(x1) - b.x0.max(x0)).max(0.0);
            let h = (b.y1.min(y1) - b.y0.max(y0)).max(0.0);
            if w * h >= SHADING_COVER * area {
                let [r, g, bl, _] = fill.color;
                cell.shading = ([r, g, bl] != [255, 255, 255]).then_some([r, g, bl]);
            }
        }
    }
}

/// Slack after a borderless table's last column's text, in points, for a
/// substitute face that sets it wider.
pub const CELL_MARGINS: f64 = 12.0;

/// Widens a whitespace table to the fills painted behind its rows: a
/// borderless table's shaded header row or banded rows run from the table's
/// left to its right, while its bounds were only its text's, so the table
/// reflowed as narrow as its text and the fills, wider than it, were placed
/// as drawings beside it. A fill counts when it lies within the table's
/// rows (half a row of slack) and meets its columns. O(fills).
pub fn fit_to_fills(table: &mut ITable, fills: &[Fill], work: &mut u64) {
    if table.ruled || table.rows.is_empty() {
        return;
    }
    let (left, top, right, bottom) = table.bounds;
    let slack = (bottom - top) / table.rows.len() as f64 / 2.0;
    let (mut x0, mut x1) = (left, right);
    for fill in fills {
        *work += 1;
        let b = fill.bounds;
        let [r, g, bl, _] = fill.color;
        if [r, g, bl] == [255, 255, 255]
            || b.y0 < top - slack
            || b.y1 > bottom + slack
            || b.x1 <= left
            || b.x0 >= right
        {
            continue;
        }
        x0 = x0.min(b.x0);
        x1 = x1.max(b.x1);
    }
    if x1 > right {
        if let Some(w) = table.columns.last_mut() {
            *w += x1 - right;
        }
        for row in &mut table.rows {
            if let Some(cell) = row.last_mut() {
                cell.x1 = x1;
            }
        }
    }
    // Every column starts its padding before its text, so the widths
    // between column starts are unchanged and the first cell's text keeps
    // its place.
    if x0 < left {
        table.pad = (left - x0).min(12.0);
        for row in &mut table.rows {
            if let Some(cell) = row.first_mut() {
                cell.x0 = x0;
            }
        }
    }
    // The bounds stay the text's: they set the page's body (its margins),
    // which a fill reaching into the margin must not move.
}

/// Whether vector art is a table's shading: every path a flat fill of
/// rectangles inside one of `tables` (a point of slack). O(paths x tables).
#[must_use]
pub fn is_shading(vector: &SourceVector, tables: &[ITable]) -> bool {
    let b = vector.bounds;
    tables.iter().any(|t| {
        // A whitespace table's bounds are its text's; its cells' fills reach
        // past the text by the cell padding.
        let slack = if t.ruled { 1.0 } else { 12.0 };
        let (left, top, right, bottom) = t.bounds;
        // Its cells may reach past its text (`fit_to_fills`).
        let cells = t.rows.iter().flatten();
        let left = cells.clone().map(|c| c.x0).fold(left, f64::min);
        let right = cells.map(|c| c.x1).fold(right, f64::max);
        b.x0 >= left - slack
            && b.x1 <= right + slack
            && b.y0 >= top - slack
            && b.y1 <= bottom + slack
            && vector.paths.iter().all(|path| {
                matches!(path.fill, Some(Some(_)))
                    && path.stroke.is_none()
                    && super::record::rectangles(path).is_some()
            })
    })
}

/// The text of a row's cells, for comparing a repeated header.
fn row_text(row: &[ICell]) -> Vec<String> {
    row.iter()
        .map(|cell| {
            cell.lines
                .iter()
                .flat_map(|l| l.chars.iter().map(|c| c.text.as_str()))
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// Whether `next` continues `table`: as many columns, each within
/// [`CONTINUATION_TOLERANCE`], from the same left edge, and the same kind
/// (ruled or not).
fn continues(table: &ITable, next: &ITable) -> bool {
    if table.ruled != next.ruled || table.columns.len() != next.columns.len() {
        return false;
    }
    if table.ruled {
        return (table.bounds.0 - next.bounds.0).abs() <= CONTINUATION_TOLERANCE
            && table
                .columns
                .iter()
                .zip(&next.columns)
                .all(|(a, b)| (a - b).abs() <= CONTINUATION_TOLERANCE);
    }
    // A whitespace table's columns are its text's extents on that page (a
    // centred column's text moves with its content): each column of one
    // must overlap the same column of the other.
    let spans = |t: &ITable| {
        let mut start = t.bounds.0;
        t.columns
            .iter()
            .map(|w| {
                let span = (start, start + w);
                start += w;
                span
            })
            .collect::<Vec<_>>()
    };
    spans(table)
        .iter()
        .zip(spans(next))
        .all(|(a, b)| a.0.max(b.0) < a.1.min(b.1))
}

/// Whether `next` opens with the header row `table` opened with, over as
/// many columns: the strongest evidence of a continued table, whatever the
/// columns' measured edges (a whitespace table's are its text's).
fn repeats_header(table: &ITable, next: &ITable) -> bool {
    table.ruled == next.ruled
        && table.columns.len() == next.columns.len()
        && !table.rows.is_empty()
        && !next.rows.is_empty()
        && row_text(&table.rows[0]) == row_text(&next.rows[0])
        && row_text(&table.rows[0])
            .iter()
            .filter(|t| !t.is_empty())
            .count()
            >= 2
}

/// Joins each table a page break cut in two: a table that is the last
/// element of its page, followed in reading order by a table that is the
/// first of the next page with the same columns, takes that table's rows.
/// When the continuation's first row repeats the table's first row (a header
/// the producer repeated), the repeat is dropped and the first row becomes a
/// header row that repeats on every page. Returns how many joins were made.
/// O(elements + rows).
pub fn continue_across_pages(elements: &mut Vec<Element>, work: &mut u64) -> u32 {
    let mut joined = 0;
    let mut out: Vec<Element> = Vec::with_capacity(elements.len());
    // Per element of `out`, the last page it reaches.
    let mut through: Vec<usize> = Vec::with_capacity(elements.len());
    for element in elements.drain(..) {
        *work += 1;
        let page = element.page;
        // The text or table before it: a picture or drawing in between (a
        // shading run the table did not take, a logo) does not separate a
        // table from its continuation.
        let previous = out
            .iter()
            .rposition(|e| matches!(e.kind, EKind::Text(_) | EKind::Table(_)));
        let joins = match (previous, &element.kind) {
            (Some(k), EKind::Table(next)) => {
                through[k] + 1 == page
                    && matches!(&out[k].kind, EKind::Table(table) if continues(table, next)
                        || repeats_header(table, next))
            }
            _ => false,
        };
        let Some(k) = previous.filter(|_| joins) else {
            through.push(page);
            out.push(element);
            continue;
        };
        let EKind::Table(mut next) = element.kind else {
            unreachable!("checked above")
        };
        let EKind::Table(table) = &mut out[k].kind else {
            unreachable!("checked above")
        };
        let repeated = !table.rows.is_empty()
            && !next.rows.is_empty()
            && row_text(&table.rows[0]) == row_text(&next.rows[0])
            && row_text(&table.rows[0]).iter().any(|t| !t.is_empty());
        if repeated {
            next.rows.remove(0);
            table.header_rows = table.header_rows.max(1);
        }
        *work += next.rows.len() as u64;
        // The bounds stay the first page's: they place the table on it.
        table.rows.append(&mut next.rows);
        through[k] = page;
        joined += 1;
    }
    *elements = out;
    joined
}

/// An aligned row: its baseline, its largest em size, and its cells (left,
/// right, element) left to right.
type AlignedRow = (f64, f64, Vec<(f64, f64, usize)>);

/// Cells of an aligned row are at least this many ems apart: closer pieces
/// on a baseline are one line written as several text objects.
pub const CELL_GUTTER_EMS: f64 = 0.6;
/// Two aligned rows are of one table when their baselines are at most this
/// many ems apart (a cell's wrapped line between them included).
pub const ROW_REACH: f64 = 4.0;
/// Cell edges within this many points are one column edge.
pub const COLUMN_TOLERANCE: f64 = 4.0;

/// Whether two aligned rows' cells line up as one table's columns.
///
/// - As many cells: each cell sits in the gutter-bounded slot of the cell
///   above it (it overlaps no other cell of the other row and lies between
///   that cell's neighbours), so a header left-aligned over a centred or
///   right-aligned column still agrees.
/// - Fewer cells (merged or empty cells): every cell of the shorter row
///   starts, ends or centres on a cell of the other.
///
/// At least two cells either way. O(cells²), a handful.
#[must_use]
pub fn columns_agree(upper: &[(f64, f64, usize)], lower: &[(f64, f64, usize)]) -> bool {
    let near = |a: f64, b: f64| (a - b).abs() <= COLUMN_TOLERANCE;
    if upper.len() == lower.len() && upper.len() >= 2 {
        return upper.iter().zip(lower).enumerate().all(|(i, (a, b))| {
            let left_ok = i == 0 || (a.0 > lower[i - 1].1 && b.0 > upper[i - 1].1);
            let right_ok = i + 1 == upper.len() || (a.1 < lower[i + 1].0 && b.1 < upper[i + 1].0);
            left_ok && right_ok
        });
    }
    let (fewer, more) = if upper.len() <= lower.len() {
        (upper, lower)
    } else {
        (lower, upper)
    };
    let edge = |a: &(f64, f64, usize)| {
        more.iter()
            .any(|b| near(a.0, b.0) || near(a.1, b.1) || near((a.0 + a.1) / 2.0, (b.0 + b.1) / 2.0))
    };
    fewer.len() >= 2 && fewer.iter().all(edge)
}

/// A whitespace-table candidate: the blocks it links and its confidence.
#[derive(Clone, Debug)]
pub struct Candidate {
    /// Element indices, left to right.
    pub elements: Vec<usize>,
    /// The score.
    pub confidence: f64,
    /// Aligned rows.
    pub rows: usize,
    /// Tall rows whose cells share no baseline (one at the row's top, one
    /// in its middle, one at its bottom): the baseline of each such row's
    /// highest line, for [`whitespace_table`] to make a row of.
    pub tall_rows: Vec<f64>,
}

/// A tall row reaches at most this many ems below the aligned row above it.
pub const TALL_ROW_EMS: f64 = 12.0;

/// Scores whitespace-table candidates among text elements (each a list of
/// lines with its bounds). O(l log l).
#[must_use]
pub fn whitespace_candidates(
    elements: &[(&[ILine], (f64, f64))],
    work: &mut u64,
) -> Vec<Candidate> {
    let mut all: Vec<(f64, usize, usize)> = Vec::new();
    for (e, (lines, _)) in elements.iter().enumerate() {
        for (l, line) in lines.iter().enumerate() {
            all.push((line.baseline, e, l));
        }
    }
    all.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    *work += all.len() as u64;
    let mut uf = UnionFind((0..elements.len()).collect());
    let mut aligned_lines = vec![Vec::<bool>::new(); elements.len()];
    for (e, (lines, _)) in elements.iter().enumerate() {
        aligned_lines[e] = vec![false; lines.len()];
    }
    let mut rows_of: BTreeMap<usize, usize> = BTreeMap::new();
    // Every aligned row: its baseline, its largest em size and its cells
    // (left, right, element), left to right.
    let mut aligned_rows: Vec<AlignedRow> = Vec::new();
    let mut start = 0;
    while start < all.len() {
        let mut end = start + 1;
        while end < all.len() && all[end].0 - all[start].0 <= BASELINE_TOLERANCE {
            end += 1;
        }
        let group = &all[start..end];
        let mut distinct: Vec<usize> = group.iter().map(|g| g.1).collect();
        distinct.dedup();
        if distinct.len() >= 2 {
            // Disjoint horizontally (side by side), each pair in x order.
            let mut by_x: Vec<(f64, f64, usize, usize)> = group
                .iter()
                .map(|&(_, e, l)| (elements[e].0[l].x0, elements[e].0[l].x1, e, l))
                .collect();
            by_x.sort_by(|a, b| a.0.total_cmp(&b.0));
            // Side by side with a gutter between: pieces of one line that a
            // producer wrote as separate text objects (Chromium writes every
            // link and styled run as its own) abut, and are not cells.
            let em = group
                .iter()
                .map(|&(_, e, l)| elements[e].0[l].size)
                .fold(1.0, f64::max);
            let disjoint = by_x
                .windows(2)
                .all(|w| w[1].0 - w[0].1 >= CELL_GUTTER_EMS * em);
            if disjoint {
                for w in by_x.windows(2) {
                    uf.union(w[0].2, w[1].2);
                }
                for &(_, _, e, l) in &by_x {
                    aligned_lines[e][l] = true;
                }
                *rows_of.entry(by_x[0].2).or_default() += 1;
                let size = by_x
                    .iter()
                    .map(|&(_, _, e, l)| elements[e].0[l].size)
                    .fold(0.0, f64::max);
                aligned_rows.push((
                    group[0].0,
                    size,
                    by_x.iter().map(|&(x0, x1, e, _)| (x0, x1, e)).collect(),
                ));
            }
        }
        start = end;
    }
    // A row whose cells are each their own block (a table whose rows are
    // farther apart than a paragraph's leading, the usual case for Word and
    // LibreOffice tables, every cell one line) links to no other row by
    // baseline. Consecutive aligned rows whose columns line up are one table.
    for pair in aligned_rows.windows(2) {
        *work += 1;
        let ((b0, s0, upper), (b1, _, lower)) = (&pair[0], &pair[1]);
        if b1 - b0 <= ROW_REACH * s0.max(1.0) && columns_agree(upper, lower) {
            uf.union(upper[0].2, lower[0].2);
        }
    }
    // A tall row whose cells are aligned top, centre and bottom shares no
    // baseline, so it links nothing and its cells became a staircase of
    // paragraphs. Under an aligned row (before the next one, within
    // TALL_ROW_EMS), the single-line elements standing side by side, a
    // gutter apart, whose columns agree with the row above are one row.
    let mut tall: Vec<(usize, f64)> = Vec::new();
    for (k, (b0, s0, upper)) in aligned_rows.iter().enumerate() {
        *work += 1;
        let limit = aligned_rows
            .get(k + 1)
            .map_or(b0 + TALL_ROW_EMS * s0.max(1.0), |next| next.0)
            .min(b0 + TALL_ROW_EMS * s0.max(1.0));
        let mut pieces: Vec<(f64, f64, usize, f64)> = elements
            .iter()
            .enumerate()
            .filter_map(|(e, (lines, _))| match lines {
                [line]
                    if !aligned_lines[e][0]
                        && line.baseline > b0 + BASELINE_TOLERANCE
                        && line.baseline < limit - BASELINE_TOLERANCE =>
                {
                    Some((line.x0, line.x1, e, line.baseline))
                }
                _ => None,
            })
            .collect();
        if pieces.len() < 2 {
            continue;
        }
        pieces.sort_by(|a, b| a.0.total_cmp(&b.0));
        let gutter = CELL_GUTTER_EMS * s0.max(1.0);
        let disjoint = pieces.windows(2).all(|w| w[1].0 - w[0].1 >= gutter);
        let cells: Vec<(f64, f64, usize)> = pieces.iter().map(|p| (p.0, p.1, p.2)).collect();
        // Every piece in its own column of the row above.
        let columned = cells.iter().all(|c| {
            upper
                .iter()
                .filter(|u| c.0 <= u.1 + COLUMN_SLACK && c.1 >= u.0 - COLUMN_SLACK)
                .count()
                == 1
        }) && {
            let mut used: Vec<usize> = cells
                .iter()
                .filter_map(|c| {
                    upper
                        .iter()
                        .position(|u| c.0 <= u.1 + COLUMN_SLACK && c.1 >= u.0 - COLUMN_SLACK)
                })
                .collect();
            used.dedup();
            used.len() == cells.len()
        };
        // Not one line: the pieces' baselines differ (else they are an
        // aligned row already).
        let staggered = pieces
            .iter()
            .any(|p| (p.3 - pieces[0].3).abs() > BASELINE_TOLERANCE);
        if disjoint && columned && staggered {
            for p in &pieces {
                uf.union(upper[0].2, p.2);
                aligned_lines[p.2][0] = true;
            }
            let top = pieces.iter().map(|p| p.3).fold(f64::INFINITY, f64::min);
            let lowest = pieces.iter().map(|p| p.3).fold(f64::NEG_INFINITY, f64::max);
            // The row under it, a row's reach below its lowest line and in
            // the same columns, is the same table.
            if let Some((b1, _, lower)) = aligned_rows.get(k + 1)
                && b1 - lowest <= ROW_REACH * s0.max(1.0)
                && columns_agree(upper, lower)
            {
                uf.union(upper[0].2, lower[0].2);
            }
            *rows_of.entry(pieces[0].2).or_default() += 1;
            tall.push((pieces[0].2, top));
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for e in 0..elements.len() {
        groups.entry(uf.find(e)).or_default().push(e);
    }
    let mut out = Vec::new();
    for members in groups.into_values() {
        if members.len() < 2 {
            continue;
        }
        let (mut total, mut aligned, mut prose) = (0usize, 0usize, 0usize);
        for &e in &members {
            for (l, line) in elements[e].0.iter().enumerate() {
                total += 1;
                aligned += usize::from(aligned_lines[e][l]);
                let words = line
                    .chars
                    .iter()
                    .map(|c| c.text.as_str())
                    .collect::<String>()
                    .split_whitespace()
                    .count();
                prose += usize::from(words >= PROSE_WORDS);
            }
        }
        let rows = members
            .iter()
            .map(|e| rows_of.get(e).copied().unwrap_or(0))
            .sum::<usize>();
        let share = aligned as f64 / total.max(1) as f64;
        let prose_share = prose as f64 / total.max(1) as f64;
        let confidence = share * (1.0 - prose_share) * (rows as f64 / 3.0).min(1.0);
        let mut elements_sorted = members.clone();
        elements_sorted.sort_by(|&a, &b| elements[a].1.0.total_cmp(&elements[b].1.0));
        let tall_rows = tall
            .iter()
            .filter(|(e, _)| members.contains(e))
            .map(|(_, b)| *b)
            .collect();
        out.push(Candidate {
            elements: elements_sorted,
            confidence,
            rows,
            tall_rows,
        });
    }
    out
}

/// Where a cell's `lines` sit in a tall row of `cells`: at its top (the
/// default, `None`), its bottom or its middle, against the band from the
/// row's highest line top to its lowest line bottom, within a third of a
/// line. O(lines in the row).
fn cell_valign(
    lines: &[ILine],
    cells: &[Vec<ILine>],
) -> Option<casual_doc_model::v1::CellVerticalAlignment> {
    use casual_doc_model::v1::CellVerticalAlignment;
    let all = cells.iter().flatten();
    let top = all.clone().map(|l| l.top).fold(f64::INFINITY, f64::min);
    let bottom = all.map(|l| l.bottom).fold(f64::NEG_INFINITY, f64::max);
    let own_top = lines.iter().map(|l| l.top).fold(f64::INFINITY, f64::min);
    let own_bottom = lines
        .iter()
        .map(|l| l.bottom)
        .fold(f64::NEG_INFINITY, f64::max);
    if !own_top.is_finite() {
        return None;
    }
    let slack = lines.iter().map(|l| l.bottom - l.top).fold(1.0, f64::max) / 3.0;
    if own_top - top <= slack {
        None
    } else if bottom - own_bottom <= slack {
        Some(CellVerticalAlignment::Bottom)
    } else if (((own_top + own_bottom) - (top + bottom)) / 2.0).abs() <= slack * 1.5 {
        Some(CellVerticalAlignment::Center)
    } else {
        None
    }
}

/// The shaded header band over a whitespace candidate, if there is one: one
/// single-line element (`elements[e]`, not among `members`) on a filled,
/// non-white rectangle that spans the candidate's columns, directly above
/// its first row (within [`ROW_REACH`] ems). Word and LibreOffice draw a
/// borderless table's merged header ("Merged header across three columns")
/// exactly so: a shaded row, one cell spanning every column, no rules. It is
/// the row that tells a two-row borderless table from two lines that happen
/// to share columns. Returns the element and the extent its cell spans (the
/// band across the candidate's columns: its text alone is only as wide as
/// the words, centred over one column). O(elements × fills).
#[must_use]
pub fn shaded_header(
    members: &[usize],
    elements: &[(&[ILine], (f64, f64))],
    fills: &[Fill],
    work: &mut u64,
) -> Option<(usize, (f64, f64))> {
    let lines = || members.iter().flat_map(|&e| elements[e].0.iter());
    let x0 = members
        .iter()
        .map(|&e| elements[e].1.0)
        .fold(f64::INFINITY, f64::min);
    let x1 = members
        .iter()
        .map(|&e| elements[e].1.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let first = lines().min_by(|a, b| a.baseline.total_cmp(&b.baseline))?;
    let (top, baseline, size) = (
        lines().map(|l| l.top).fold(f64::INFINITY, f64::min),
        first.baseline,
        first.size.max(1.0),
    );
    (0..elements.len())
        .filter(|e| !members.contains(e))
        .find(|&e| {
            *work += fills.len() as u64;
            let [line] = elements[e].0 else {
                return false;
            };
            line.bottom <= top + BASELINE_TOLERANCE
                && baseline - line.baseline <= ROW_REACH * size
                && fills.iter().any(|fill| {
                    let b = fill.bounds;
                    let [r, g, bl, a] = fill.color;
                    let white = r >= 250 && g >= 250 && bl >= 250;
                    !white
                        && a > 0
                        && b.x0 <= line.x0
                        && b.x1 >= line.x1
                        && b.y0 <= line.top + BASELINE_TOLERANCE
                        && b.y1 >= line.bottom - BASELINE_TOLERANCE
                        && b.x0 <= x0 + COLUMN_SLACK * 4.0
                        && b.x1 >= x1 - COLUMN_SLACK
                })
        })
        .map(|e| (e, (x0, x1)))
}

/// Gap (points) under which two cells' horizontal extents are one column:
/// a header left-aligned over a centred column sits a few points off it.
pub const COLUMN_SLACK: f64 = 6.0;

/// Builds the table for an accepted whitespace candidate.
///
/// Columns are clustered from the elements' horizontal extents, narrowest
/// first: an element overlapping (within [`COLUMN_SLACK`]) one column joins
/// it; one overlapping several is a cell spanning them; one overlapping none
/// starts a column. So a table whose every cell is its own block (Word's and
/// LibreOffice's, rows farther apart than a paragraph's leading) has as many
/// columns as the page shows, not one per cell. Rows are baselines at least
/// two cells share; a line that aligns with nothing continues the cell above
/// it in its column, so no text is dropped. O(e² + l log l) for `e` elements,
/// a table's cells.
#[must_use]
pub fn whitespace_table(columns: &[(Vec<ILine>, (f64, f64))], tall_rows: &[f64]) -> ITable {
    // Columns: (left, right), and per element (column, span).
    let mut order: Vec<usize> = (0..columns.len()).collect();
    order.sort_by(|&a, &b| {
        let wa = columns[a].1.1 - columns[a].1.0;
        let wb = columns[b].1.1 - columns[b].1.0;
        wa.total_cmp(&wb).then(a.cmp(&b))
    });
    let mut clusters: Vec<(f64, f64)> = Vec::new();
    let mut spanning: Vec<usize> = Vec::new();
    for &e in &order {
        let (x0, x1) = columns[e].1;
        let hits: Vec<usize> = (0..clusters.len())
            .filter(|&k| x0 <= clusters[k].1 + COLUMN_SLACK && x1 >= clusters[k].0 - COLUMN_SLACK)
            .collect();
        match hits.as_slice() {
            [] => clusters.push((x0, x1)),
            [k] => {
                clusters[*k].0 = clusters[*k].0.min(x0);
                clusters[*k].1 = clusters[*k].1.max(x1);
            }
            _ => spanning.push(e),
        }
    }
    clusters.sort_by(|a, b| a.0.total_cmp(&b.0));
    let place = |x0: f64, x1: f64| -> (usize, usize) {
        let hits: Vec<usize> = (0..clusters.len())
            .filter(|&k| x0 <= clusters[k].1 + COLUMN_SLACK && x1 >= clusters[k].0 - COLUMN_SLACK)
            .collect();
        match (hits.first(), hits.last()) {
            (Some(&a), Some(&b)) => (a, b - a + 1),
            _ => {
                // Between columns: the nearest by centre.
                let c = (x0 + x1) / 2.0;
                let k = (0..clusters.len())
                    .min_by(|&a, &b| {
                        let da = ((clusters[a].0 + clusters[a].1) / 2.0 - c).abs();
                        let db = ((clusters[b].0 + clusters[b].1) / 2.0 - c).abs();
                        da.total_cmp(&db)
                    })
                    .unwrap_or(0);
                (k, 1)
            }
        }
    };
    let mut all: Vec<f64> = columns
        .iter()
        .flat_map(|(lines, _)| lines.iter().map(|l| l.baseline))
        .collect();
    all.sort_by(f64::total_cmp);
    // A row is a baseline at least two cells share, or the line of a
    // single-line cell spanning several columns (a merged header row, the
    // only cell on its baseline).
    let mut baselines: Vec<f64> = Vec::new();
    let mut i = 0;
    while i < all.len() {
        let mut j = i + 1;
        while j < all.len() && all[j] - all[i] <= BASELINE_TOLERANCE {
            j += 1;
        }
        if j - i >= 2 {
            baselines.push(all[i]);
        }
        i = j;
    }
    for &e in &spanning {
        if let [line] = columns[e].0.as_slice()
            && !baselines
                .iter()
                .any(|b| (b - line.baseline).abs() <= BASELINE_TOLERANCE)
        {
            baselines.push(line.baseline);
        }
    }
    // A tall row is a row from its highest line down: its lower cells'
    // lines fall to it as the row above them.
    for &b in tall_rows {
        if !baselines
            .iter()
            .any(|x| (x - b).abs() <= BASELINE_TOLERANCE)
        {
            baselines.push(b);
        }
    }
    baselines.sort_by(f64::total_cmp);
    let width = clusters.len().max(1);
    let mut grid: Vec<Vec<Vec<ILine>>> = vec![vec![Vec::new(); width]; baselines.len()];
    let mut spans: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for (e, (lines, (x0, x1))) in columns.iter().enumerate() {
        let (c, span) = place(*x0, *x1);
        for line in lines {
            let at = baselines.partition_point(|b| *b < line.baseline - BASELINE_TOLERANCE);
            let row = if baselines
                .get(at)
                .is_some_and(|b| (b - line.baseline).abs() <= BASELINE_TOLERANCE)
            {
                at
            } else {
                at.saturating_sub(1)
            };
            if let Some(r) = grid.get_mut(row) {
                r[c.min(width - 1)].push(line.clone());
                if span > 1 && spanning.contains(&e) {
                    let entry = spans.entry((row, c)).or_insert(1);
                    *entry = (*entry).max(span);
                }
            }
        }
    }
    // Each row's band, top to bottom: from its lines' top to the next row's,
    // so a fill painted behind the row covers its cells.
    let mut bands: Vec<(f64, f64)> = grid
        .iter()
        .map(|cells| {
            let lines = cells.iter().flatten();
            (
                lines.clone().map(|l| l.top).fold(f64::INFINITY, f64::min),
                lines.map(|l| l.bottom).fold(f64::NEG_INFINITY, f64::max),
            )
        })
        .collect();
    for r in 0..bands.len().saturating_sub(1) {
        let middle = (bands[r].1 + bands[r + 1].0) / 2.0;
        if middle.is_finite() && middle > bands[r].1 {
            bands[r].1 = middle;
            bands[r + 1].0 = middle;
        }
    }
    // Each tall row's cells: at its top, in its middle or at its bottom.
    let valigns: Vec<Vec<Option<casual_doc_model::v1::CellVerticalAlignment>>> = grid
        .iter()
        .enumerate()
        .map(|(r, cells)| {
            let tall = tall_rows
                .iter()
                .any(|b| (b - baselines[r]).abs() <= BASELINE_TOLERANCE);
            cells
                .iter()
                .map(|lines| {
                    if tall {
                        cell_valign(lines, cells)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .collect();
    let mut rows: Vec<Vec<ICell>> = Vec::with_capacity(grid.len());
    for (r, cells) in grid.into_iter().enumerate() {
        let mut row = Vec::new();
        let mut cells = cells.into_iter().enumerate().peekable();
        while let Some((c, mut lines)) = cells.next() {
            let span = spans.get(&(r, c)).copied().unwrap_or(1).min(width - c);
            for _ in 1..span {
                if let Some((_, more)) = cells.next() {
                    lines.extend(more);
                }
            }
            let last = (c + span - 1).min(width - 1);
            let valign = valigns[r][c];
            row.push(ICell {
                lines: rejoin(lines),
                span: u32::try_from(span).unwrap_or(1),
                vmerge: None,
                x0: clusters.get(c).map_or(0.0, |k| k.0),
                x1: clusters.get(last).map_or(0.0, |k| k.1),
                y0: bands[r].0,
                y1: bands[r].1,
                valign,
                ..ICell::default()
            });
        }
        rows.push(row);
    }
    let left = clusters.first().map_or(0.0, |c| c.0);
    let right = clusters.last().map_or(0.0, |c| c.1);
    let mut widths: Vec<f64> = clusters.windows(2).map(|w| w[1].0 - w[0].0).collect();
    // The last column holds its widest text plus some slack: at exactly its
    // text's width, the substitute face's slightly wider text wrapped ("12
    // ms" became "12" over "ms" in a tab-leader list).
    if let Some(last) = clusters.last() {
        widths.push((last.1 - last.0).max(1.0) + CELL_MARGINS);
    }
    let top = columns
        .iter()
        .flat_map(|(l, _)| l.iter().map(|x| x.top))
        .fold(f64::INFINITY, f64::min);
    let bottom = columns
        .iter()
        .flat_map(|(l, _)| l.iter().map(|x| x.bottom))
        .fold(f64::NEG_INFINITY, f64::max);
    // The table reaches as far as its rows will when laid out: each row is
    // as tall as the step to the next row's baseline, the last as tall as
    // the step before it (`emit::pitch_height`). Ending it at the last
    // line's ink counted that row's slack twice below a borderless table,
    // once in the row and again as space above the next paragraph.
    let first_baseline = |row: &Vec<ICell>| {
        row.iter()
            .filter_map(|c| c.lines.first().map(|l| l.baseline))
            .reduce(f64::min)
    };
    let bottom = match rows.as_slice() {
        [.., before, last] => match (first_baseline(before), first_baseline(last)) {
            (Some(a), Some(b)) if b > a => {
                let last_top = last
                    .iter()
                    .flat_map(|c| c.lines.iter().map(|l| l.top))
                    .fold(f64::INFINITY, f64::min);
                let laid = last_top + (b - a);
                if laid.is_finite() && laid > bottom && laid - bottom < b - a {
                    laid
                } else {
                    bottom
                }
            }
            _ => bottom,
        },
        _ => bottom,
    };
    ITable {
        rows,
        columns: widths,
        ruled: false,
        bounds: (left, top, right, bottom),
        ..ITable::default()
    }
}
