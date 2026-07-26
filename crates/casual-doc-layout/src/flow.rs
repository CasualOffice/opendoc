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

use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Alignment, BlockNode, BorderEdge, BreakKind, Color, ColorScheme, DefinitionMap, Document,
    Drawing, Extent, FontRef, FontScheme, HeightRule, HighlightColor, InlineNode, MediaId,
    MediaReference, ParagraphProperties, RunProperties, SchemeColor, TabAlignment, TabLeader,
    TabStop, Table, TableBorders, TableCell, TableLayout, TableRowProperties, TextBox,
    ThemeColorRef, ThemeFontRef, VerticalAlignment,
};

use crate::block::{
    BlockBorders, BlockFragment, BoxMetrics, BreakControl, CellBorders, CellFragment,
    ParagraphDecor, ResolvedEdge,
};
use crate::incremental::{DirtySet, GalleyCache};
use crate::model::{ModelPos, ModelRange};
use crate::resolve::{FaceRequest, FontResolutionReport, FontResolver};
use crate::tabs::{self, FlowItem};
use crate::text::{
    Decoration, FieldKind, FieldMarker, FieldStyle, FontId, Glyph, GlyphRun, InlineImage,
    InlineTextBox, Line, LineBreak, LineConstraints, LineLayout, LineShaper, StyledRun,
    TextAlignment,
};
use crate::units::{Point, Size, Twip};

/// Context threaded through the flow: the font resolver, the document's theme
/// font scheme (needed to turn `w:rFonts@*Theme` slots into concrete families),
/// and the running font-resolution report.
struct FlowCtx<'a> {
    resolver: &'a FontResolver,
    scheme: Option<&'a FontScheme>,
    report: &'a mut FontResolutionReport,
    /// The document's default tab-stop interval (`w:defaultTabStop`), already
    /// resolved to twips (falling back to the 720-twip standard).
    default_tab: Twip,
    /// The document's media table, so an inline drawing's [`MediaId`] resolves to
    /// its package part name (the display list's stable media key).
    media: &'a DefinitionMap<MediaId, MediaReference>,
    /// The document's theme color palette (`a:clrScheme`) resolved to RGBA per
    /// slot, so a run's `w:themeColor` resolves to the real theme color rather than
    /// silently falling back to black. `None` when the document declares no theme
    /// color scheme.
    palette: Option<&'a ResolvedPalette>,
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
    // Resolve the theme color palette once so every run's `w:themeColor` resolves
    // against the real scheme colors (not a black fallback).
    let palette = document
        .definitions()
        .color_scheme
        .as_ref()
        .map(resolve_palette);
    let mut ctx = FlowCtx {
        resolver: &resolver,
        scheme: document.definitions().font_scheme.as_ref(),
        report: &mut report,
        default_tab: tabs::default_tab_stop(document.definitions().settings.default_tab_stop),
        media: &document.definitions().media,
        palette: palette.as_ref(),
    };
    let galley = flow_blocks(document.body(), shaper, content_width, &mut ctx);
    (galley, report)
}

/// Flows a header's or footer's block content into a galley of fragments at
/// `content_width` (the header/footer band width, normally the body content
/// width), reusing the document's font scheme, media table, and default tab stop.
///
/// This is the running-content counterpart to [`build_galley`]: the returned
/// fragments are laid out into the page's header/footer band by
/// [`crate::running::place_running_content`], and any `PAGE`/`NUMPAGES` field they
/// contain is resolved by [`crate::paginate::resolve_fields`]. `blocks` is a
/// [`casual_doc_model::v1::HeaderFooter::blocks`] list.
#[must_use]
pub fn flow_header_footer(
    document: &Document,
    blocks: &[BlockNode],
    shaper: &dyn LineShaper,
    content_width: Twip,
) -> Vec<BlockFragment> {
    let resolver = FontResolver::new();
    let mut report = FontResolutionReport::new();
    // Resolve the theme color palette once so every run's `w:themeColor` resolves
    // against the real scheme colors (not a black fallback).
    let palette = document
        .definitions()
        .color_scheme
        .as_ref()
        .map(resolve_palette);
    let mut ctx = FlowCtx {
        resolver: &resolver,
        scheme: document.definitions().font_scheme.as_ref(),
        report: &mut report,
        default_tab: tabs::default_tab_stop(document.definitions().settings.default_tab_stop),
        media: &document.definitions().media,
        palette: palette.as_ref(),
    };
    flow_blocks(blocks, shaper, content_width, &mut ctx)
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
    // Resolve the theme color palette once so every run's `w:themeColor` resolves
    // against the real scheme colors (not a black fallback).
    let palette = document
        .definitions()
        .color_scheme
        .as_ref()
        .map(resolve_palette);
    let mut ctx = FlowCtx {
        resolver: &resolver,
        scheme: document.definitions().font_scheme.as_ref(),
        report: &mut report,
        default_tab: tabs::default_tab_stop(document.definitions().settings.default_tab_stop),
        media: &document.definitions().media,
        palette: palette.as_ref(),
    };
    cache.begin_build(content_width);
    let mut galley = Vec::new();
    for block in document.body() {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let mut items = Vec::new();
                // Resolving here sets each run's resolved `font`, which
                // `paragraph_hash` hashes — so the cache key tracks the face.
                collect_items(
                    &paragraph.inlines,
                    &mut items,
                    shaper,
                    content_width,
                    &mut ctx,
                );
                let shape = ShapeInputs {
                    items: &items,
                    tab_stops: &paragraph.properties.tabs,
                    default_tab: ctx.default_tab,
                    constraints: line_constraints(&paragraph.properties, content_width),
                };
                let box_metrics = box_metrics(&paragraph.properties);
                let break_control = break_control(&paragraph.properties);
                let decor = paragraph_decor(&paragraph.properties, content_width);
                // A paragraph carrying an inline text box is never cached: the box's
                // flowed fragments are not folded into the paragraph hash, so a
                // reuse could serve stale nested content. Text boxes are rare, so
                // always reshaping them is the correct, simple choice.
                let has_text_box = items.iter().any(|i| matches!(i, FlowItem::TextBox { .. }));
                let hash = paragraph_hash(paragraph.id, &shape, box_metrics, break_control, decor);

                if !has_text_box && let Some(fragment) = cache.reusable(paragraph.id, hash, dirty) {
                    galley.push(fragment.clone());
                    continue;
                }
                let range = ModelRange::new(
                    ModelPos::new(paragraph.id, 0),
                    ModelPos::new(paragraph.id, 0),
                );
                let lines = shape_paragraph_items(
                    shaper,
                    shape.items,
                    shape.tab_stops,
                    shape.default_tab,
                    shape.constraints,
                    range,
                );
                let fragment = BlockFragment::Paragraph {
                    id: paragraph.id,
                    lines,
                    box_metrics,
                    break_control,
                    decor,
                };
                if !has_text_box {
                    cache.store(paragraph.id, hash, fragment.clone());
                }
                galley.push(fragment);
            }
            BlockNode::Table(table) => {
                flow_table(table, shaper, content_width, &mut galley, &mut ctx)
            }
            BlockNode::Sdt(_) => {}
            // An alt chunk's aggregated external content is not laid out here.
            BlockNode::AltChunk(_) => {}
        }
    }
    galley
}

/// The inputs that determine a paragraph's shaped lines: its flattened item
/// stream (runs + tabs + breaks), the tab-stop context, and the wrap constraints.
/// Bundled so the hash and the shaper read the exact same values.
struct ShapeInputs<'a> {
    items: &'a [FlowItem<'a>],
    tab_stops: &'a [TabStop],
    default_tab: Twip,
    constraints: LineConstraints,
}

/// A stable per-value key for a run of tab-stop alignment/leader/break enums that
/// do not derive `Hash` (so they can feed the cache key).
const fn tab_alignment_key(alignment: TabAlignment) -> u8 {
    match alignment {
        TabAlignment::Start => 0,
        TabAlignment::Center => 1,
        TabAlignment::End => 2,
        TabAlignment::Decimal => 3,
        TabAlignment::Bar => 4,
    }
}

/// A stable per-value key for a tab leader (see [`tab_alignment_key`]).
const fn tab_leader_key(leader: Option<TabLeader>) -> u8 {
    match leader {
        None => 0,
        Some(TabLeader::Dot) => 1,
        Some(TabLeader::Hyphen) => 2,
        Some(TabLeader::Underscore) => 3,
        Some(TabLeader::MiddleDot) => 4,
        Some(TabLeader::Heavy) => 5,
    }
}

/// A stable per-value key for a hard-break kind (see [`tab_alignment_key`]).
const fn break_kind_key(kind: BreakKind) -> u8 {
    match kind {
        BreakKind::Line => 0,
        BreakKind::Page => 1,
        BreakKind::Column => 2,
    }
}

/// Hashes every input that determines a paragraph's shaped fragment: its node
/// identity, each run's text and styling, the tab/break structure and tab-stop
/// context, the wrap constraints, and the box/break metrics. Because the hash is
/// computed from the exact values fed to the shaper and the fragment builder, a
/// matching hash guarantees an identical fragment — the [`GalleyCache`] reuse
/// condition.
fn paragraph_hash(
    id: casual_doc_model::NodeId,
    shape: &ShapeInputs<'_>,
    box_metrics: BoxMetrics,
    break_control: BreakControl,
    decor: ParagraphDecor,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    id.as_u128().hash(&mut hasher);
    for item in shape.items {
        match item {
            FlowItem::Run(run) => {
                0u8.hash(&mut hasher);
                run.text.hash(&mut hasher);
                run.font.0.hash(&mut hasher);
                run.size.0.hash(&mut hasher);
                run.bold.hash(&mut hasher);
                run.italic.hash(&mut hasher);
                run.letter_spacing.0.hash(&mut hasher);
                run.color.hash(&mut hasher);
                run.decoration.underline.hash(&mut hasher);
                run.decoration.strikethrough.hash(&mut hasher);
                run.highlight.hash(&mut hasher);
                run.baseline_shift.0.hash(&mut hasher);
            }
            FlowItem::Tab => 1u8.hash(&mut hasher),
            FlowItem::Break(kind) => {
                2u8.hash(&mut hasher);
                break_kind_key(*kind).hash(&mut hasher);
            }
            FlowItem::Image { media, size } => {
                3u8.hash(&mut hasher);
                media.hash(&mut hasher);
                size.width.0.hash(&mut hasher);
                size.height.0.hash(&mut hasher);
            }
            FlowItem::Field { kind, value, style } => {
                4u8.hash(&mut hasher);
                (*kind as u8).hash(&mut hasher);
                value.hash(&mut hasher);
                style.font.0.hash(&mut hasher);
                style.size.0.hash(&mut hasher);
                style.color.hash(&mut hasher);
                style.bold.hash(&mut hasher);
                style.italic.hash(&mut hasher);
                style.letter_spacing.0.hash(&mut hasher);
                style.decoration.underline.hash(&mut hasher);
                style.decoration.strikethrough.hash(&mut hasher);
            }
            // A text box's flowed fragments are not hashed here; a paragraph
            // carrying one bypasses the galley cache entirely (see
            // `build_galley_cached`), so this arm only keeps the match exhaustive.
            FlowItem::TextBox {
                size, border, fill, ..
            } => {
                5u8.hash(&mut hasher);
                size.width.0.hash(&mut hasher);
                size.height.0.hash(&mut hasher);
                border.hash(&mut hasher);
                fill.hash(&mut hasher);
            }
        }
    }
    for stop in shape.tab_stops {
        stop.position_twips.hash(&mut hasher);
        tab_alignment_key(stop.alignment).hash(&mut hasher);
        tab_leader_key(stop.leader).hash(&mut hasher);
    }
    shape.default_tab.0.hash(&mut hasher);
    let constraints = &shape.constraints;
    constraints.max_width.0.hash(&mut hasher);
    constraints.rtl.hash(&mut hasher);
    (constraints.alignment as u8).hash(&mut hasher);
    constraints.line_height_percent.hash(&mut hasher);
    constraints.first_line_indent.0.hash(&mut hasher);
    box_metrics.space_before.0.hash(&mut hasher);
    box_metrics.space_after.0.hash(&mut hasher);
    box_metrics.indent_start.0.hash(&mut hasher);
    box_metrics.indent_end.0.hash(&mut hasher);
    break_control.page_break_before.hash(&mut hasher);
    break_control.keep_next.hash(&mut hasher);
    break_control.keep_lines.hash(&mut hasher);
    break_control.widow_control.hash(&mut hasher);
    decor.shading.hash(&mut hasher);
    decor.width.0.hash(&mut hasher);
    for e in [
        decor.borders.top,
        decor.borders.bottom,
        decor.borders.start,
        decor.borders.end,
    ]
    .into_iter()
    .flatten()
    {
        e.color.hash(&mut hasher);
        e.width.0.hash(&mut hasher);
    }
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
                let mut items = Vec::new();
                collect_items(&paragraph.inlines, &mut items, shaper, width, ctx);
                let range = ModelRange::new(
                    ModelPos::new(paragraph.id, 0),
                    ModelPos::new(paragraph.id, 0),
                );
                let lines = shape_paragraph_items(
                    shaper,
                    &items,
                    &paragraph.properties.tabs,
                    ctx.default_tab,
                    line_constraints(&paragraph.properties, width),
                    range,
                );
                galley.push(BlockFragment::Paragraph {
                    id: paragraph.id,
                    lines,
                    box_metrics: box_metrics(&paragraph.properties),
                    break_control: break_control(&paragraph.properties),
                    decor: paragraph_decor(&paragraph.properties, width),
                });
            }
            BlockNode::Table(table) => flow_table(table, shaper, width, &mut galley, ctx),
            BlockNode::Sdt(_) => {}
            // An alt chunk's aggregated external content is not laid out here.
            BlockNode::AltChunk(_) => {}
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
            let shading = cell.properties.shading.fill.map(|c| [c.r, c.g, c.b, 255]);
            cells.push(CellFragment {
                id: cell.id,
                grid_span: span as u32,
                x: cell_x,
                width: cell_width,
                blocks: flow_blocks(&cell.blocks, shaper, cell_width, ctx),
                borders,
                shading,
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
        default_tab: ctx.default_tab,
        media: ctx.media,
        palette: ctx.palette,
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
            // An alt chunk contributes no laid-out width.
            BlockNode::AltChunk(_) => {}
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
///
/// A run marked hidden (`w:vanish`) is skipped entirely: it is neither shaped nor
/// painted, matching Word's screen view. The flow layer reads direct run
/// properties (it does not resolve style inheritance here), so an inherited
/// `w:vanish` from a character/paragraph style is honored once style resolution
/// lands upstream.
fn collect_runs<'a>(inlines: &'a [InlineNode], out: &mut Vec<StyledRun<'a>>, ctx: &mut FlowCtx) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) if run.properties.hidden == Some(true) => {}
            InlineNode::Run(run) => push_styled_runs(&run.text, &run.properties, ctx, out),
            InlineNode::Tab(_) => out.push(styled_run("\t", &RunProperties::default(), ctx)),
            InlineNode::Hyperlink(hyperlink) => collect_runs(&hyperlink.inlines, out, ctx),
            InlineNode::Revision(revision) => collect_runs(&revision.inlines, out, ctx),
            InlineNode::Sdt(sdt) => collect_runs(&sdt.inlines, out, ctx),
            _ => {}
        }
    }
}

/// Flattens a paragraph's inline nodes into a [`FlowItem`] stream — styled runs
/// interleaved with the explicit tabs (`w:tab`) and hard breaks (`w:br`/`w:cr`)
/// that control horizontal advance and forced lines. Unlike [`collect_runs`],
/// tabs and breaks are preserved as first-class items so the tab/break layer can
/// resolve them; recursion through wrappers matches [`collect_runs`].
fn collect_items<'a>(
    inlines: &'a [InlineNode],
    out: &mut Vec<FlowItem<'a>>,
    shaper: &dyn LineShaper,
    width: Twip,
    ctx: &mut FlowCtx,
) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) if run.properties.hidden == Some(true) => {}
            InlineNode::Run(run) => {
                let mut styled = Vec::new();
                push_styled_runs(&run.text, &run.properties, ctx, &mut styled);
                out.extend(styled.into_iter().map(FlowItem::Run));
            }
            InlineNode::Tab(_) => out.push(FlowItem::Tab),
            InlineNode::Break(node) => out.push(FlowItem::Break(node.kind)),
            InlineNode::Drawing(drawing) => {
                if let Some(item) = image_item(drawing, ctx) {
                    out.push(item);
                }
            }
            InlineNode::Field(field) => out.push(field_item(field, ctx)),
            InlineNode::TextBox(text_box) => out.push(textbox_item(text_box, shaper, width, ctx)),
            InlineNode::Hyperlink(hyperlink) => {
                collect_items(&hyperlink.inlines, out, shaper, width, ctx)
            }
            InlineNode::Revision(revision) => {
                collect_items(&revision.inlines, out, shaper, width, ctx)
            }
            InlineNode::Sdt(sdt) => collect_items(&sdt.inlines, out, shaper, width, ctx),
            _ => {}
        }
    }
}

/// The default border color (opaque black RGBA) drawn around an inline text box —
/// Word's default text-box outline. Applied until the model carries the shape's
/// own line/fill properties.
const TEXTBOX_DEFAULT_BORDER: [u8; 4] = [0, 0, 0, 255];

/// Flows an inline text box (`wps:txbx` / `v:textbox`) into an [`FlowItem::TextBox`]:
/// its recursive block content is laid out through the **same** [`flow_blocks`]
/// pipeline the document body uses (so it supports paragraphs, tables incl. nested,
/// inline images, borders/shading — the uniform-flow-pipeline invariant), and it
/// works identically in headers/footers/cells because they share this flow.
///
/// The v1 model does not carry the DrawingML extent, so the box is sized to the
/// available column `width`: its content flows at that width minus the internal
/// margins on both sides, and the box height is the flowed content height plus the
/// top/bottom margins. A default hairline border is applied so the box is visible
/// (Word's default text-box outline); an explicit extent and border/fill from the
/// shape's properties are a follow-up once the model carries them. Anchored /
/// floating placement reuses the anchored-drawing path (`P1F-28`) in a later slice.
fn textbox_item(
    text_box: &TextBox,
    shaper: &dyn LineShaper,
    width: Twip,
    ctx: &mut FlowCtx,
) -> FlowItem<'static> {
    let inset = crate::compose::TEXTBOX_INSET;
    let inner_width = Twip((width.raw() - 2 * inset.raw()).max(1));
    let blocks = flow_blocks(&text_box.blocks, shaper, inner_width, ctx);
    let content_height = blocks
        .iter()
        .map(BlockFragment::height)
        .fold(Twip::ZERO, |a, h| a + h);
    let size = Size::new(
        width.max(Twip(1)),
        Twip(content_height.raw() + 2 * inset.raw()),
    );
    FlowItem::TextBox {
        blocks,
        size,
        border: Some(TEXTBOX_DEFAULT_BORDER),
        fill: None,
    }
}

/// Maps an inline field to a [`FlowItem::Field`]: classifies its instruction
/// (`PAGE`/`NUMPAGES`/other), shapes a placeholder value from its cached result,
/// and captures the run styling so the post-pagination field pass can reshape a
/// recomputed value. A field is laid out inline like a run; page-dependent fields
/// carry only a marker through pagination (never a baked number).
fn field_item(field: &casual_doc_model::v1::Field, ctx: &mut FlowCtx) -> FlowItem<'static> {
    let kind = field_kind(&field.instruction);
    let cached = cached_result_text(&field.inlines);
    let value = if cached.is_empty() {
        match kind {
            // A placeholder so the flowed field has sensible width/height before the
            // field pass runs; `Passthrough` shows nothing when it has no result.
            FieldKind::Page | FieldKind::NumPages => "1".to_owned(),
            FieldKind::Passthrough => String::new(),
        }
    } else {
        cached
    };
    let style = field_style(&field.inlines, &value, ctx);
    FlowItem::Field { kind, value, style }
}

/// Classifies a field instruction by its leading keyword (case-insensitive):
/// `PAGE` and `NUMPAGES` are the page-dependent fields the field pass recomputes;
/// everything else passes its cached result through unchanged.
fn field_kind(instruction: &str) -> FieldKind {
    match instruction
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase()
        .as_str()
    {
        "PAGE" => FieldKind::Page,
        "NUMPAGES" => FieldKind::NumPages,
        _ => FieldKind::Passthrough,
    }
}

/// Concatenates the text of a field's cached-result runs (its `inlines`, which are
/// leaf-only) — the value shown before the field pass recomputes it.
fn cached_result_text(inlines: &[InlineNode]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => out.push_str(&run.text),
            InlineNode::Tab(_) => out.push('\t'),
            _ => {}
        }
    }
    out
}

/// The styling used to shape (and later reshape) a field's value: the first
/// cached-result run's resolved style, or the document default when the field has
/// no cached result.
fn field_style(inlines: &[InlineNode], value: &str, ctx: &mut FlowCtx) -> FieldStyle {
    let props = inlines.iter().find_map(|n| match n {
        InlineNode::Run(run) => Some(&run.properties),
        _ => None,
    });
    let styled = match props {
        Some(props) => styled_run(value, props, ctx),
        None => styled_run(value, &RunProperties::default(), ctx),
    };
    FieldStyle {
        font: styled.font,
        size: styled.size,
        color: styled.color,
        bold: styled.bold,
        italic: styled.italic,
        letter_spacing: styled.letter_spacing,
        decoration: styled.decoration,
    }
}

/// Maps an inline drawing to an [`FlowItem::Image`]: resolves its media id to the
/// package part name (the display list's stable media key) and its EMU extent to a
/// twip box. Returns `None` — contributing nothing, never panicking — when the
/// drawing declares no extent (so it cannot be sized here) or its media id is
/// absent from the table. Anchored/floating placement is a later slice
/// (`P1F-28`); this is the inline case.
fn image_item(drawing: &Drawing, ctx: &FlowCtx) -> Option<FlowItem<'static>> {
    let part = ctx.media.get(&drawing.media)?.part_name.clone();
    let size = extent_to_size(drawing.extent.as_ref()?);
    (size.width.raw() > 0 && size.height.raw() > 0).then_some(FlowItem::Image { media: part, size })
}

/// Converts a drawing's EMU extent to a twip box size.
fn extent_to_size(extent: &Extent) -> Size {
    Size::new(
        emu_to_twip(extent.width_emu),
        emu_to_twip(extent.height_emu),
    )
}

/// EMU → twips: 914400 EMU/inch ÷ 1440 twips/inch = exactly 635 EMU per twip.
/// Clamped to a non-negative `i32` twip (the model bounds `Extent` to `MAX_EMU`).
fn emu_to_twip(emu: i64) -> Twip {
    Twip((emu / 635).clamp(0, i64::from(i32::MAX)) as i32)
}

/// Shapes a paragraph's [`FlowItem`] stream into lines. A stream with no inline
/// box (image or text box) takes the text path directly ([`shape_text_items`]); a
/// stream carrying one splits at each box, shaping the intervening text with the
/// same path and placing each box as its own inline-box line, so reading order and
/// vertical stacking are preserved. Anchored/floating placement is a later slice
/// (`P1F-28`); this is the inline case.
fn shape_paragraph_items(
    shaper: &dyn LineShaper,
    items: &[FlowItem<'_>],
    tab_stops: &[TabStop],
    default_tab: Twip,
    constraints: LineConstraints,
    range: ModelRange,
) -> LineLayout {
    // A paragraph carrying an inline field takes the fielded path, which lays runs,
    // tabs, and fields on one horizontal line per hard-break block and records a
    // field marker per field for the post-pagination field pass.
    if items.iter().any(|i| matches!(i, FlowItem::Field { .. })) {
        return shape_fielded_paragraph(shaper, items, tab_stops, default_tab, constraints, range);
    }
    let is_box =
        |item: &FlowItem<'_>| matches!(item, FlowItem::Image { .. } | FlowItem::TextBox { .. });
    if !items.iter().any(is_box) {
        return shape_text_items(shaper, items, tab_stops, default_tab, constraints, range);
    }

    let mut out: Vec<Line> = Vec::new();
    let mut cursor_y = Twip::ZERO;
    let mut i = 0usize;
    while i < items.len() {
        match &items[i] {
            FlowItem::Image { media, size } => {
                let line = image_line(media.clone(), *size, range);
                stack_lines(&mut out, vec![line], &mut cursor_y);
                i += 1;
            }
            FlowItem::TextBox {
                blocks,
                size,
                border,
                fill,
            } => {
                let line = textbox_line(blocks.clone(), *size, *border, *fill, range);
                stack_lines(&mut out, vec![line], &mut cursor_y);
                i += 1;
            }
            _ => {
                let start = i;
                while i < items.len() && !is_box(&items[i]) {
                    i += 1;
                }
                let chunk = shape_text_items(
                    shaper,
                    &items[start..i],
                    tab_stops,
                    default_tab,
                    constraints,
                    range,
                );
                stack_lines(&mut out, chunk.lines, &mut cursor_y);
            }
        }
    }
    LineLayout { lines: out }
}

/// Shapes an image-free [`FlowItem`] slice. Ordinary text (no tab, no break, no
/// `bar` tab stop) takes the fast path — the base shaper alone, so its output is
/// byte-identical to before the flow layer existed; anything with a tab or break
/// is resolved by [`crate::tabs`].
fn shape_text_items(
    shaper: &dyn LineShaper,
    items: &[FlowItem<'_>],
    tab_stops: &[TabStop],
    default_tab: Twip,
    constraints: LineConstraints,
    range: ModelRange,
) -> LineLayout {
    if !tabs::needs_flow_layout(items, tab_stops) {
        let runs: Vec<StyledRun<'_>> = items
            .iter()
            .filter_map(|item| match item {
                FlowItem::Run(run) => Some(run.clone()),
                _ => None,
            })
            .collect();
        return shaper.shape_paragraph(&runs, constraints, range);
    }
    tabs::shape_with_flow(shaper, items, tab_stops, default_tab, constraints, range)
}

/// A line holding a single inline image box at the paragraph's leading edge, its
/// height equal to the image height so following content stacks below it.
fn image_line(media: String, size: Size, range: ModelRange) -> Line {
    Line {
        runs: Vec::new(),
        ascent: size.height,
        descent: Twip::ZERO,
        height: size.height,
        range,
        line_break: LineBreak::Wrap,
        page_break_after: false,
        bars: Vec::new(),
        images: vec![InlineImage {
            media,
            origin: Point::new(Twip::ZERO, Twip::ZERO),
            size,
        }],
        fields: Vec::new(),
        text_boxes: Vec::new(),
    }
}

/// A line holding a single inline text box at the paragraph's leading edge, its
/// height equal to the box's outer height so following content stacks below it. The
/// box carries its already-flowed block fragments; composition paints the fill and
/// border and composes those fragments offset into the box.
fn textbox_line(
    blocks: Vec<BlockFragment>,
    size: Size,
    border: Option<[u8; 4]>,
    fill: Option<[u8; 4]>,
    range: ModelRange,
) -> Line {
    Line {
        runs: Vec::new(),
        ascent: size.height,
        descent: Twip::ZERO,
        height: size.height,
        range,
        line_break: LineBreak::Wrap,
        page_break_after: false,
        bars: Vec::new(),
        images: Vec::new(),
        fields: Vec::new(),
        text_boxes: vec![InlineTextBox {
            origin: Point::new(Twip::ZERO, Twip::ZERO),
            size,
            blocks,
            border,
            fill,
        }],
    }
}

/// Appends `lines` below the ones already in `out`, shifting each line's runs,
/// images, and text boxes down by `cursor_y` (into paragraph-absolute y) and
/// advancing `cursor_y` past them.
fn stack_lines(out: &mut Vec<Line>, mut lines: Vec<Line>, cursor_y: &mut Twip) {
    for line in &mut lines {
        for run in &mut line.runs {
            run.origin.y = run.origin.y + *cursor_y;
        }
        for image in &mut line.images {
            image.origin.y = image.origin.y + *cursor_y;
        }
        for text_box in &mut line.text_boxes {
            text_box.origin.y = text_box.origin.y + *cursor_y;
        }
        *cursor_y = *cursor_y + line.height;
    }
    out.extend(lines);
}

// --- Inline fields ---------------------------------------------------------

/// A field's value shaped into a single glyph run at `origin` — the shared shaping
/// used both to lay a field out at flow time and to restamp it in the field pass
/// ([`crate::paginate::resolve_fields`]), so a resolved field is byte-identical
/// however it was produced. The glyphs of `value`'s shaped runs are concatenated
/// into one run (clusters zeroed — a field's value is not model text), and its
/// advance / metrics are returned so callers can position following content and
/// size the line.
#[must_use]
pub(crate) fn shape_field_run(
    shaper: &dyn LineShaper,
    value: &str,
    style: FieldStyle,
    origin: Point,
) -> FieldRunShape {
    let styled = StyledRun {
        text: value.into(),
        font: style.font,
        size: style.size,
        bold: style.bold,
        italic: style.italic,
        letter_spacing: style.letter_spacing,
        color: style.color,
        decoration: style.decoration,
        highlight: None,
        baseline_shift: Twip::ZERO,
    };
    let node = NodeId::from_parts(1, 1).expect("1/1 is a valid node id");
    let dummy = ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0));
    let layout = shaper.shape_paragraph(&[styled], tabs::unwrapped_constraints(), dummy);
    let mut glyphs: Vec<Glyph> = Vec::new();
    let (mut ascent, mut descent) = (Twip::ZERO, Twip::ZERO);
    if let Some(line) = layout.lines.first() {
        ascent = line.ascent;
        descent = line.descent;
        for run in &line.runs {
            for g in &run.glyphs {
                glyphs.push(Glyph {
                    id: g.id,
                    advance: g.advance,
                    cluster: 0,
                });
            }
        }
    }
    let advance = glyphs.iter().fold(Twip::ZERO, |a, g| a + g.advance);
    FieldRunShape {
        run: GlyphRun {
            font: style.font,
            size: style.size,
            color: style.color,
            origin,
            bidi_level: 0,
            decoration: style.decoration,
            highlight: None,
            glyphs,
        },
        ascent,
        descent,
        advance,
    }
}

/// The result of [`shape_field_run`]: the shaped run plus the metrics a caller
/// needs to place following content and size the line.
pub(crate) struct FieldRunShape {
    /// The single glyph run for the field's value.
    pub run: GlyphRun,
    /// Ascent above the baseline.
    pub ascent: Twip,
    /// Descent below the baseline.
    pub descent: Twip,
    /// Total advance width.
    pub advance: Twip,
}

/// The total advance of a glyph run.
fn run_advance(run: &GlyphRun) -> Twip {
    run.glyphs.iter().fold(Twip::ZERO, |a, g| a + g.advance)
}

/// One tab-delimited segment of a fielded line: its shaped runs (local x from the
/// segment's left edge), the field markers pointing into those runs, and its box
/// metrics.
struct FieldedSegment {
    runs: Vec<GlyphRun>,
    fields: Vec<FieldPiece>,
    width: Twip,
    ascent: Twip,
    descent: Twip,
}

/// A field within a [`FieldedSegment`]: which of the segment's runs holds it, plus
/// the marker payload carried to the line.
struct FieldPiece {
    run: usize,
    kind: FieldKind,
    style: FieldStyle,
    value: String,
}

/// Shapes a paragraph whose inline stream contains fields. Runs, tabs, and fields
/// are laid out on a single horizontal line per hard-break block (fielded
/// paragraphs are not soft-wrapped — the target cases, header/footer and
/// page-number lines, are single-line in Word); each field becomes its own glyph
/// run tagged with a [`FieldMarker`] the post-pagination field pass resolves.
fn shape_fielded_paragraph(
    shaper: &dyn LineShaper,
    items: &[FlowItem<'_>],
    tab_stops: &[TabStop],
    default_tab: Twip,
    constraints: LineConstraints,
    range: ModelRange,
) -> LineLayout {
    let ctx = FieldedCtx {
        shaper,
        node: range.start.node,
        base: range.start.offset,
        tab_stops,
        default_tab,
        constraints,
    };
    let bars: Vec<Twip> = tab_stops
        .iter()
        .filter(|t| t.alignment == TabAlignment::Bar)
        .map(|t| Twip(t.position_twips))
        .collect();

    // Split the stream into hard-break-delimited blocks (each is one line).
    let mut blocks: Vec<(Vec<&FlowItem<'_>>, Option<BreakKind>)> = Vec::new();
    let mut current: Vec<&FlowItem<'_>> = Vec::new();
    for item in items {
        match item {
            FlowItem::Break(kind) => {
                blocks.push((std::mem::take(&mut current), Some(*kind)));
            }
            other => current.push(other),
        }
    }
    blocks.push((current, None));

    let mut out: Vec<Line> = Vec::new();
    let mut cursor_y = Twip::ZERO;
    let last = blocks.len().saturating_sub(1);
    for (bi, (block_items, trailing)) in blocks.iter().enumerate() {
        let first_line_indent = if bi == 0 {
            constraints.first_line_indent
        } else {
            Twip::ZERO
        };
        let mut line = layout_fielded_line(&ctx, block_items, first_line_indent);
        for run in &mut line.runs {
            run.origin.y = run.origin.y + cursor_y;
        }
        line.bars = bars.clone();
        match trailing {
            Some(kind) => {
                line.line_break = LineBreak::Hard;
                line.page_break_after = matches!(kind, BreakKind::Page | BreakKind::Column);
            }
            None if bi == last => line.line_break = LineBreak::ParagraphEnd,
            None => line.line_break = LineBreak::Hard,
        }
        cursor_y = cursor_y + line.height;
        out.push(line);
    }
    LineLayout { lines: out }
}

/// The invariant context threaded through a fielded paragraph's line layout: the
/// shaper, the node/byte anchor, and the paragraph's tab + wrap settings.
struct FieldedCtx<'a> {
    shaper: &'a dyn LineShaper,
    node: NodeId,
    base: u32,
    tab_stops: &'a [TabStop],
    default_tab: Twip,
    constraints: LineConstraints,
}

/// Lays out one hard-break block of a fielded paragraph onto a single line: splits
/// it at tabs into segments, measures each, then positions segment 0 at the
/// (indented) start and each later segment at its resolved tab stop. Records a
/// [`FieldMarker`] per field with the run's absolute origin as its idempotent
/// reposition anchor.
fn layout_fielded_line(
    ctx: &FieldedCtx<'_>,
    items: &[&FlowItem<'_>],
    first_line_indent: Twip,
) -> Line {
    let shaper = ctx.shaper;
    let node = ctx.node;
    let base = ctx.base;
    let tab_stops = ctx.tab_stops;
    let default_tab = ctx.default_tab;
    let constraints = ctx.constraints;
    // Split at tabs into segments.
    let mut segments: Vec<Vec<&FlowItem<'_>>> = vec![Vec::new()];
    for item in items {
        if matches!(item, FlowItem::Tab) {
            segments.push(Vec::new());
        } else {
            segments.last_mut().expect("non-empty").push(item);
        }
    }
    let has_tab = segments.len() > 1;
    let measured: Vec<FieldedSegment> = segments
        .iter()
        .map(|seg| measure_fielded_segment(shaper, node, seg))
        .collect();

    let ascent = measured
        .iter()
        .map(|s| s.ascent)
        .max()
        .unwrap_or(Twip::ZERO);
    let descent = measured
        .iter()
        .map(|s| s.descent)
        .max()
        .unwrap_or(Twip::ZERO);
    let baseline = ascent;

    let mut runs: Vec<GlyphRun> = Vec::new();
    let mut fields: Vec<FieldMarker> = Vec::new();
    let mut pen = first_line_indent.raw().max(0);

    for (i, seg) in measured.iter().enumerate() {
        let left = if i == 0 {
            pen
        } else {
            let stop = tabs::resolve_next_stop(pen, tab_stops, default_tab);
            let mut l = match stop.alignment {
                TabAlignment::Start | TabAlignment::Bar => stop.position,
                TabAlignment::End | TabAlignment::Decimal => stop.position - seg.width.raw(),
                TabAlignment::Center => stop.position - seg.width.raw() / 2,
            };
            if l < pen {
                l = pen;
            }
            l
        };
        place_segment(seg, Twip(left), baseline, &mut runs, &mut fields);
        pen = left + seg.width.raw();
    }

    // A tab-free fielded line honors paragraph alignment (a centered / right page
    // number). Tabbed lines are positioned by their stops, as in Word.
    if !has_tab {
        let width = pen - first_line_indent.raw().max(0);
        let slack = constraints.max_width.raw() - width;
        let offset = match constraints.alignment {
            TextAlignment::Center => slack / 2,
            TextAlignment::End => slack,
            TextAlignment::Start | TextAlignment::Justify => 0,
        };
        if offset > 0 {
            for run in &mut runs {
                run.origin.x = run.origin.x + Twip(offset);
            }
            for field in &mut fields {
                field.base_x = field.base_x + Twip(offset);
            }
        }
    }

    Line {
        runs,
        ascent,
        descent,
        height: ascent + descent,
        range: ModelRange::new(ModelPos::new(node, base), ModelPos::new(node, base)),
        line_break: LineBreak::ParagraphEnd,
        page_break_after: false,
        bars: Vec::new(),
        images: Vec::new(),
        fields,
        text_boxes: Vec::new(),
    }
}

/// Places a measured segment at absolute `left`/`baseline`, appending its runs to
/// `runs` and its field markers (with absolute `base_x`) to `fields`.
fn place_segment(
    seg: &FieldedSegment,
    left: Twip,
    baseline: Twip,
    runs: &mut Vec<GlyphRun>,
    fields: &mut Vec<FieldMarker>,
) {
    let base_index = runs.len();
    for run in &seg.runs {
        let mut placed = run.clone();
        placed.origin = Point::new(run.origin.x + left, baseline);
        runs.push(placed);
    }
    for piece in &seg.fields {
        let run_idx = base_index + piece.run;
        fields.push(FieldMarker {
            kind: piece.kind,
            run: run_idx as u32,
            base_x: runs[run_idx].origin.x,
            style: piece.style,
            value: piece.value.clone(),
        });
    }
}

/// Measures one tab-delimited segment of a fielded line: consecutive text runs are
/// shaped together (preserving intra-group kerning); each field is shaped on its
/// own. Runs carry local x from the segment's left edge; field pieces point at the
/// run holding each field.
fn measure_fielded_segment(
    shaper: &dyn LineShaper,
    node: NodeId,
    seg: &[&FlowItem<'_>],
) -> FieldedSegment {
    let mut runs: Vec<GlyphRun> = Vec::new();
    let mut fields: Vec<FieldPiece> = Vec::new();
    let mut pen = 0i32;
    let mut ascent = Twip::ZERO;
    let mut descent = Twip::ZERO;
    let range = ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0));

    let mut i = 0;
    while i < seg.len() {
        match seg[i] {
            FlowItem::Field { kind, value, style } => {
                let shape =
                    shape_field_run(shaper, value, *style, Point::new(Twip(pen), Twip::ZERO));
                ascent = ascent.max(shape.ascent);
                descent = descent.max(shape.descent);
                let idx = runs.len();
                runs.push(shape.run);
                fields.push(FieldPiece {
                    run: idx,
                    kind: *kind,
                    style: *style,
                    value: value.clone(),
                });
                pen += shape.advance.raw();
                i += 1;
            }
            FlowItem::Run(_) => {
                let start = i;
                while i < seg.len() && matches!(seg[i], FlowItem::Run(_)) {
                    i += 1;
                }
                let styled: Vec<StyledRun<'_>> = seg[start..i]
                    .iter()
                    .filter_map(|it| match it {
                        FlowItem::Run(r) => Some(r.clone()),
                        _ => None,
                    })
                    .collect();
                let layout = shaper.shape_paragraph(&styled, tabs::unwrapped_constraints(), range);
                if let Some(line) = layout.lines.first() {
                    ascent = ascent.max(line.ascent);
                    descent = descent.max(line.descent);
                    let mut group_w = 0i32;
                    for r in &line.runs {
                        let end = r.origin.x.raw() + run_advance(r).raw();
                        group_w = group_w.max(end);
                        let mut placed = r.clone();
                        placed.origin = Point::new(r.origin.x + Twip(pen), Twip::ZERO);
                        runs.push(placed);
                    }
                    pen += group_w;
                }
            }
            // Images/tabs/breaks do not reach here (tabs split the segment; images
            // and breaks are handled by their own paths).
            _ => i += 1,
        }
    }

    FieldedSegment {
        runs,
        fields,
        width: Twip(pen),
        ascent,
        descent,
    }
}

/// Maps a run's text + properties to a styled run, resolving its declared font
/// family (`w:rFonts`, direct or theme) to a concrete bundled face via the
/// [`FontResolver`] and recording any substitution/coverage fallback (`P1C-002b`).
///
/// Run appearance beyond the face is applied here too: `w:vertAlign` super/subscript
/// (a baseline shift plus a reduced shaped size) and `w:position` (a half-point
/// baseline raise/lower with no resize) fold into `size`/`baseline_shift`; `w:caps`
/// and `w:smallCaps` uppercase the text; and `w:color` resolves against the theme
/// palette. Small-caps per-letter size-splitting is done by [`push_styled_runs`],
/// so this single-run path uppercases without the per-letter size step.
fn styled_run<'a>(text: &'a str, properties: &RunProperties, ctx: &mut FlowCtx) -> StyledRun<'a> {
    let (size, baseline_shift) = run_metrics(properties);
    let text = case_transform(text, properties);
    let bold = properties.bold.unwrap_or(false);
    let italic = properties.italic.unwrap_or(false);
    StyledRun {
        // Resolve the declared family to a concrete face so the renderer outlines
        // the same face `parley` shapes with (measured against the shaped text).
        font: resolve_font(text.as_ref(), properties, bold, italic, ctx),
        text,
        size,
        bold,
        italic,
        letter_spacing: properties.character_spacing_twips.map_or(Twip::ZERO, Twip),
        color: run_color(properties.color, ctx.palette),
        decoration: Decoration {
            underline: properties.underline.unwrap_or(false),
            strikethrough: properties.strike.unwrap_or(false),
        },
        highlight: properties.highlight.and_then(highlight_rgba),
        baseline_shift,
    }
}

/// The size (twips) a run shapes at and its baseline shift (twips, positive =
/// raised toward the top of the line), from `w:vertAlign` and `w:position`.
/// Super/subscript raise (~1/3 of the size) / lower (~1/6) the baseline and shrink
/// the glyphs to ~2/3; `w:position` adds a half-point baseline raise(+)/lower(−)
/// with no resize. The two compose.
fn run_metrics(properties: &RunProperties) -> (Twip, Twip) {
    // `w:sz` is in half-points; a half-point is 10 twips (a point is 20). Default
    // to 11pt (Word's default body size) when unset.
    let base = properties
        .size_half_points
        .map_or(Twip::from_points(11), |hp| Twip(hp as i32 * 10));
    let (size, mut shift) = match properties.vertical_alignment {
        Some(VerticalAlignment::Superscript) => (Twip(base.raw() * 2 / 3), Twip(base.raw() / 3)),
        Some(VerticalAlignment::Subscript) => (Twip(base.raw() * 2 / 3), Twip(-base.raw() / 6)),
        Some(VerticalAlignment::Baseline) | None => (base, Twip::ZERO),
    };
    // `w:position` is a half-point baseline offset (positive raises), no resize.
    if let Some(hp) = properties.position_half_points {
        shift = Twip(shift.raw() + hp * 10);
    }
    (size, shift)
}

/// Applies `w:caps`/`w:smallCaps` casing: both uppercase the run text (small-caps
/// per-letter sizing is layered on by [`push_styled_runs`]). Untransformed text is
/// borrowed, so the common case stays allocation-free.
fn case_transform<'a>(text: &'a str, properties: &RunProperties) -> Cow<'a, str> {
    if properties.all_caps == Some(true) || properties.small_caps == Some(true) {
        Cow::Owned(text.to_uppercase())
    } else {
        Cow::Borrowed(text)
    }
}

/// Pushes the styled run(s) for a model run. `w:smallCaps` splits the run so that
/// originally-lowercase letters are uppercased and shaped at ~3/4 size while the
/// rest keep full size (Word's small-caps look); every other run yields exactly one
/// styled run via [`styled_run`].
fn push_styled_runs<'a>(
    text: &'a str,
    properties: &RunProperties,
    ctx: &mut FlowCtx,
    out: &mut Vec<StyledRun<'a>>,
) {
    if properties.small_caps == Some(true) {
        push_small_caps_runs(text, properties, ctx, out);
    } else {
        out.push(styled_run(text, properties, ctx));
    }
}

/// Splits a small-caps run into uppercased spans, shaping originally-lowercase
/// spans at ~3/4 of the run size and every other span at full size. Each span's
/// text is owned (uppercased), so the pushed runs borrow nothing from `text`.
fn push_small_caps_runs<'a>(
    text: &str,
    properties: &RunProperties,
    ctx: &mut FlowCtx,
    out: &mut Vec<StyledRun<'a>>,
) {
    let (base, baseline_shift) = run_metrics(properties);
    let bold = properties.bold.unwrap_or(false);
    let italic = properties.italic.unwrap_or(false);
    let letter_spacing = properties.character_spacing_twips.map_or(Twip::ZERO, Twip);
    let color = run_color(properties.color, ctx.palette);
    let decoration = Decoration {
        underline: properties.underline.unwrap_or(false),
        strikethrough: properties.strike.unwrap_or(false),
    };
    let highlight = properties.highlight.and_then(highlight_rgba);
    for (span, was_lower) in small_caps_spans(text) {
        let size = if was_lower {
            Twip(base.raw() * 3 / 4)
        } else {
            base
        };
        let upper = span.to_uppercase();
        let font = resolve_font(&upper, properties, bold, italic, ctx);
        out.push(StyledRun {
            text: Cow::Owned(upper),
            font,
            size,
            bold,
            italic,
            letter_spacing,
            color,
            decoration,
            highlight,
            baseline_shift,
        });
    }
}

/// Splits `text` into maximal spans that are each either all originally-lowercase
/// letters or all other characters, in order, tagging each span with whether it was
/// the lowercase group (which small-caps shrinks).
fn small_caps_spans(text: &str) -> Vec<(&str, bool)> {
    let mut spans = Vec::new();
    let mut start = 0;
    let mut group: Option<bool> = None;
    for (i, ch) in text.char_indices() {
        let lower = ch.is_lowercase();
        match group {
            Some(g) if g == lower => {}
            Some(g) => {
                spans.push((&text[start..i], g));
                start = i;
            }
            None => {}
        }
        group = Some(lower);
    }
    if let Some(g) = group {
        spans.push((&text[start..], g));
    }
    spans
}

/// Resolves a run's `w:color` to opaque RGBA: an explicit `w:color@val` sRGB is
/// itself; a `w:themeColor` resolves against the document's theme palette (with any
/// tint/shade applied); an absent color (`w:color@val="auto"` / unset) is black.
fn run_color(color: Option<Color>, palette: Option<&ResolvedPalette>) -> [u8; 4] {
    match color {
        Some(Color::Rgb(rgb)) => [rgb.r, rgb.g, rgb.b, 255],
        Some(Color::Theme(theme)) => match palette {
            // The model carries no tint/shade on a theme color yet, so the factors
            // are `None` today; routing every theme color through `apply_tint_shade`
            // keeps the resolution complete for when the model gains them.
            Some(palette) => apply_tint_shade(palette.slot(theme.slot), None, None),
            None => [0, 0, 0, 255],
        },
        None => [0, 0, 0, 255],
    }
}

/// The document's theme color scheme (`a:clrScheme`) resolved to one opaque RGBA
/// per slot, so a `w:themeColor` reference resolves to the real color rather than
/// silently rendering black.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ResolvedPalette {
    /// The twelve slots, in `ColorScheme` field order (see [`theme_slot_index`]).
    slots: [[u8; 4]; 12],
}

impl ResolvedPalette {
    /// The resolved RGBA for a theme color slot.
    fn slot(&self, slot: ThemeColorRef) -> [u8; 4] {
        self.slots[theme_slot_index(slot)]
    }
}

/// The [`ResolvedPalette`] index for a theme color slot (matching [`ColorScheme`]'s
/// OOXML child order: dk1, lt1, dk2, lt2, accent1..6, hlink, folHlink).
fn theme_slot_index(slot: ThemeColorRef) -> usize {
    match slot {
        ThemeColorRef::Dark1 => 0,
        ThemeColorRef::Light1 => 1,
        ThemeColorRef::Dark2 => 2,
        ThemeColorRef::Light2 => 3,
        ThemeColorRef::Accent1 => 4,
        ThemeColorRef::Accent2 => 5,
        ThemeColorRef::Accent3 => 6,
        ThemeColorRef::Accent4 => 7,
        ThemeColorRef::Accent5 => 8,
        ThemeColorRef::Accent6 => 9,
        ThemeColorRef::Hyperlink => 10,
        ThemeColorRef::FollowedHyperlink => 11,
    }
}

/// Resolves a document's [`ColorScheme`] to a [`ResolvedPalette`]: each slot's
/// `a:srgbClr` becomes its RGB and each `a:sysClr` resolves to its `lastClr` (or a
/// sensible default for the named system color when none was recorded).
fn resolve_palette(scheme: &ColorScheme) -> ResolvedPalette {
    ResolvedPalette {
        slots: [
            resolve_scheme_color(&scheme.dark1),
            resolve_scheme_color(&scheme.light1),
            resolve_scheme_color(&scheme.dark2),
            resolve_scheme_color(&scheme.light2),
            resolve_scheme_color(&scheme.accent1),
            resolve_scheme_color(&scheme.accent2),
            resolve_scheme_color(&scheme.accent3),
            resolve_scheme_color(&scheme.accent4),
            resolve_scheme_color(&scheme.accent5),
            resolve_scheme_color(&scheme.accent6),
            resolve_scheme_color(&scheme.hyperlink),
            resolve_scheme_color(&scheme.followed_hyperlink),
        ],
    }
}

/// Resolves one scheme color slot to opaque RGBA: an `a:srgbClr` is its RGB; an
/// `a:sysClr` uses its recorded `lastClr`, falling back to the conventional value
/// for the named system color (see [`default_system_color`]).
fn resolve_scheme_color(color: &SchemeColor) -> [u8; 4] {
    match color {
        SchemeColor::Srgb(rgb) => [rgb.r, rgb.g, rgb.b, 255],
        SchemeColor::System(sys) => match sys.last_color {
            Some(rgb) => [rgb.r, rgb.g, rgb.b, 255],
            None => default_system_color(&sys.value),
        },
    }
}

/// The conventional sRGB for a named `a:sysClr` token when no `lastClr` was
/// recorded: the light "surface" tokens are white, everything else (text-like) is
/// black — enough to keep an unrecorded system color legible.
fn default_system_color(value: &str) -> [u8; 4] {
    match value {
        "window" | "background" | "btnFace" | "menu" | "3dLight" => [255, 255, 255, 255],
        _ => [0, 0, 0, 255],
    }
}

/// Applies an OOXML `tint` (lightens toward white) and/or `shade` (darkens toward
/// black) to an RGB, each a `0.0..=1.0` factor; `None` leaves the channel
/// unchanged. Alpha is preserved. Ready for when the model carries theme
/// tint/shade — [`run_color`] routes every resolved theme color through it.
fn apply_tint_shade(rgb: [u8; 4], tint: Option<f32>, shade: Option<f32>) -> [u8; 4] {
    let adjust = |c: u8| -> u8 {
        let mut v = f32::from(c);
        if let Some(t) = tint {
            let t = t.clamp(0.0, 1.0);
            v = v * t + 255.0 * (1.0 - t);
        }
        if let Some(s) = shade {
            v *= s.clamp(0.0, 1.0);
        }
        v.round().clamp(0.0, 255.0) as u8
    };
    [adjust(rgb[0]), adjust(rgb[1]), adjust(rgb[2]), rgb[3]]
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

/// Builds the shaper constraints for a paragraph flowed into `width`: the wrap
/// width is the column width less the start/end indents (so lines wrap at the
/// indented column), and the first-line indent carries `w:ind@firstLine`
/// (positive) or `w:ind@hanging` (negative, protruding). Hanging wins when both
/// are present, matching Word.
fn line_constraints(properties: &ParagraphProperties, width: Twip) -> LineConstraints {
    let spacing = properties.spacing.as_ref();
    let metrics = box_metrics(properties);
    let indent = properties.indentation.as_ref();
    let first_line_indent = match indent {
        Some(i) if i.hanging_twips.unwrap_or(0) != 0 => Twip(-i.hanging_twips.unwrap_or(0)),
        Some(i) => Twip(i.first_line_twips.unwrap_or(0)),
        None => Twip::ZERO,
    };
    let max_width =
        Twip((width.raw() - metrics.indent_start.raw() - metrics.indent_end.raw()).max(1));
    LineConstraints {
        max_width,
        rtl: false,
        alignment: alignment(properties),
        line_height_percent: spacing.and_then(|s| s.line_percent),
        first_line_indent,
    }
}

/// Builds the paint-only decoration (background shading + borders) of a paragraph
/// box flowed into `width`. Empty (serializes to nothing) for a plain paragraph.
fn paragraph_decor(properties: &ParagraphProperties, width: Twip) -> ParagraphDecor {
    let shading = properties.shading.fill.map(|c| [c.r, c.g, c.b, 255]);
    let b = &properties.borders;
    let borders = BlockBorders {
        top: single_edge(b.top.as_ref()),
        bottom: single_edge(b.bottom.as_ref()),
        start: single_edge(b.start.as_ref()),
        end: single_edge(b.end.as_ref()),
    };
    if shading.is_none() && borders.is_empty() {
        return ParagraphDecor::default();
    }
    ParagraphDecor {
        shading,
        borders,
        width,
    }
}

/// Converts a single model border edge to a drawable [`ResolvedEdge`] (no
/// conflict resolution — paragraph borders have a single candidate per edge),
/// returning `None` for an absent or invisible (`nil`/`none`) edge.
fn single_edge(edge: Option<&BorderEdge>) -> Option<ResolvedEdge> {
    let edge = edge?;
    if !is_visible_border(edge) {
        return None;
    }
    let color = edge.color.map_or([0, 0, 0, 255], |c| [c.r, c.g, c.b, 255]);
    // `w:sz` is in eighths of a point; a point is 20 twips.
    let width = edge
        .size_eighth_points
        .map_or(Twip(10), |sz| Twip(((sz * 20) / 8).max(1) as i32));
    Some(ResolvedEdge { color, width })
}

/// Resolves a named `w:highlight` color to an opaque RGBA fill. `None`
/// (`ST_HighlightColor` `none`) yields no highlight.
fn highlight_rgba(color: HighlightColor) -> Option<[u8; 4]> {
    let (r, g, b) = match color {
        HighlightColor::None => return None,
        HighlightColor::Black => (0, 0, 0),
        HighlightColor::Blue => (0, 0, 255),
        HighlightColor::Cyan => (0, 255, 255),
        HighlightColor::DarkBlue => (0, 0, 139),
        HighlightColor::DarkCyan => (0, 139, 139),
        HighlightColor::DarkGray => (169, 169, 169),
        HighlightColor::DarkGreen => (0, 100, 0),
        HighlightColor::DarkMagenta => (139, 0, 139),
        HighlightColor::DarkRed => (139, 0, 0),
        HighlightColor::DarkYellow => (153, 153, 0),
        HighlightColor::Green => (0, 255, 0),
        HighlightColor::LightGray => (211, 211, 211),
        HighlightColor::Magenta => (255, 0, 255),
        HighlightColor::Red => (255, 0, 0),
        HighlightColor::White => (255, 255, 255),
        HighlightColor::Yellow => (255, 255, 0),
    };
    Some([r, g, b, 255])
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

    /// The lines of the first paragraph fragment in a galley.
    fn first_paragraph_lines(galley: &[BlockFragment]) -> &crate::text::LineLayout {
        match &galley[0] {
            BlockFragment::Paragraph { lines, .. } => lines,
            BlockFragment::TableRow { .. } => panic!("expected a paragraph fragment"),
        }
    }

    #[test]
    fn a_hard_line_break_splits_a_paragraph_into_two_lines() {
        use casual_doc_model::v1::{Break, BreakKind};
        let para = paragraph(
            10,
            vec![
                run_node(11, "before", RunProperties::default()),
                InlineNode::Break(Break {
                    id: NodeId::from_parts(12, 1).unwrap(),
                    kind: BreakKind::Line,
                }),
                run_node(13, "after", RunProperties::default()),
            ],
        );
        let shaper = ParleyShaper::new();
        let galley = build_galley(&document(vec![para]), &shaper, Twip::from_points(400));
        let lines = first_paragraph_lines(&galley);
        assert_eq!(lines.lines.len(), 2, "the hard break yields two lines");
        assert!(
            !lines.lines[0].page_break_after,
            "a line break is not a page break"
        );
    }

    #[test]
    fn a_page_break_threads_a_marker_to_the_paginator() {
        use casual_doc_model::v1::{Break, BreakKind};
        let para = paragraph(
            10,
            vec![
                run_node(11, "before", RunProperties::default()),
                InlineNode::Break(Break {
                    id: NodeId::from_parts(12, 1).unwrap(),
                    kind: BreakKind::Page,
                }),
                run_node(13, "after", RunProperties::default()),
            ],
        );
        let shaper = ParleyShaper::new();
        let galley = build_galley(&document(vec![para]), &shaper, Twip::from_points(400));
        let lines = first_paragraph_lines(&galley);
        assert_eq!(lines.lines.len(), 2);
        assert!(
            lines.lines[0].page_break_after,
            "the page break sets the paginator marker on the first line"
        );
    }

    #[test]
    fn a_tab_advances_to_the_paragraphs_tab_stop() {
        use casual_doc_model::v1::{Tab, TabAlignment, TabStop};
        let properties = ParagraphProperties {
            tabs: vec![TabStop {
                position_twips: 3000,
                alignment: TabAlignment::Start,
                leader: None,
            }],
            ..ParagraphProperties::default()
        };
        let para = BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(10, 1).unwrap(),
            properties,
            inlines: vec![
                run_node(11, "A", RunProperties::default()),
                InlineNode::Tab(Tab {
                    id: NodeId::from_parts(12, 1).unwrap(),
                }),
                run_node(13, "B", RunProperties::default()),
            ],
        });
        let shaper = ParleyShaper::new();
        let galley = build_galley(&document(vec![para]), &shaper, Twip::from_points(400));
        let lines = first_paragraph_lines(&galley);
        assert_eq!(lines.lines.len(), 1);
        let b = lines.lines[0].runs.last().unwrap();
        assert!(
            (b.origin.x.raw() - 3000).abs() <= 20,
            "the tabbed run advances to the stop (3000), got {}",
            b.origin.x.raw()
        );
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
            grid_change: None,
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

    /// Regression: dense inline formatting must not overprint (P1F inline-overprint
    /// fix). A `parley` shaping run is split into one `GlyphRun` per contiguous
    /// brush/decoration span; the shaper must emit each `GlyphRun`'s *own* glyph
    /// slice, not the whole parent run's glyphs. Before the fix, adjacent same-face
    /// runs that differed only in brush (a highlighted run beside a recolored
    /// "inverse video" run) each re-emitted every glyph of the merged run, so their
    /// glyph clusters and x-ranges overlapped and the text drew on top of itself.
    ///
    /// The invariant asserted here is the one that was violated: on a line, the
    /// runs' glyph *cluster ranges* are pairwise disjoint (no byte is emitted by
    /// two runs), and each run's x-range starts at or after the previous run's end
    /// (allowing only `parley`'s <=1-twip per-run rounding, never a real backtrack).
    #[test]
    fn dense_inline_runs_do_not_overprint() {
        use casual_doc_model::v1::{Color, HighlightColor, RgbColor, VerticalAlignment};
        // Every run shares one face (no bold/italic split) so `parley` shapes them
        // as a single run split only by brush/decoration — the overprint trigger.
        let colored = |r, g, b| Some(Color::Rgb(RgbColor { r, g, b }));
        let inlines = vec![
            run_node(11, "Here is ", RunProperties::default()),
            run_node(
                12,
                "red",
                RunProperties {
                    color: colored(200, 0, 0),
                    ..RunProperties::default()
                },
            ),
            run_node(
                13,
                " strike",
                RunProperties {
                    strike: Some(true),
                    ..RunProperties::default()
                },
            ),
            run_node(
                14,
                "sup",
                RunProperties {
                    vertical_alignment: Some(VerticalAlignment::Superscript),
                    ..RunProperties::default()
                },
            ),
            run_node(
                15,
                "highlight",
                RunProperties {
                    highlight: Some(HighlightColor::Yellow),
                    ..RunProperties::default()
                },
            ),
            // "Inverse video": white glyphs, same face as the neighbours, so it
            // merges into the same shaping run as the highlighted span before it.
            run_node(
                16,
                "INVERSE",
                RunProperties {
                    color: colored(255, 255, 255),
                    ..RunProperties::default()
                },
            ),
            run_node(17, " tail", RunProperties::default()),
        ];
        let doc = document(vec![paragraph(10, inlines)]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(600));
        let lines = first_paragraph_lines(&galley);
        assert_eq!(lines.lines.len(), 1, "the text fits on one line");
        let runs = &lines.lines[0].runs;
        assert!(
            runs.len() >= 6,
            "the brush/decoration changes split the line into distinct runs (got {})",
            runs.len()
        );

        // 1) Cluster ranges are pairwise disjoint: no glyph byte offset is emitted
        //    by two runs. Before the fix, the highlighted and inverse runs both
        //    carried the whole merged run's clusters, so this set had duplicates.
        let mut seen: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        for run in runs {
            for glyph in &run.glyphs {
                assert!(
                    seen.insert(glyph.cluster),
                    "cluster byte {} is emitted by two runs — runs overprint",
                    glyph.cluster
                );
            }
        }

        // 2) Runs advance in x: each run starts at or after the previous run's
        //    right edge. A tolerance of 1 twip absorbs `parley`'s independent
        //    per-run offset/advance rounding; the pre-fix bug backtracked by
        //    hundreds of twips (a whole span's width), far outside it.
        let mut prev_end = i32::MIN;
        for run in runs {
            let start = run.origin.x.raw();
            let end = start + run_advance(run).raw();
            assert!(
                start + 1 >= prev_end,
                "run at x={start} overprints the previous run ending at x={prev_end}"
            );
            prev_end = end;
        }
    }

    /// Regression: consecutive non-empty paragraphs must each have a positive
    /// height so composition stacks them at distinct, non-overlapping y ranges.
    #[test]
    fn consecutive_paragraphs_have_positive_distinct_heights() {
        let doc = document(vec![
            paragraph(
                10,
                vec![run_node(11, "First paragraph", RunProperties::default())],
            ),
            paragraph(
                20,
                vec![run_node(21, "Second paragraph", RunProperties::default())],
            ),
        ]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let heights: Vec<i32> = galley
            .iter()
            .filter_map(|f| match f {
                BlockFragment::Paragraph { lines, .. } => Some(lines.height().raw()),
                BlockFragment::TableRow { .. } => None,
            })
            .collect();
        assert_eq!(heights.len(), 2, "two paragraph fragments");
        assert!(
            heights.iter().all(|&h| h > 0),
            "each paragraph has positive height so the next stacks below it: {heights:?}"
        );
    }

    /// A run with the given size (half-points) and extra run properties.
    fn sized_run(id: u64, text: &str, half_points: u32, properties: RunProperties) -> InlineNode {
        run_node(
            id,
            text,
            RunProperties {
                size_half_points: Some(half_points),
                ..properties
            },
        )
    }

    #[test]
    fn superscript_raises_and_shrinks_the_run() {
        use casual_doc_model::v1::VerticalAlignment;
        // A baseline run and a superscript run share the line; the superscript is
        // raised (smaller screen-y, which grows downward) and shaped smaller (~2/3).
        let base = sized_run(11, "x", 24, RunProperties::default());
        let sup = sized_run(
            12,
            "2",
            24,
            RunProperties {
                vertical_alignment: Some(VerticalAlignment::Superscript),
                ..RunProperties::default()
            },
        );
        let doc = document(vec![paragraph(10, vec![base, sup])]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let runs = &first_paragraph_lines(&galley).lines[0].runs;
        assert!(runs.len() >= 2, "base + superscript are distinct runs");
        let big = runs.iter().max_by_key(|r| r.size.raw()).unwrap();
        let small = runs.iter().min_by_key(|r| r.size.raw()).unwrap();
        assert!(
            small.size.raw() < big.size.raw(),
            "superscript shapes smaller ({} < {})",
            small.size.raw(),
            big.size.raw()
        );
        assert!(
            small.origin.y.raw() < big.origin.y.raw(),
            "superscript is raised above the baseline ({} < {})",
            small.origin.y.raw(),
            big.origin.y.raw()
        );
    }

    #[test]
    fn subscript_lowers_the_run() {
        use casual_doc_model::v1::VerticalAlignment;
        let base = sized_run(11, "x", 24, RunProperties::default());
        let sub = sized_run(
            12,
            "2",
            24,
            RunProperties {
                vertical_alignment: Some(VerticalAlignment::Subscript),
                ..RunProperties::default()
            },
        );
        let doc = document(vec![paragraph(10, vec![base, sub])]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let runs = &first_paragraph_lines(&galley).lines[0].runs;
        let big = runs.iter().max_by_key(|r| r.size.raw()).unwrap();
        let small = runs.iter().min_by_key(|r| r.size.raw()).unwrap();
        assert!(
            small.size.raw() < big.size.raw(),
            "subscript shapes smaller"
        );
        assert!(
            small.origin.y.raw() > big.origin.y.raw(),
            "subscript is lowered below the baseline ({} > {})",
            small.origin.y.raw(),
            big.origin.y.raw()
        );
    }

    #[test]
    fn position_raises_the_baseline_without_resizing() {
        // `w:position` is a pure baseline offset — same size, only the origin moves.
        let base = sized_run(11, "x", 24, RunProperties::default());
        let raised = sized_run(
            12,
            "y",
            24,
            RunProperties {
                position_half_points: Some(6), // +6 half-points = +60 twips (raise)
                ..RunProperties::default()
            },
        );
        let doc = document(vec![paragraph(10, vec![base, raised])]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let runs = &first_paragraph_lines(&galley).lines[0].runs;
        assert!(runs.len() >= 2, "base + positioned are distinct runs");
        assert!(
            runs.iter().all(|r| r.size == runs[0].size),
            "w:position must not resize the run"
        );
        let min_y = runs.iter().map(|r| r.origin.y.raw()).min().unwrap();
        let max_y = runs.iter().map(|r| r.origin.y.raw()).max().unwrap();
        assert!(
            min_y < max_y,
            "the positioned run is raised above the baseline"
        );
    }

    /// The glyph-id sequence of a galley's first paragraph (visual order).
    fn glyph_ids(galley: &[BlockFragment]) -> Vec<u32> {
        first_paragraph_lines(galley)
            .lines
            .iter()
            .flat_map(|l| &l.runs)
            .flat_map(|r| &r.glyphs)
            .map(|g| g.id)
            .collect()
    }

    #[test]
    fn all_caps_shapes_like_pre_uppercased_text() {
        let shaper = ParleyShaper::new();
        let caps = document(vec![paragraph(
            10,
            vec![run_node(
                11,
                "abc",
                RunProperties {
                    all_caps: Some(true),
                    ..RunProperties::default()
                },
            )],
        )]);
        let plain = document(vec![paragraph(
            20,
            vec![run_node(21, "ABC", RunProperties::default())],
        )]);
        let caps_glyphs = glyph_ids(&build_galley(&caps, &shaper, Twip::from_points(400)));
        let plain_glyphs = glyph_ids(&build_galley(&plain, &shaper, Twip::from_points(400)));
        assert!(!caps_glyphs.is_empty(), "the all-caps run shaped to glyphs");
        assert_eq!(
            caps_glyphs, plain_glyphs,
            "all-caps `abc` shapes to the same glyphs as literal `ABC`"
        );
    }

    #[test]
    fn small_caps_splits_lowercase_at_three_quarter_size() {
        // `aB`: the originally-lowercase `a` shapes at 3/4 size, the `B` at full
        // size, so the run splits into two spans of distinct size.
        let doc = document(vec![paragraph(
            10,
            vec![sized_run(
                11,
                "aB",
                24, // 12pt -> 240 twips; lowercase span shapes at 180 twips (3/4)
                RunProperties {
                    small_caps: Some(true),
                    ..RunProperties::default()
                },
            )],
        )]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let runs = &first_paragraph_lines(&galley).lines[0].runs;
        let sizes: std::collections::BTreeSet<i32> = runs.iter().map(|r| r.size.raw()).collect();
        assert_eq!(
            sizes,
            [180, 240].into_iter().collect(),
            "the lowercase span is 3/4 size (180) and the rest full size (240): {sizes:?}"
        );
    }

    #[test]
    fn a_theme_colored_run_resolves_to_the_scheme_slot_not_black() {
        use casual_doc_model::v1::{
            Color, ColorScheme, RgbColor, SchemeColor, ThemeColor, ThemeColorRef,
        };
        // A distinct accent-1 slot color, referenced by a run's `w:themeColor`.
        let scheme = ColorScheme {
            accent1: SchemeColor::Srgb(RgbColor {
                r: 0x11,
                g: 0x22,
                b: 0x33,
            }),
            ..ColorScheme::default()
        };
        let defs = Definitions {
            color_scheme: Some(scheme),
            ..Definitions::default()
        };
        let props = RunProperties {
            color: Some(Color::Theme(ThemeColor {
                slot: ThemeColorRef::Accent1,
            })),
            ..RunProperties::default()
        };
        let doc = Document::new(
            NodeId::from_parts(1, 1).unwrap(),
            vec![paragraph(10, vec![run_node(11, "themed", props)])],
            defs,
        )
        .unwrap();
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let run = &first_paragraph_lines(&galley).lines[0].runs[0];
        assert_eq!(
            run.color,
            [0x11, 0x22, 0x33, 255],
            "the theme slot resolves to its real RGB, flowed end to end"
        );
        assert_ne!(
            run.color,
            [0, 0, 0, 255],
            "a resolvable theme color is not black"
        );
    }

    #[test]
    fn a_syscolor_slot_resolves_to_its_last_color() {
        use casual_doc_model::v1::{
            ColorScheme, RgbColor, SchemeColor, SystemColor, ThemeColorRef,
        };
        let scheme = ColorScheme {
            dark1: SchemeColor::System(SystemColor {
                value: "windowText".to_owned(),
                last_color: Some(RgbColor { r: 5, g: 6, b: 7 }),
            }),
            ..ColorScheme::default()
        };
        let palette = resolve_palette(&scheme);
        assert_eq!(
            palette.slot(ThemeColorRef::Dark1),
            [5, 6, 7, 255],
            "a sysClr slot resolves to its recorded lastClr"
        );
    }

    #[test]
    fn tint_lightens_and_shade_darkens_a_theme_color() {
        // shade 0.5 halves each channel toward black; tint 0.5 blends halfway to
        // white (100*0.5 + 255*0.5 = 177.5 -> 178); no factors is the identity.
        assert_eq!(
            apply_tint_shade([100, 100, 100, 255], None, Some(0.5)),
            [50, 50, 50, 255]
        );
        assert_eq!(
            apply_tint_shade([100, 100, 100, 255], Some(0.5), None),
            [178, 178, 178, 255]
        );
        assert_eq!(
            apply_tint_shade([12, 34, 56, 255], None, None),
            [12, 34, 56, 255]
        );
    }

    #[test]
    fn an_auto_or_unresolvable_color_is_black() {
        use casual_doc_model::v1::{Color, ThemeColor, ThemeColorRef};
        // An unset (`auto`) color is black.
        assert_eq!(run_color(None, None), [0, 0, 0, 255]);
        // A theme color with no palette available also falls back to black.
        assert_eq!(
            run_color(
                Some(Color::Theme(ThemeColor {
                    slot: ThemeColorRef::Dark1,
                })),
                None,
            ),
            [0, 0, 0, 255]
        );
    }

    #[test]
    fn a_start_end_indent_reduces_the_wrap_width_and_carries_into_box_metrics() {
        use casual_doc_model::v1::{Indentation, Paragraph, ParagraphProperties};
        let text = "Hello world this is a longer paragraph that wraps onto lines";
        let plain = document(vec![paragraph(
            10,
            vec![run_node(11, text, RunProperties::default())],
        )]);
        let indented = document(vec![BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(10, 1).unwrap(),
            properties: ParagraphProperties {
                indentation: Some(Indentation {
                    start_twips: Some(2000),
                    end_twips: Some(2000),
                    ..Indentation::default()
                }),
                ..ParagraphProperties::default()
            },
            inlines: vec![run_node(11, text, RunProperties::default())],
        })]);
        let shaper = ParleyShaper::new();
        let width = Twip::from_points(300);
        let plain_lines = {
            let g = build_galley(&plain, &shaper, width);
            let BlockFragment::Paragraph { lines, .. } = &g[0] else {
                panic!()
            };
            lines.lines.len()
        };
        let g = build_galley(&indented, &shaper, width);
        let BlockFragment::Paragraph {
            lines, box_metrics, ..
        } = &g[0]
        else {
            panic!()
        };
        assert_eq!(
            box_metrics.indent_start,
            Twip(2000),
            "the start indent is carried"
        );
        assert_eq!(
            box_metrics.indent_end,
            Twip(2000),
            "the end indent is carried"
        );
        assert!(
            lines.lines.len() > plain_lines,
            "the 4000-twip indent narrows the column so the text wraps into more lines \
             ({} vs {plain_lines})",
            lines.lines.len()
        );
    }

    #[test]
    fn a_highlighted_run_carries_its_resolved_fill_into_the_shaped_run() {
        use casual_doc_model::v1::HighlightColor;
        let props = RunProperties {
            highlight: Some(HighlightColor::Yellow),
            ..RunProperties::default()
        };
        let doc = document(vec![paragraph(10, vec![run_node(11, "lit", props)])]);
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!();
        };
        assert_eq!(
            lines.lines[0].runs[0].highlight,
            Some([255, 255, 0, 255]),
            "the yellow highlight resolves to RGBA and rides the shaped run"
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

    #[test]
    fn a_vanished_run_is_not_collected_into_styled_runs() {
        // `collect_runs` is the single funnel that turns inline runs into shaped
        // text; a `w:vanish` (hidden) run must be dropped here so it is never
        // shaped or painted.
        let hidden = RunProperties {
            hidden: Some(true),
            ..RunProperties::default()
        };
        let inlines = vec![
            run_node(1, "shown", RunProperties::default()),
            run_node(2, "secret", hidden),
        ];
        let resolver = FontResolver::new();
        let mut report = FontResolutionReport::new();
        let media = DefinitionMap::default();
        let mut ctx = FlowCtx {
            resolver: &resolver,
            scheme: None,
            report: &mut report,
            default_tab: crate::tabs::DEFAULT_TAB_STOP,
            media: &media,
            palette: None,
        };
        let mut runs = Vec::new();
        collect_runs(&inlines, &mut runs, &mut ctx);
        assert_eq!(runs.len(), 1, "only the visible run is collected");
        assert_eq!(&*runs[0].text, "shown");
    }

    #[test]
    fn a_vanished_run_produces_no_glyphs() {
        // End to end: a paragraph of one visible + one vanished run shapes to the
        // same glyphs as the visible run alone — the hidden text paints nothing.
        let shaper = ParleyShaper::new();

        let visible_only = document(vec![paragraph(
            10,
            vec![run_node(11, "shown", RunProperties::default())],
        )]);
        let g_visible = build_galley(&visible_only, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph {
            lines: visible_lines,
            ..
        } = &g_visible[0]
        else {
            panic!("expected a paragraph fragment");
        };
        let visible_glyphs: usize = visible_lines
            .lines
            .iter()
            .flat_map(|l| &l.runs)
            .map(|r| r.glyphs.len())
            .sum();

        let with_hidden = document(vec![paragraph(
            20,
            vec![
                run_node(21, "shown", RunProperties::default()),
                run_node(
                    22,
                    "invisible",
                    RunProperties {
                        hidden: Some(true),
                        ..RunProperties::default()
                    },
                ),
            ],
        )]);
        let g_hidden = build_galley(&with_hidden, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph {
            lines: hidden_lines,
            ..
        } = &g_hidden[0]
        else {
            panic!("expected a paragraph fragment");
        };
        let hidden_glyphs: usize = hidden_lines
            .lines
            .iter()
            .flat_map(|l| &l.runs)
            .map(|r| r.glyphs.len())
            .sum();

        assert!(visible_glyphs > 0, "the visible run shaped to glyphs");
        assert_eq!(
            hidden_glyphs, visible_glyphs,
            "the w:vanish run added no glyphs"
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
            grid_change: None,
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
            grid_change: None,
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
            grid_change: None,
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
            grid_change: None,
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
            grid_change: None,
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
            grid_change: None,
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
            grid_change: None,
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

    #[test]
    fn an_inline_drawing_becomes_an_image_paint_item_with_the_extent_derived_rect() {
        use crate::compose::compose_paragraph;
        use crate::display::PaintItem;
        use casual_doc_model::v1::{Drawing, Extent, MediaId, MediaReference};

        // A media table with one embedded PNG, referenced by an inline drawing.
        let media_id = MediaId::new(NodeId::from_parts(70, 1).unwrap());
        let mut media = DefinitionMap::default();
        media.insert(
            media_id,
            MediaReference {
                relationship_id: "rId7".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/image1.png".to_owned(),
            },
        );
        let definitions = Definitions {
            media,
            ..Definitions::default()
        };
        let para = BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(10, 1).unwrap(),
            properties: ParagraphProperties::default(),
            inlines: vec![InlineNode::Drawing(Drawing {
                id: NodeId::from_parts(11, 1).unwrap(),
                media: media_id,
                // 190500 × 127000 EMU (635 EMU/twip) → 300 × 200 twips.
                extent: Some(Extent {
                    width_emu: 190_500,
                    height_emu: 127_000,
                }),
            })],
        });
        let doc =
            Document::new(NodeId::from_parts(1, 1).unwrap(), vec![para], definitions).unwrap();

        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            panic!("expected a paragraph fragment");
        };
        // The drawing became an inline image box, sized from the EMU extent and
        // keyed by its package part name.
        let image = lines
            .lines
            .iter()
            .flat_map(|l| &l.images)
            .next()
            .expect("an inline image was placed");
        assert_eq!(image.media, "word/media/image1.png");
        assert_eq!(
            image.size,
            Size::new(Twip(300), Twip(200)),
            "190500×127000 EMU resolves to 300×200 twips"
        );

        // And it composes to a `PaintItem::Image` carrying that extent-derived rect.
        let list = compose_paragraph(lines, Point::new(Twip::ZERO, Twip::ZERO));
        let rect = list
            .items
            .iter()
            .find_map(|item| match item {
                PaintItem::Image { media, rect } if media == "word/media/image1.png" => Some(*rect),
                _ => None,
            })
            .expect("an image paint item");
        assert_eq!(
            rect.size,
            Size::new(Twip(300), Twip(200)),
            "the paint rect carries the extent-derived size"
        );
    }

    /// The first inline text box placed anywhere in a paragraph fragment's lines.
    fn text_box_of(fragment: &BlockFragment) -> &InlineTextBox {
        let BlockFragment::Paragraph { lines, .. } = fragment else {
            panic!("expected a paragraph fragment");
        };
        lines
            .lines
            .iter()
            .flat_map(|l| &l.text_boxes)
            .next()
            .expect("a text box was placed inline")
    }

    #[test]
    fn a_text_box_flows_its_paragraph_and_composes_text_inside_a_bordered_box() {
        use crate::compose::compose_paragraph;
        use crate::display::PaintItem;
        use casual_doc_model::v1::TextBox;

        // A paragraph whose only inline is a text box holding one paragraph.
        let text_box = InlineNode::TextBox(TextBox {
            id: NodeId::from_parts(20, 1).unwrap(),
            blocks: vec![paragraph(
                21,
                vec![run_node(22, "boxed", RunProperties::default())],
            )],
        });
        let shaper = ParleyShaper::new();
        let galley = build_galley(
            &document(vec![paragraph(10, vec![text_box])]),
            &shaper,
            Twip::from_points(400),
        );

        // The box was placed as an inline box on its own line, carrying the flowed
        // block content of the box (the uniform-flow pipeline).
        let tb = text_box_of(&galley[0]);
        assert_eq!(tb.blocks.len(), 1, "the box flowed its one inner paragraph");
        let BlockFragment::Paragraph { lines: inner, .. } = &tb.blocks[0] else {
            panic!("the box's content is a paragraph fragment");
        };
        assert!(
            inner.lines.iter().any(|l| !l.runs.is_empty()),
            "the inner paragraph shaped glyphs"
        );
        assert!(tb.border.is_some(), "the box carries a default border");
        assert!(
            tb.size.height.raw() > inner.height().raw(),
            "the box is taller than its content by the top/bottom margins"
        );

        // Composition paints the border (a stroked rect) and the inner glyphs.
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            unreachable!()
        };
        let list = compose_paragraph(lines, Point::new(Twip::ZERO, Twip::ZERO));
        assert!(
            list.items.iter().any(|i| matches!(
                i,
                PaintItem::Rect {
                    stroke: Some(_),
                    ..
                }
            )),
            "the box border paints as a stroked rect"
        );
        assert!(
            list.items
                .iter()
                .any(|i| matches!(i, PaintItem::Glyphs { .. })),
            "the box's text paints inside it"
        );
    }

    #[test]
    fn a_text_box_renders_a_nested_table_through_the_shared_pipeline() {
        use crate::compose::compose_paragraph;
        use crate::display::PaintItem;
        use casual_doc_model::v1::{
            GridColumn, Table, TableCell, TableCellProperties, TableProperties, TableRow,
            TableRowProperties, TextBox,
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
                    width_twips: Some(2000),
                },
                GridColumn {
                    width_twips: Some(2000),
                },
            ],
            properties: TableProperties::default(),
            rows: vec![TableRow {
                id: NodeId::from_parts(51, 1).unwrap(),
                properties: TableRowProperties::default(),
                cells: vec![cell(60, "a"), cell(61, "b")],
            }],
        });
        let text_box = InlineNode::TextBox(TextBox {
            id: NodeId::from_parts(20, 1).unwrap(),
            blocks: vec![table],
        });
        let shaper = ParleyShaper::new();
        let galley = build_galley(
            &document(vec![paragraph(10, vec![text_box])]),
            &shaper,
            Twip::from_points(400),
        );

        // The nested table expanded to a row fragment inside the box — exactly what
        // the body pipeline produces for a table.
        let tb = text_box_of(&galley[0]);
        assert!(
            matches!(tb.blocks[0], BlockFragment::TableRow { .. }),
            "the nested table flowed to a table-row fragment"
        );

        // Composition emits both cells' text inside the box.
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            unreachable!()
        };
        let list = compose_paragraph(lines, Point::new(Twip::ZERO, Twip::ZERO));
        let glyph_runs = list
            .items
            .iter()
            .filter(|i| matches!(i, PaintItem::Glyphs { .. }))
            .count();
        assert!(
            glyph_runs >= 2,
            "both nested cells' text paints inside the box"
        );
    }

    #[test]
    fn a_text_box_renders_an_inline_image_through_the_shared_pipeline() {
        use crate::compose::compose_paragraph;
        use crate::display::PaintItem;
        use casual_doc_model::v1::{Drawing, Extent, MediaId, MediaReference, TextBox};

        let media_id = MediaId::new(NodeId::from_parts(70, 1).unwrap());
        let mut media = DefinitionMap::default();
        media.insert(
            media_id,
            MediaReference {
                relationship_id: "rId7".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/image1.png".to_owned(),
            },
        );
        let definitions = Definitions {
            media,
            ..Definitions::default()
        };
        let inner_para = BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(21, 1).unwrap(),
            properties: ParagraphProperties::default(),
            inlines: vec![InlineNode::Drawing(Drawing {
                id: NodeId::from_parts(22, 1).unwrap(),
                media: media_id,
                extent: Some(Extent {
                    width_emu: 190_500,
                    height_emu: 127_000,
                }),
            })],
        });
        let para = BlockNode::Paragraph(Paragraph {
            id: NodeId::from_parts(10, 1).unwrap(),
            properties: ParagraphProperties::default(),
            inlines: vec![InlineNode::TextBox(TextBox {
                id: NodeId::from_parts(20, 1).unwrap(),
                blocks: vec![inner_para],
            })],
        });
        let doc =
            Document::new(NodeId::from_parts(1, 1).unwrap(), vec![para], definitions).unwrap();
        let shaper = ParleyShaper::new();
        let galley = build_galley(&doc, &shaper, Twip::from_points(400));

        // The image inside the box resolved to its package part name.
        let tb = text_box_of(&galley[0]);
        let BlockFragment::Paragraph { lines: inner, .. } = &tb.blocks[0] else {
            panic!("the box's content is a paragraph fragment");
        };
        let image = inner
            .lines
            .iter()
            .flat_map(|l| &l.images)
            .next()
            .expect("an inline image was placed inside the box");
        assert_eq!(image.media, "word/media/image1.png");

        // And it composes to an image paint item inside the box.
        let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
            unreachable!()
        };
        let list = compose_paragraph(lines, Point::new(Twip::ZERO, Twip::ZERO));
        assert!(
            list.items.iter().any(|i| matches!(
                i,
                PaintItem::Image { media, .. } if media == "word/media/image1.png"
            )),
            "the box's image paints inside it"
        );
    }
}
