//! The block/flow engine — turning a v1 [`Document`] into a shaped galley.
//!
//! This is the bridge from the semantic model to layout: each body paragraph's
//! inline runs (and their run properties) become [`StyledRun`]s, which the
//! [`LineShaper`] turns into positioned lines, yielding a [`BlockFragment`]. The
//! resulting galley is what the paginator ([`crate::paginate`]) slices into pages
//! and the renderer paints — so this closes the loop from imported DOCX to a
//! rendered page.
//!
//! Scope: body paragraphs (runs with size/color/weight/decoration, recursing
//! through hyperlink/revision/content-control wrappers) and **tables** (rows and
//! nested tables, cells flowed at their grid-column width). Inline drawings,
//! fields, block content controls, indents/tabs (`P1C-003b`), and cross-page
//! table splitting (`P1D-003`) are the following slices; unmapped inline nodes
//! contribute no text yet (never panic).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use casual_doc_model::v1::{
    Alignment, BlockNode, BorderEdge, Color, Document, FontRef, FontScheme, HeightRule, InlineNode,
    ParagraphProperties, RunProperties, Table, TableBorders, TableCell, TableLayout,
    TableRowProperties, ThemeFontRef,
};

use crate::block::{
    BlockFragment, BoxMetrics, BreakControl, CellBorders, CellFragment, ResolvedEdge,
};
use crate::incremental::{DirtySet, GalleyCache};
use crate::model::{ModelPos, ModelRange};
use crate::resolve::{FaceRequest, FontResolutionReport, FontResolver};
use crate::text::{Decoration, FontId, LineConstraints, LineShaper, StyledRun, TextAlignment};
use crate::units::Twip;

/// Context threaded through the flow: the font resolver, the document's theme
/// font scheme (needed to turn `w:rFonts@*Theme` slots into concrete families),
/// and the running font-resolution report.
struct FlowCtx<'a> {
    resolver: &'a FontResolver,
    scheme: Option<&'a FontScheme>,
    report: &'a mut FontResolutionReport,
}

/// Builds a galley of block fragments from a document's body, shaped to fit
/// `content_width` (the page content-area width, in twips). The font-resolution
/// report is discarded; call [`build_galley_with_report`] to inspect which
/// families were substituted.
#[must_use]
pub fn build_galley(
    document: &Document,
    shaper: &dyn LineShaper,
    content_width: Twip,
) -> Vec<BlockFragment> {
    build_galley_with_report(document, shaper, content_width).0
}

/// Builds a galley and returns the [`FontResolutionReport`] alongside it: the
/// whole-face substitutions and per-glyph coverage fallbacks performed while
/// resolving each run's declared family (`P1C-002b`), mirroring the importer's
/// compatibility-report pattern so substitution is surfaced, never silent.
#[must_use]
pub fn build_galley_with_report(
    document: &Document,
    shaper: &dyn LineShaper,
    content_width: Twip,
) -> (Vec<BlockFragment>, FontResolutionReport) {
    let resolver = FontResolver::new();
    let mut report = FontResolutionReport::new();
    let mut ctx = FlowCtx {
        resolver: &resolver,
        scheme: document.definitions().font_scheme.as_ref(),
        report: &mut report,
    };
    let galley = flow_blocks(document.body(), shaper, content_width, &mut ctx);
    (galley, report)
}

/// Builds the galley like [`build_galley`], but reuses the shaped lines of
/// unchanged paragraphs from `cache` instead of re-shaping them — the incremental
/// path (`43-…` §7.4 step 2). Only paragraphs whose content changed (their hash
/// differs) or whose node the transaction reported in `dirty` are handed to the
/// `shaper`; everything else is cloned from the cache. Shaping is the dominant
/// cost of building a galley, so this is what makes an edit `O(edit)` rather than
/// `O(document)`.
///
/// The result is identical to a fresh [`build_galley`] of the same document — the
/// cache changes only *how much shaping* happens, never the galley's content. The
/// cache is scoped to `content_width`; a width change transparently clears it.
///
/// Top-level body paragraphs are cached; tables (and their nested cell paragraphs)
/// are re-flowed each call, so a document dominated by large tables sees less
/// benefit — paragraph editing, the common case, is fully incremental.
#[must_use]
pub fn build_galley_cached(
    document: &Document,
    shaper: &dyn LineShaper,
    content_width: Twip,
    cache: &mut GalleyCache,
    dirty: &DirtySet,
) -> Vec<BlockFragment> {
    // The cache path resolves fonts exactly like the fresh path so a reused
    // fragment is byte-for-byte identical to a freshly built one; the resolved
    // face is folded into `paragraph_hash` (via `run.font`), so a cached fragment
    // can never carry a face resolved under different font inputs. The report is
    // discarded here, matching [`build_galley`]; callers wanting it use
    // [`build_galley_with_report`].
    let resolver = FontResolver::new();
    let mut report = FontResolutionReport::new();
    let mut ctx = FlowCtx {
        resolver: &resolver,
        scheme: document.definitions().font_scheme.as_ref(),
        report: &mut report,
    };
    cache.begin_build(content_width);
    let mut galley = Vec::new();
    for block in document.body() {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let mut runs = Vec::new();
                // Resolving here sets each run's resolved `font`, which
                // `paragraph_hash` hashes — so the cache key tracks the face.
                collect_runs(&paragraph.inlines, &mut runs, &mut ctx);
                let spacing = paragraph.properties.spacing.as_ref();
                let constraints = LineConstraints {
                    max_width: content_width,
                    rtl: false,
                    alignment: alignment(&paragraph.properties),
                    line_height_percent: spacing.and_then(|s| s.line_percent),
                };
                let box_metrics = box_metrics(&paragraph.properties);
                let break_control = break_control(&paragraph.properties);
                let hash = paragraph_hash(
                    paragraph.id,
                    &runs,
                    &constraints,
                    box_metrics,
                    break_control,
                );

                if let Some(fragment) = cache.reusable(paragraph.id, hash, dirty) {
                    galley.push(fragment.clone());
                    continue;
                }
                let range = ModelRange::new(
                    ModelPos::new(paragraph.id, 0),
                    ModelPos::new(paragraph.id, 0),
                );
                let lines = shaper.shape_paragraph(&runs, constraints, range);
                let fragment = BlockFragment::Paragraph {
                    id: paragraph.id,
                    lines,
                    box_metrics,
                    break_control,
                };
                cache.store(paragraph.id, hash, fragment.clone());
                galley.push(fragment);
            }
            BlockNode::Table(table) => {
                flow_table(table, shaper, content_width, &mut galley, &mut ctx)
            }
            BlockNode::Sdt(_) => {}
        }
    }
    galley
}

/// Hashes every input that determines a paragraph's shaped fragment: its node
/// identity, each run's text and styling, the wrap constraints, and the box/break
/// metrics. Because the hash is computed from the exact values fed to the shaper
/// and the fragment builder, a matching hash guarantees an identical fragment —
/// the [`GalleyCache`] reuse condition.
fn paragraph_hash(
    id: casual_doc_model::NodeId,
    runs: &[StyledRun<'_>],
    constraints: &LineConstraints,
    box_metrics: BoxMetrics,
    break_control: BreakControl,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    id.as_u128().hash(&mut hasher);
    for run in runs {
        run.text.hash(&mut hasher);
        run.font.0.hash(&mut hasher);
        run.size.0.hash(&mut hasher);
        run.bold.hash(&mut hasher);
        run.italic.hash(&mut hasher);
        run.letter_spacing.0.hash(&mut hasher);
        run.color.hash(&mut hasher);
        run.decoration.underline.hash(&mut hasher);
        run.decoration.strikethrough.hash(&mut hasher);
    }
    constraints.max_width.0.hash(&mut hasher);
    constraints.rtl.hash(&mut hasher);
    (constraints.alignment as u8).hash(&mut hasher);
    constraints.line_height_percent.hash(&mut hasher);
    box_metrics.space_before.0.hash(&mut hasher);
    box_metrics.space_after.0.hash(&mut hasher);
    box_metrics.indent_start.0.hash(&mut hasher);
    box_metrics.indent_end.0.hash(&mut hasher);
    break_control.page_break_before.hash(&mut hasher);
    break_control.keep_next.hash(&mut hasher);
    break_control.keep_lines.hash(&mut hasher);
    break_control.widow_control.hash(&mut hasher);
    hasher.finish()
}

/// Flows a sequence of block nodes (a body or a table cell) into shaped
/// fragments at `width`. Paragraphs shape to lines; tables expand to their rows;
/// block content controls are laid out in a later slice.
fn flow_blocks(
    blocks: &[BlockNode],
    shaper: &dyn LineShaper,
    width: Twip,
    ctx: &mut FlowCtx,
) -> Vec<BlockFragment> {
    let mut galley = Vec::new();
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let mut runs = Vec::new();
                collect_runs(&paragraph.inlines, &mut runs, ctx);
                let range = ModelRange::new(
                    ModelPos::new(paragraph.id, 0),
                    ModelPos::new(paragraph.id, 0),
                );
                let spacing = paragraph.properties.spacing.as_ref();
                let lines = shaper.shape_paragraph(
                    &runs,
                    LineConstraints {
                        max_width: width,
                        rtl: false,
                        alignment: alignment(&paragraph.properties),
                        line_height_percent: spacing.and_then(|s| s.line_percent),
                    },
                    range,
                );
                galley.push(BlockFragment::Paragraph {
                    id: paragraph.id,
                    lines,
                    box_metrics: box_metrics(&paragraph.properties),
                    break_control: break_control(&paragraph.properties),
                });
            }
            BlockNode::Table(table) => flow_table(table, shaper, width, &mut galley, ctx),
            BlockNode::Sdt(_) => {}
        }
    }
    galley
}

/// Flows a table into one [`BlockFragment::TableRow`] per row at Word-grade
/// fidelity (`P1D-003`):
///
/// - **Column widths** come from the width solver ([`solve_column_widths`]):
///   preferred `w:tblW`/`w:tcW` widths, the fixed-vs-autofit algorithm,
///   content-driven minimum/preferred widths, and `w:gridSpan` distribution.
/// - **Table indent** (`w:tblInd`) offsets every cell's leading edge and reduces
///   the width available to the columns.
/// - **Row height** honors the `w:trHeight` rule (`atLeast` grows with content,
///   `exact` fixes the height and clips overflow).
/// - **Borders** are resolved by OOXML conflict precedence so composition draws
///   the winning edge.
///
/// Cross-page row splitting and header repetition are the paginator's job
/// ([`crate::paginate`]); this produces the row fragments it slices.
fn flow_table(
    table: &Table,
    shaper: &dyn LineShaper,
    width: Twip,
    galley: &mut Vec<BlockFragment>,
    ctx: &mut FlowCtx,
) {
    let indent = table.properties.indent_twips.unwrap_or(0);
    let available = (width.raw() - indent).max(1);
    let widths = solve_table_columns(table, shaper, available, ctx);

    // Cumulative left edge of each column, shifted by the table indent.
    let ncols = widths.len();
    let mut edges = Vec::with_capacity(ncols + 1);
    let mut x = indent;
    for w in &widths {
        edges.push(x);
        x += w.raw();
    }
    edges.push(x);
    let edge = |col: usize| Twip(edges[col.min(edges.len() - 1)]);

    for row in &table.rows {
        let mut cells = Vec::new();
        let mut col = 0usize;
        for (index, cell) in row.cells.iter().enumerate() {
            let span = cell.properties.grid_span.unwrap_or(1).max(1) as usize;
            let cell_x = edge(col);
            let cell_end = edge(col + span);
            let cell_width = Twip((cell_end.raw() - cell_x.raw()).max(1));
            let borders = resolve_cell_borders(&table.properties.borders, &row.cells, index);
            cells.push(CellFragment {
                id: cell.id,
                grid_span: span as u32,
                x: cell_x,
                width: cell_width,
                blocks: flow_blocks(&cell.blocks, shaper, cell_width, ctx),
                borders,
            });
            col += span;
        }
        let content_h = BlockFragment::cells_content_height(&cells);
        let (height, clip) = resolve_row_height(&row.properties, content_h);
        galley.push(BlockFragment::TableRow {
            id: row.id,
            table: table.id,
            cells,
            height,
            can_split: !row.properties.cant_split,
            header: row.properties.header,
            clip,
        });
    }
}

/// The resolved height of a table row and whether its content must be clipped,
/// per the `w:trHeight` rule. `atLeast`/`auto` grow to the content when it is
/// taller; `exact` pins the height and clips overflow (`docs/38-…#tables`).
fn resolve_row_height(props: &TableRowProperties, content: Twip) -> (Twip, bool) {
    let val = props.height.value_twips.map_or(0, |v| v as i32);
    match props.height.rule {
        Some(HeightRule::Exact) if props.height.value_twips.is_some() => {
            (Twip(val), content.raw() > val)
        }
        // `atLeast`, `auto`, and an `exact` with no value all resolve to "at
        // least this tall": the larger of the content height and the stated value.
        _ => (Twip(content.raw().max(val)), false),
    }
}

/// A table column's width constraints, the input to [`solve_column_widths`].
#[derive(Clone, Copy, Debug)]
struct ColumnConstraint {
    /// Declared grid width (`w:gridCol`), if any.
    grid: Option<i32>,
    /// Minimum content width — the widest run that cannot be broken (twips).
    min: i32,
    /// Preferred content width — the natural (unwrapped) width (twips).
    preferred: i32,
}

/// A table's preferred-width specification (`w:tblW`). The v1 model stores
/// `w:tblW` as `dxa` only, so the flow path emits `Auto`/`Dxa`; `Pct` (resolved
/// against the containing width) is supported and tested at the solver seam so
/// the algorithm is complete when the model gains percentage widths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WidthSpec {
    /// No preferred width — size to content within the available width.
    Auto,
    /// A fixed width in twips.
    Dxa(i32),
    /// A percentage of the containing width, in fiftieths of a percent
    /// (`5000` = 100%). The v1 model stores `w:tblW` as `dxa` only, so the flow
    /// path never emits this today; the solver implements it (and it is covered
    /// by tests) so percentage widths are ready the moment the model carries them.
    #[allow(dead_code)]
    Pct(u32),
}

/// Solves the grid column widths for a table (twips), measuring each cell's
/// content min/preferred widths with the shaper and distributing `w:gridSpan`
/// cells across the columns they cover.
fn solve_table_columns(
    table: &Table,
    shaper: &dyn LineShaper,
    available: i32,
    ctx: &FlowCtx,
) -> Vec<Twip> {
    let ncols = if table.grid.is_empty() {
        table
            .rows
            .iter()
            .map(|r| {
                r.cells
                    .iter()
                    .map(|c| c.properties.grid_span.unwrap_or(1).max(1) as usize)
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(1)
            .max(1)
    } else {
        table.grid.len()
    };

    let mut cols: Vec<ColumnConstraint> = (0..ncols)
        .map(|i| ColumnConstraint {
            grid: table.grid.get(i).and_then(|g| g.width_twips),
            min: 0,
            preferred: 0,
        })
        .collect();

    for row in &table.rows {
        let mut col = 0usize;
        for cell in &row.cells {
            let span = cell.properties.grid_span.unwrap_or(1).max(1) as usize;
            let (cmin, cmax) = block_intrinsic(&cell.blocks, shaper, ctx);
            let cpref = cmax.max(cell.properties.width_twips.unwrap_or(0));
            // A spanning cell's demand is shared over the columns it covers.
            let per_min = div_ceil(cmin, span as i32);
            let per_pref = div_ceil(cpref, span as i32);
            for c in cols.iter_mut().skip(col).take(span) {
                c.min = c.min.max(per_min);
                c.preferred = c.preferred.max(per_pref);
            }
            col += span;
        }
    }
    for c in &mut cols {
        if let Some(g) = c.grid {
            c.preferred = c.preferred.max(g);
        }
        c.preferred = c.preferred.max(c.min);
    }

    let spec = table
        .properties
        .width_twips
        .map_or(WidthSpec::Auto, WidthSpec::Dxa);
    let layout = match table.properties.layout {
        Some(TableLayout::Fixed) => TableLayout::Fixed,
        _ => TableLayout::Autofit,
    };
    solve_column_widths(&cols, spec, available, layout)
        .into_iter()
        .map(Twip)
        .collect()
}

/// The column-width solver: resolves a table's target width from `spec`, seeds a
/// base width per column (the grid for fixed layout, the preferred content width
/// for autofit), then grows or shrinks the columns to hit the target while never
/// shrinking a column below its content minimum when the target allows.
fn solve_column_widths(
    cols: &[ColumnConstraint],
    spec: WidthSpec,
    available: i32,
    layout: TableLayout,
) -> Vec<i32> {
    let n = cols.len();
    if n == 0 {
        return Vec::new();
    }
    let available = available.max(1);
    let grid_sum: i32 = cols.iter().map(|c| c.grid.unwrap_or(0)).sum();
    let pref_sum: i32 = cols.iter().map(|c| c.preferred.max(1)).sum();
    let min_sum: i32 = cols.iter().map(|c| c.min.max(1)).sum();

    let target = match spec {
        WidthSpec::Dxa(v) => v.max(1),
        WidthSpec::Pct(p) => ((i64::from(available) * i64::from(p)) / 5000).max(1) as i32,
        WidthSpec::Auto => match layout {
            TableLayout::Fixed if grid_sum > 0 => grid_sum,
            TableLayout::Fixed => available,
            // Autofit sizes to the content, but never wider than the available
            // width nor narrower than the content minimum.
            TableLayout::Autofit => pref_sum.clamp(min_sum, available.max(min_sum)),
        },
    };

    let base: Vec<i32> = match layout {
        TableLayout::Fixed => {
            let undeclared = cols.iter().filter(|c| c.grid.is_none()).count();
            let each = if undeclared > 0 {
                ((available - grid_sum).max(0) / undeclared as i32).max(1)
            } else {
                0
            };
            cols.iter().map(|c| c.grid.unwrap_or(each).max(1)).collect()
        }
        TableLayout::Autofit => cols.iter().map(|c| c.preferred.max(1)).collect(),
    };
    let mins: Vec<i32> = cols.iter().map(|c| c.min.max(1)).collect();
    distribute_width(base, &mins, target)
}

/// Distributes `base` column widths to sum to `target`: extra space is shared in
/// proportion to each column's base width; a deficit is taken first from each
/// column's slack above its minimum (in proportion to that slack), and only if
/// that is not enough are columns shrunk below their minimums proportionally.
fn distribute_width(base: Vec<i32>, mins: &[i32], target: i32) -> Vec<i32> {
    let n = base.len();
    let sum: i32 = base.iter().sum();
    let mut out = base.clone();
    if n == 0 || sum == target {
        for v in &mut out {
            *v = (*v).max(1);
        }
        return out;
    }
    if sum < target {
        let extra = target - sum;
        let wsum: i64 = base.iter().map(|&b| i64::from(b.max(1))).sum();
        let mut given = 0;
        for i in 0..n {
            let add = if i == n - 1 {
                extra - given
            } else {
                ((i64::from(extra) * i64::from(base[i].max(1))) / wsum) as i32
            };
            out[i] += add;
            given += add;
        }
    } else {
        let deficit = sum - target;
        let slack: Vec<i32> = (0..n).map(|i| (base[i] - mins[i]).max(0)).collect();
        let slack_sum: i64 = slack.iter().map(|&s| i64::from(s)).sum();
        if slack_sum >= i64::from(deficit) && slack_sum > 0 {
            let last = (0..n).rev().find(|&i| slack[i] > 0).unwrap();
            let mut taken = 0;
            for i in 0..n {
                if slack[i] == 0 {
                    continue;
                }
                let sub = if i == last {
                    deficit - taken
                } else {
                    ((i64::from(deficit) * i64::from(slack[i])) / slack_sum) as i32
                };
                out[i] -= sub;
                taken += sub;
            }
        } else {
            let wsum: i64 = base.iter().map(|&b| i64::from(b.max(1))).sum();
            let mut taken = 0;
            for i in 0..n {
                let sub = if i == n - 1 {
                    deficit - taken
                } else {
                    ((i64::from(deficit) * i64::from(base[i].max(1))) / wsum) as i32
                };
                out[i] -= sub;
                taken += sub;
            }
        }
    }
    for v in &mut out {
        *v = (*v).max(1);
    }
    out
}

/// Ceiling of `a / b` for non-negative integers (`b >= 1`).
fn div_ceil(a: i32, b: i32) -> i32 {
    (a + b - 1) / b
}

/// A cell's intrinsic content widths `(min, preferred)` in twips: `min` is the
/// widest run that cannot be line-broken (measured by shaping at a 1-twip width);
/// `preferred` is the natural, unwrapped width (shaping at an effectively
/// unbounded width). Nested tables contribute their declared grid width.
///
/// Runs are resolved to their concrete face (so intrinsic widths are measured
/// with the advances actually shaped) but into a throwaway report — measurement
/// is internal and must not inflate the substitution counts surfaced for the
/// rendered galley, which the flow pass records once per run.
fn block_intrinsic(blocks: &[BlockNode], shaper: &dyn LineShaper, ctx: &FlowCtx) -> (i32, i32) {
    let mut scratch = FontResolutionReport::new();
    let mut mctx = FlowCtx {
        resolver: ctx.resolver,
        scheme: ctx.scheme,
        report: &mut scratch,
    };
    let mut min = 0;
    let mut preferred = 0;
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let mut runs = Vec::new();
                collect_runs(&paragraph.inlines, &mut runs, &mut mctx);
                if runs.is_empty() {
                    continue;
                }
                let range = ModelRange::new(
                    ModelPos::new(paragraph.id, 0),
                    ModelPos::new(paragraph.id, 0),
                );
                let narrow = shaper.shape_paragraph(
                    &runs,
                    LineConstraints {
                        max_width: Twip(1),
                        ..LineConstraints::default()
                    },
                    range,
                );
                let wide = shaper.shape_paragraph(
                    &runs,
                    LineConstraints {
                        max_width: Twip(1_000_000),
                        ..LineConstraints::default()
                    },
                    range,
                );
                min = min.max(max_line_width(&narrow));
                preferred = preferred.max(max_line_width(&wide));
            }
            BlockNode::Table(table) => {
                let grid: i32 = table.grid.iter().filter_map(|c| c.width_twips).sum();
                min = min.max(grid);
                preferred = preferred.max(grid);
            }
            BlockNode::Sdt(_) => {}
        }
    }
    (min, preferred)
}

/// The widest line in a shaped paragraph (twips) — a run's right edge is its
/// origin plus the sum of its glyph advances.
fn max_line_width(layout: &crate::text::LineLayout) -> i32 {
    layout
        .lines
        .iter()
        .map(|line| {
            line.runs
                .iter()
                .map(|run| {
                    run.origin.x.raw() + run.glyphs.iter().map(|g| g.advance.raw()).sum::<i32>()
                })
                .max()
                .unwrap_or(0)
        })
        .max()
        .unwrap_or(0)
}

/// Resolves a cell's four visible borders by OOXML conflict precedence
/// (ECMA-376 §17.4.66): each edge is the winner among the cell's own border, the
/// abutting neighbor cell's border, and the table-level border for that edge.
/// Interior edges use `w:insideH`/`w:insideV`; outer edges use the table's outer
/// borders. `None` = no explicit border (the default grid line is drawn).
fn resolve_cell_borders(table: &TableBorders, row: &[TableCell], index: usize) -> CellBorders {
    let own = &row[index].properties.borders;
    let left = index.checked_sub(1).map(|i| &row[i].properties.borders);
    let right = row.get(index + 1).map(|c| &c.properties.borders);
    let is_first = index == 0;
    let is_last = index + 1 == row.len();

    let start_default = if is_first {
        table.start.as_ref()
    } else {
        table.inside_v.as_ref()
    };
    let end_default = if is_last {
        table.end.as_ref()
    } else {
        table.inside_v.as_ref()
    };
    CellBorders {
        top: resolve_edge(&[
            own.top.as_ref(),
            table.top.as_ref(),
            table.inside_h.as_ref(),
        ]),
        bottom: resolve_edge(&[
            own.bottom.as_ref(),
            table.bottom.as_ref(),
            table.inside_h.as_ref(),
        ]),
        start: resolve_edge(&[
            own.start.as_ref(),
            left.and_then(|b| b.end.as_ref()),
            start_default,
        ]),
        end: resolve_edge(&[
            own.end.as_ref(),
            right.and_then(|b| b.start.as_ref()),
            end_default,
        ]),
    }
}

/// Picks the highest-precedence border among `candidates` and converts it to a
/// drawable [`ResolvedEdge`] (or `None` if none is a visible border).
fn resolve_edge(candidates: &[Option<&BorderEdge>]) -> Option<ResolvedEdge> {
    let winner = candidates
        .iter()
        .filter_map(|c| *c)
        .filter(|e| is_visible_border(e))
        .max_by(|a, b| border_rank(a).cmp(&border_rank(b)))?;
    let color = winner
        .color
        .map_or([0, 0, 0, 255], |c| [c.r, c.g, c.b, 255]);
    // `w:sz` is in eighths of a point; a point is 20 twips.
    let width = winner
        .size_eighth_points
        .map_or(Twip(10), |sz| Twip(((sz * 20) / 8).max(1) as i32));
    Some(ResolvedEdge { color, width })
}

/// Whether a border edge is a visible line (not `nil`/`none`/empty).
fn is_visible_border(edge: &BorderEdge) -> bool {
    !matches!(edge.style.as_str(), "" | "nil" | "none")
}

/// The border-conflict ranking key (higher wins): a visible border beats none,
/// then wider beats narrower, then a higher style rank, then a darker color.
fn border_rank(edge: &BorderEdge) -> (u32, u32, u32) {
    let width = edge.size_eighth_points.unwrap_or(0);
    let style: u32 = match edge.style.as_str() {
        "double" => 3,
        "single" => 2,
        "dashed" | "dotted" | "dotDash" | "dashDotStroked" => 1,
        _ => 0,
    };
    // Darker colors win ties: rank by inverse luminance (absent color = black).
    let luminance = edge
        .color
        .map_or(0, |c| u32::from(c.r) + u32::from(c.g) + u32::from(c.b));
    (width, style, 765 - luminance)
}

/// Flattens a paragraph's inline nodes into styled text runs, recursing through
/// the wrappers that carry inline content (hyperlinks, revisions, content
/// controls). Text-bearing runs and explicit tabs contribute text; other inline
/// nodes are not yet laid out.
fn collect_runs<'a>(inlines: &'a [InlineNode], out: &mut Vec<StyledRun<'a>>, ctx: &mut FlowCtx) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => out.push(styled_run(&run.text, &run.properties, ctx)),
            InlineNode::Tab(_) => out.push(styled_run("\t", &RunProperties::default(), ctx)),
            InlineNode::Hyperlink(hyperlink) => collect_runs(&hyperlink.inlines, out, ctx),
            InlineNode::Revision(revision) => collect_runs(&revision.inlines, out, ctx),
            InlineNode::Sdt(sdt) => collect_runs(&sdt.inlines, out, ctx),
            _ => {}
        }
    }
}

/// Maps a run's text + properties to a styled run, resolving its declared font
/// family (`w:rFonts`, direct or theme) to a concrete bundled face via the
/// [`FontResolver`] and recording any substitution/coverage fallback (`P1C-002b`).
fn styled_run<'a>(text: &'a str, properties: &RunProperties, ctx: &mut FlowCtx) -> StyledRun<'a> {
    // `w:sz` is in half-points; a half-point is 10 twips (a point is 20). Default
    // to 11pt (Word's default body size) when unset.
    let size = properties
        .size_half_points
        .map_or(Twip::from_points(11), |hp| Twip(hp as i32 * 10));
    let color = match properties.color {
        Some(Color::Rgb(rgb)) => [rgb.r, rgb.g, rgb.b, 255],
        _ => [0, 0, 0, 255],
    };
    let bold = properties.bold.unwrap_or(false);
    let italic = properties.italic.unwrap_or(false);
    StyledRun {
        text,
        // Resolve the declared family to a concrete face so the renderer outlines
        // the same face `parley` shapes with.
        font: resolve_font(text, properties, bold, italic, ctx),
        size,
        bold,
        italic,
        letter_spacing: properties.character_spacing_twips.map_or(Twip::ZERO, Twip),
        color,
        decoration: Decoration {
            underline: properties.underline.unwrap_or(false),
            strikethrough: properties.strike.unwrap_or(false),
        },
    }
}

/// Resolves a run's declared font family to a bundled face, records any
/// substitution and per-glyph coverage fallback, and returns the chosen face. A
/// run with no declared family uses the bundled default (matching weight/style).
fn resolve_font(
    text: &str,
    properties: &RunProperties,
    bold: bool,
    italic: bool,
    ctx: &mut FlowCtx,
) -> FontId {
    let face = match requested_family(properties, ctx.scheme) {
        Some(family) => {
            let outcome = ctx.resolver.resolve(&FaceRequest {
                family: &family,
                bold,
                italic,
            });
            ctx.report.note_resolution(&family, &outcome);
            outcome.face
        }
        None => crate::fonts::face_id(bold, italic),
    };
    ctx.resolver.record_coverage(face, text, ctx.report);
    face
}

/// The concrete family name a run requests through its ascii `w:rFonts` slot,
/// resolving a theme reference against the document's font scheme. `None` when the
/// run declares no family (it inherits the default).
fn requested_family(properties: &RunProperties, scheme: Option<&FontScheme>) -> Option<String> {
    match properties.font_ref.as_ref()? {
        FontRef::Named(name) => Some(name.name.clone()),
        FontRef::Theme(theme) => theme_family(theme.slot, scheme),
    }
}

/// Which per-script entry of a theme font collection a slot resolves against.
enum ThemeAxis {
    Latin,
    EastAsia,
    ComplexScript,
}

/// Resolves a `w:rFonts@*Theme` slot to a concrete typeface via the theme font
/// scheme (design §3.4): `*Ascii`/`*HAnsi` → latin, `*EastAsia` → ea, `*Bidi` →
/// cs, with an empty ea/cs typeface falling back to the latin entry.
fn theme_family(slot: ThemeFontRef, scheme: Option<&FontScheme>) -> Option<String> {
    let scheme = scheme?;
    let (collection, axis) = match slot {
        ThemeFontRef::MajorAscii | ThemeFontRef::MajorHAnsi => (&scheme.major, ThemeAxis::Latin),
        ThemeFontRef::MajorEastAsia => (&scheme.major, ThemeAxis::EastAsia),
        ThemeFontRef::MajorBidi => (&scheme.major, ThemeAxis::ComplexScript),
        ThemeFontRef::MinorAscii | ThemeFontRef::MinorHAnsi => (&scheme.minor, ThemeAxis::Latin),
        ThemeFontRef::MinorEastAsia => (&scheme.minor, ThemeAxis::EastAsia),
        ThemeFontRef::MinorBidi => (&scheme.minor, ThemeAxis::ComplexScript),
    };
    let entry = match axis {
        ThemeAxis::Latin => &collection.latin,
        ThemeAxis::EastAsia => &collection.ea,
        ThemeAxis::ComplexScript => &collection.cs,
    };
    let typeface = if entry.typeface.is_empty() {
        &collection.latin.typeface
    } else {
        &entry.typeface
    };
    (!typeface.is_empty()).then(|| typeface.clone())
}

/// Maps model paragraph alignment to the layout alignment.
fn alignment(properties: &ParagraphProperties) -> TextAlignment {
    match properties.alignment {
        Some(Alignment::Start) | None => TextAlignment::Start,
        Some(Alignment::End) => TextAlignment::End,
        Some(Alignment::Center) => TextAlignment::Center,
        Some(Alignment::Justify) => TextAlignment::Justify,
    }
}

/// Maps paragraph break properties to the fragment's break control.
fn break_control(properties: &ParagraphProperties) -> BreakControl {
    BreakControl {
        page_break_before: properties.page_break_before,
        keep_next: properties.keep_next,
        keep_lines: properties.keep_lines,
        widow_control: properties.widow_control,
    }
}

/// Maps paragraph spacing/indent to the fragment's box metrics.
fn box_metrics(properties: &ParagraphProperties) -> BoxMetrics {
    let spacing = properties.spacing.as_ref();
    let indent = properties.indentation.as_ref();
    BoxMetrics {
        space_before: spacing
            .and_then(|s| s.before_twips)
            .map_or(Twip::ZERO, Twip),
        space_after: spacing.and_then(|s| s.after_twips).map_or(Twip::ZERO, Twip),
        indent_start: indent.and_then(|i| i.start_twips).map_or(Twip::ZERO, Twip),
        indent_end: indent.and_then(|i| i.end_twips).map_or(Twip::ZERO, Twip),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::ParleyShaper;
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::{
        BlockNode, Definitions, Document, InlineNode, Paragraph, ParagraphProperties, Run,
        RunProperties,
    };

    fn run_node(id: u64, text: &str, properties: RunProperties) -> InlineNode {
        InlineNode::Run(Run {
            id: NodeId::from_parts(id, 1).unwrap(),
            properties,
            text: text.to_owned(),
        })
    }

    fn paragraph(id: u64, inlines: Vec<InlineNode>) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(id, 1).unwrap(),
            properties: ParagraphProperties::default(),
            inlines,
        })
    }

    fn document(body: Vec<BlockNode>) -> Document {
        Document::new(
            NodeId::from_parts(1, 1).unwrap(),
            body,
            Definitions::default(),
        )
        .unwrap()
    }

    #[test]
    fn a_table_flows_to_a_row_fragment_with_positioned_cells() {
        use casual_doc_model::v1::{
            GridColumn, Table, TableCell, TableCellProperties, TableProperties, TableRow,
            TableRowProperties,
        };
        let cell = |id: u64, text: &str| TableCell {
            id: NodeId::from_parts(id, 1).unwrap(),
            properties: TableCellProperties::default(),
            blocks: vec![paragraph(
                id + 100,
                vec![run_node(id + 200, text, RunProperties::default())],
            )],
        };
        let table = BlockNode::Table(Table {
            id: NodeId::from_parts(50, 1).unwrap(),
            grid: vec![
                GridColumn {
                    width_twips: Some(3000),
                },
                GridColumn {
                    width_twips: Some(3000),
                },
            ],
            properties: TableProperties::default(),
            rows: vec![TableRow {
                id: NodeId::from_parts(51, 1).unwrap(),
                properties: TableRowProperties::default(),
                cells: vec![cell(60, "left cell"), cell(61, "right cell")],
            }],
        });
        let shaper = ParleyShaper::new();
        let galley = build_galley(&document(vec![table]), &shaper, Twip::from_points(400));
        assert_eq!(galley.len(), 1, "the table flows to one row fragment");
        let BlockFragment::TableRow { cells, .. } = &galley[0] else {
            panic!("expected a table row");
        };
        assert_eq!(cells.len(), 2);
        // First cell at x=0 width 3000; second at x=3000.
        assert_eq!(cells[0].x, Twip::ZERO);
        assert_eq!(cells[0].width, Twip(3000));
        assert_eq!(cells[1].x, Twip(3000));
        // Each cell shaped its paragraph.
        assert!(!cells[0].blocks.is_empty() && !cells[1].blocks.is_empty());
    }

    #[test]
    fn builds_a_shaped_fragment_per_paragraph() {
        let doc = document(vec![
            paragraph(
                10,
                vec![run_node(11, "First paragraph.", RunProperties::default())],
            ),
            paragraph(
                20,
                vec![run_node(
                    21,
                    "Second one, a bit longer.",
                    RunProperties::default(),
                )],
            ),
        ]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        assert_eq!(galley.len(), 2, "one fragment per paragraph");
        for fragment in &galley {
            let BlockFragment::Paragraph { lines, .. } = fragment else {
                panic!("expected a paragraph fragment");
            };
            assert!(!lines.lines.is_empty(), "the paragraph shaped to lines");
            assert!(fragment.height().raw() > 0, "positive height");
        }
    }

    #[test]
    fn run_size_and_color_flow_into_the_shaped_run() {
        use casual_doc_model::v1::{Color, RgbColor};
        let props = RunProperties {
            size_half_points: Some(48), // 24pt
            color: Some(Color::Rgb(RgbColor {
                r: 200,
                g: 60,
                b: 20,
            })),
            ..RunProperties::default()
        };
        let doc = document(vec![paragraph(10, vec![run_node(11, "Big red", props)])]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!();
        };
        let run = &lines.lines[0].runs[0];
        assert_eq!(run.color, [200, 60, 20, 255], "run color flows to layout");
        assert!(
            run.size.raw() >= Twip::from_points(20).raw(),
            "24pt size flows through"
        );
    }

    #[test]
    fn hyperlink_and_revision_text_is_collected() {
        use casual_doc_model::v1::{
            Hyperlink, HyperlinkTarget, InternalTarget, Revision, RevisionKind,
        };
        let link = InlineNode::Hyperlink(Hyperlink {
            id: NodeId::from_parts(30, 1).unwrap(),
            target: HyperlinkTarget::Internal(InternalTarget {
                anchor: "a".to_owned(),
            }),
            tooltip: None,
            inlines: vec![run_node(31, "linked", RunProperties::default())],
        });
        let rev = InlineNode::Revision(Revision {
            id: NodeId::from_parts(40, 1).unwrap(),
            kind: RevisionKind::Insertion,
            author: None,
            date: None,
            revision_id: None,
            inlines: vec![run_node(41, " inserted", RunProperties::default())],
        });
        let doc = document(vec![paragraph(10, vec![link, rev])]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!();
        };
        let glyphs: usize = lines
            .lines
            .iter()
            .flat_map(|l| &l.runs)
            .map(|r| r.glyphs.len())
            .sum();
        assert!(
            glyphs >= 12,
            "hyperlink + revision text both shaped (got {glyphs})"
        );
    }

    fn named_font(name: &str) -> RunProperties {
        use casual_doc_model::v1::{FontName, FontRef};
        RunProperties {
            font_ref: Some(FontRef::Named(FontName {
                name: name.to_owned(),
            })),
            ..RunProperties::default()
        }
    }

    // --- Table layout fidelity (P1D-003) --------------------------------------

    use casual_doc_model::v1::{
        BorderEdge, GridColumn, HeightRule, RowHeight, Table, TableBorders, TableCell,
        TableCellProperties, TableProperties, TableRow as ModelRow, TableRowProperties,
    };

    fn node(id: u64) -> NodeId {
        NodeId::from_parts(id, 1).unwrap()
    }

    fn text_cell(id: u64, props: TableCellProperties, text: &str) -> TableCell {
        TableCell {
            id: node(id),
            properties: props,
            blocks: vec![paragraph(
                id + 1000,
                vec![run_node(id + 2000, text, RunProperties::default())],
            )],
        }
    }

    fn edge(style: &str, sz: u32) -> BorderEdge {
        BorderEdge {
            style: style.to_owned(),
            size_eighth_points: Some(sz),
            color: None,
            space_points: None,
        }
    }

    /// Builds a single-row table galley and returns the row fragment's cells.
    fn flow_single_row(table: Table, width: Twip) -> BlockFragment {
        let shaper = ParleyShaper::new();
        let mut galley = build_galley(&document(vec![BlockNode::Table(table)]), &shaper, width);
        galley.remove(0)
    }

    // --- column-width solver (pure) ---

    #[test]
    fn solver_honors_a_preferred_table_width_and_distributes_the_extra() {
        let cols = [
            ColumnConstraint {
                grid: Some(2000),
                min: 100,
                preferred: 2000,
            },
            ColumnConstraint {
                grid: Some(2000),
                min: 100,
                preferred: 2000,
            },
        ];
        let w = solve_column_widths(&cols, WidthSpec::Dxa(8000), 10_000, TableLayout::Autofit);
        assert_eq!(
            w.iter().sum::<i32>(),
            8000,
            "columns sum to the preferred width"
        );
        assert_eq!(w, vec![4000, 4000], "the extra space is shared evenly");
    }

    #[test]
    fn solver_resolves_a_percentage_width_against_the_available_width() {
        let cols = [
            ColumnConstraint {
                grid: None,
                min: 1,
                preferred: 3000,
            },
            ColumnConstraint {
                grid: None,
                min: 1,
                preferred: 3000,
            },
        ];
        // 100% (5000 fiftieths) of a 10_000-twip content width.
        let w = solve_column_widths(&cols, WidthSpec::Pct(5000), 10_000, TableLayout::Autofit);
        assert_eq!(w.iter().sum::<i32>(), 10_000, "the table fills the width");
        // 50% resolves to half.
        let half = solve_column_widths(&cols, WidthSpec::Pct(2500), 10_000, TableLayout::Autofit);
        assert_eq!(half.iter().sum::<i32>(), 5000);
    }

    #[test]
    fn solver_keeps_a_column_at_its_minimum_content_width() {
        // Target narrower than the preferred sum: the deficit is taken from slack,
        // never shrinking a column below its content minimum.
        let cols = [
            ColumnConstraint {
                grid: None,
                min: 2500,
                preferred: 3000,
            },
            ColumnConstraint {
                grid: None,
                min: 200,
                preferred: 3000,
            },
        ];
        let w = solve_column_widths(&cols, WidthSpec::Dxa(4000), 10_000, TableLayout::Autofit);
        assert_eq!(w.iter().sum::<i32>(), 4000);
        assert!(
            w[0] >= 2500,
            "the narrow-min column keeps its minimum: {w:?}"
        );
    }

    #[test]
    fn solver_fixed_layout_uses_the_grid_verbatim() {
        let cols = [
            ColumnConstraint {
                grid: Some(3000),
                min: 100,
                preferred: 3000,
            },
            ColumnConstraint {
                grid: Some(5000),
                min: 100,
                preferred: 5000,
            },
        ];
        let w = solve_column_widths(&cols, WidthSpec::Auto, 12_000, TableLayout::Fixed);
        assert_eq!(w, vec![3000, 5000], "fixed layout keeps the declared grid");
    }

    // --- flow integration ---

    #[test]
    fn preferred_cell_width_grows_its_column() {
        // No grid (autofit derives the column count); cell 0 asks for a wide `w:tcW`.
        let table = Table {
            id: node(50),
            grid: Vec::new(),
            properties: TableProperties::default(),
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties::default(),
                cells: vec![
                    text_cell(
                        60,
                        TableCellProperties {
                            width_twips: Some(4000),
                            ..TableCellProperties::default()
                        },
                        "a",
                    ),
                    text_cell(61, TableCellProperties::default(), "b"),
                ],
            }],
        };
        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a row");
        };
        assert!(
            cells[0].width.raw() >= 3500,
            "the preferred tcW width is honored: {}",
            cells[0].width.raw()
        );
        assert!(
            cells[0].width.raw() > cells[1].width.raw(),
            "the wide-preferred column is wider than its neighbor"
        );
    }

    #[test]
    fn a_narrow_column_grows_to_fit_its_content() {
        // A tiny declared grid column whose cell holds a long unbreakable word:
        // autofit must grow the column to at least that word's width.
        let long_word = "Supercalifragilisticexpialidocious";
        let table = Table {
            id: node(50),
            grid: vec![
                GridColumn {
                    width_twips: Some(200),
                },
                GridColumn {
                    width_twips: Some(4000),
                },
            ],
            properties: TableProperties::default(),
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties::default(),
                cells: vec![
                    text_cell(60, TableCellProperties::default(), long_word),
                    text_cell(61, TableCellProperties::default(), "x"),
                ],
            }],
        };
        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a row");
        };
        assert!(
            cells[0].width.raw() > 200,
            "the narrow column grew past its tiny declared width: {}",
            cells[0].width.raw()
        );
    }

    #[test]
    fn grid_span_spans_the_covered_columns() {
        let table = Table {
            id: node(50),
            grid: vec![
                GridColumn {
                    width_twips: Some(3000),
                },
                GridColumn {
                    width_twips: Some(3000),
                },
            ],
            properties: TableProperties::default(),
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties::default(),
                cells: vec![text_cell(
                    60,
                    TableCellProperties {
                        grid_span: Some(2),
                        ..TableCellProperties::default()
                    },
                    "spanning header",
                )],
            }],
        };
        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a row");
        };
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].grid_span, 2);
        assert_eq!(cells[0].x, Twip::ZERO);
        assert_eq!(cells[0].width, Twip(6000), "the cell spans both columns");
    }

    #[test]
    fn table_indent_offsets_the_cells() {
        let table = Table {
            id: node(50),
            grid: vec![
                GridColumn {
                    width_twips: Some(3000),
                },
                GridColumn {
                    width_twips: Some(3000),
                },
            ],
            properties: TableProperties {
                indent_twips: Some(1000),
                ..TableProperties::default()
            },
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties::default(),
                cells: vec![
                    text_cell(60, TableCellProperties::default(), "a"),
                    text_cell(61, TableCellProperties::default(), "b"),
                ],
            }],
        };
        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a row");
        };
        assert_eq!(cells[0].x, Twip(1000), "w:tblInd offsets the first cell");
        assert_eq!(cells[1].x, Twip(4000), "the second cell follows the indent");
    }

    fn one_cell_table(props: TableRowProperties, text: &str) -> Table {
        Table {
            id: node(50),
            grid: vec![GridColumn {
                width_twips: Some(3000),
            }],
            properties: TableProperties::default(),
            rows: vec![ModelRow {
                id: node(51),
                properties: props,
                cells: vec![text_cell(60, TableCellProperties::default(), text)],
            }],
        }
    }

    #[test]
    fn a_cambria_run_resolves_to_the_caladea_face_and_is_reported() {
        use crate::fonts::CALADEA;
        use crate::resolve::Disposition;
        let doc = document(vec![paragraph(
            10,
            vec![run_node(11, "Body text", named_font("Cambria"))],
        )]);
        let shaper = ParleyShaper::new();
        let (galley, report) = build_galley_with_report(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!();
        };
        assert_eq!(
            lines.lines[0].runs[0].font,
            CALADEA.face_id(false, false),
            "Cambria shapes and renders as the Caladea face"
        );
        let subs: Vec<_> = report.substitutions().collect();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].resolved_family, "Caladea");
        assert_eq!(subs[0].disposition, Disposition::MetricCompatible);
    }

    #[test]
    fn an_unknown_family_is_reported_as_a_fallback() {
        use crate::resolve::Disposition;
        let doc = document(vec![paragraph(
            10,
            vec![run_node(11, "text", named_font("No Such Font"))],
        )]);
        let shaper = ParleyShaper::new();
        let (_galley, report) = build_galley_with_report(&doc, &shaper, Twip::from_points(400));
        let subs: Vec<_> = report.substitutions().collect();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].requested, "No Such Font");
        assert_eq!(subs[0].disposition, Disposition::Fallback);
    }

    #[test]
    fn a_run_without_a_declared_family_reports_nothing() {
        let doc = document(vec![paragraph(
            10,
            vec![run_node(11, "plain", RunProperties::default())],
        )]);
        let shaper = ParleyShaper::new();
        let (_galley, report) = build_galley_with_report(&doc, &shaper, Twip::from_points(400));
        assert!(
            report.is_empty(),
            "an undeclared family is not a substitution"
        );
    }

    #[test]
    fn at_least_row_height_grows_to_the_stated_minimum() {
        let props = TableRowProperties {
            height: RowHeight {
                value_twips: Some(6000),
                rule: Some(HeightRule::AtLeast),
            },
            ..TableRowProperties::default()
        };
        let row = flow_single_row(one_cell_table(props, "short"), Twip(9000));
        assert_eq!(
            row.height(),
            Twip(6000),
            "atLeast holds the row to its stated height when content is shorter"
        );
    }

    #[test]
    fn exact_row_height_is_fixed_and_clips_overflow() {
        let props = TableRowProperties {
            height: RowHeight {
                value_twips: Some(300),
                rule: Some(HeightRule::Exact),
            },
            ..TableRowProperties::default()
        };
        // A long sentence wraps to several lines in a 3000-twip column, exceeding
        // the 300-twip exact height.
        let table = one_cell_table(
            props,
            "This is a fairly long sentence that wraps across several lines in a narrow column.",
        );
        let BlockFragment::TableRow { height, clip, .. } = flow_single_row(table, Twip(9000))
        else {
            panic!("expected a row");
        };
        assert_eq!(height, Twip(300), "exact pins the row height");
        assert!(clip, "content taller than an exact row is clipped");
    }

    #[test]
    fn border_conflict_resolution_picks_the_higher_precedence_edge() {
        // Two adjacent cells share an edge: cell 0's end is a thin single line,
        // cell 1's start is a thick double line. The thicker/double border wins on
        // both sides of the shared edge.
        let cell0 = TableCell {
            id: node(60),
            properties: TableCellProperties {
                borders: TableBorders {
                    end: Some(edge("single", 4)),
                    ..TableBorders::default()
                },
                ..TableCellProperties::default()
            },
            blocks: vec![paragraph(
                160,
                vec![run_node(260, "a", RunProperties::default())],
            )],
        };
        let cell1 = TableCell {
            id: node(61),
            properties: TableCellProperties {
                borders: TableBorders {
                    start: Some(edge("double", 24)),
                    ..TableBorders::default()
                },
                ..TableCellProperties::default()
            },
            blocks: vec![paragraph(
                161,
                vec![run_node(261, "b", RunProperties::default())],
            )],
        };
        let table = Table {
            id: node(50),
            grid: vec![
                GridColumn {
                    width_twips: Some(3000),
                },
                GridColumn {
                    width_twips: Some(3000),
                },
            ],
            properties: TableProperties::default(),
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties::default(),
                cells: vec![cell0, cell1],
            }],
        };
        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a row");
        };
        // sz 24 eighth-points = 24*20/8 = 60 twips.
        let winner = ResolvedEdge {
            color: [0, 0, 0, 255],
            width: Twip(60),
        };
        assert_eq!(
            cells[0].borders.end,
            Some(winner),
            "the shared edge shows the double border on cell 0"
        );
        assert_eq!(
            cells[1].borders.start,
            Some(winner),
            "and the same winner on cell 1"
        );
    }

    #[test]
    fn a_stronger_table_border_overrides_a_missing_cell_border() {
        // No cell border, but a table outer border: the cell inherits it.
        let table = Table {
            id: node(50),
            grid: vec![GridColumn {
                width_twips: Some(3000),
            }],
            properties: TableProperties {
                borders: TableBorders {
                    top: Some(edge("single", 8)),
                    ..TableBorders::default()
                },
                ..TableProperties::default()
            },
            rows: vec![ModelRow {
                id: node(51),
                properties: TableRowProperties::default(),
                cells: vec![text_cell(60, TableCellProperties::default(), "a")],
            }],
        };
        let BlockFragment::TableRow { cells, .. } = flow_single_row(table, Twip(9000)) else {
            panic!("expected a row");
        };
        let top = cells[0].borders.top.expect("the table top border applies");
        assert_eq!(top.width, Twip(20), "sz 8 eighth-points = 20 twips");
    }
}
