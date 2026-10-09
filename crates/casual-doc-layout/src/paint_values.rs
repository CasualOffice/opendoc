//! The values the page paints, for an exporter that flows instead of paginating.
//!
//! An HTML export has to show the formatting the canvas shows: the heading in
//! its style's colour, the table cell in its banded fill, the border the
//! conflict rules picked, the theme colour resolved against the document's own
//! scheme. Each of those is already decided once, here, for the renderer. This
//! module is the read-only door to those decisions — **not** a second
//! implementation of them — so the two outputs cannot drift apart: a fix to how
//! the page resolves a cell's fill is a fix to the export's fill as well
//! (`docs/08` ADR-066).
//!
//! Everything here is a thin delegation to the function the flow engine itself
//! calls. Nothing paginates or shapes text; the one measurement is a chart
//! label's width, estimated ([`chart_drawing`]) because the output sets the
//! label itself.

use casual_doc_model::v1::{
    BorderEdge, Chart, Color, Document, GroupShape, GroupTextBox, HighlightColor, Indentation,
    NoteId, NoteKind, ParagraphProperties, Shading, Table, TableBorders, TableCell,
    TableCellProperties, TableProperties,
};

use crate::block::{CellContentMargins, ResolvedEdge};
use crate::cascade::{StyleCascade, TableStyleLayer};
use crate::chart::ChartLabel;
use crate::flow::{self, ResolvedPalette};
use crate::page::AnchorContent;
use crate::text::{ChartPrimitive, Decoration, FontId, GlyphRun};
use crate::units::{Point, Rect, Size, Twip};

/// The document's theme palette, resolved once, with the colour questions the
/// renderer answers through it.
///
/// Cheap to copy. A document that declares no `a:clrScheme` has no palette, and
/// every answer is then what the page paints in that case (a theme colour on a
/// run is black, a theme fill is no fill).
#[derive(Clone, Copy, Debug)]
pub struct PaintPalette {
    palette: Option<ResolvedPalette>,
}

impl PaintPalette {
    /// Resolves the document's theme colour scheme. O(1).
    #[must_use]
    pub fn new(document: &Document) -> Self {
        Self {
            palette: document
                .definitions()
                .color_scheme
                .as_ref()
                .map(flow::resolve_palette),
        }
    }

    /// A run's text colour as opaque RGBA: explicit sRGB, a theme slot with its
    /// tint and shade, or black for `auto` and for no colour at all.
    #[must_use]
    pub fn run_color(&self, color: Option<Color>) -> [u8; 4] {
        flow::run_color(color, self.palette.as_ref())
    }

    /// A `w:shd` fill as opaque RGBA, or `None` when nothing is filled.
    #[must_use]
    pub fn shading(&self, shading: &Shading) -> Option<[u8; 4]> {
        if shading.fill_none {
            return None;
        }
        flow::shading_rgba(shading, self.palette.as_ref())
    }

    /// The highlighter colour of a `w:highlight` slot, or `None` for `none`.
    #[must_use]
    pub fn highlight(&self, highlight: HighlightColor) -> Option<[u8; 4]> {
        flow::highlight_rgba(highlight)
    }

    /// The border that wins among `candidates` under OOXML conflict
    /// resolution, as the renderer draws it, or `None` when none is visible or
    /// an explicit `nil` suppresses the edge.
    #[must_use]
    pub fn edge(&self, candidates: &[Option<&BorderEdge>]) -> Option<ResolvedEdge> {
        flow::resolve_edge(candidates, self.palette.as_ref())
    }

    /// A table cell's fill: the cell's own shading, then its table-style
    /// layer, then the table's shading, with `w:fill="auto"` cancelling the
    /// chain.
    #[must_use]
    pub fn cell_fill(
        &self,
        table: &Table,
        cell: &TableCell,
        layer: &TableStyleLayer,
    ) -> Option<[u8; 4]> {
        flow::cell_shading_rgba(table, cell, layer, self.palette.as_ref())
    }
}

/// The table-style layer of every cell, `[row][cell]`, from the table's style,
/// `w:tblLook` and each row's and cell's `w:cnfStyle` — the layer the page
/// paints the cell with. O(cells × style chain).
#[must_use]
pub fn table_cell_layers(table: &Table, cascade: &StyleCascade<'_>) -> Vec<Vec<TableStyleLayer>> {
    flow::resolve_table_style_layers(table, cascade)
}

/// Each cell's four border candidates, `[row][cell]`, before adjacent-cell
/// conflict resolution: the cell's own edge where it sets one, otherwise the
/// table's outer edge on the perimeter and its inside edge elsewhere. O(cells).
#[must_use]
pub fn table_cell_borders(
    table: &Table,
    layers: &[Vec<TableStyleLayer>],
) -> Vec<Vec<TableBorders>> {
    flow::resolve_table_border_candidates(table, layers)
}

/// A cell's content insets: the cell's `w:tcMar`, then the table's
/// `w:tblCellMar`, then Word's built-in defaults. O(1).
#[must_use]
pub fn cell_margins(cell: &TableCellProperties, table: &TableProperties) -> CellContentMargins {
    flow::resolve_cell_margins(&cell.margins, &table.cell_margins)
}

/// A list paragraph's indentation: the paragraph's own (direct and style) per
/// field, with the numbering level's filling in what it leaves unset — the
/// level is lower precedence. O(1).
#[must_use]
pub fn list_indent(level: Indentation, paragraph: Option<Indentation>) -> Indentation {
    flow::merge_indent_over(level, paragraph)
}

/// A list marker's text with Symbol/Wingdings code points mapped to the Unicode
/// glyphs the page draws (Symbol `U+F0B7` → `•`), given the marker run's requested family.
/// The flag says whether anything was remapped (the symbol face is then not the
/// face to draw it in). O(marker length).
#[must_use]
pub fn marker_glyphs(text: &str, family: Option<&str>) -> (String, bool) {
    flow::map_marker_glyphs(text, family)
}

/// The space a paragraph's `w:beforeAutospacing`/`w:afterAutospacing` stands
/// for, in twips: derived from the paragraph mark's size, as the page does. O(1).
#[must_use]
pub fn auto_paragraph_space_twips(paragraph: &ParagraphProperties) -> i32 {
    flow::auto_paragraph_space(paragraph).raw()
}

/// Each referenced note's display label (`1`, `ii`, `*`), as the reference in
/// the body and the number at the head of the note print it.
///
/// Resolved without pages: a section that restarts numbering on every page
/// numbers continuously instead, which is the honest answer for an output that
/// has no pages. O(body).
#[must_use]
pub fn note_labels(document: &Document) -> NoteLabels {
    NoteLabels(crate::note_numbering::resolve_note_labels(document, None))
}

/// The display labels of a document's referenced notes.
#[derive(Clone, Debug, Default)]
pub struct NoteLabels(crate::note_numbering::NoteLabels);

impl NoteLabels {
    /// The label of one note, or `None` when the body never references it.
    #[must_use]
    pub fn label(&self, kind: NoteKind, note: NoteId) -> Option<&str> {
        self.0.label(kind, note)
    }
}

/// What the page draws for a drawn shape laid into `rect` (twips): its preset
/// or custom geometry evaluated at that box, with its fill and outline —
/// direct, or from the theme's format scheme. O(geometry).
#[must_use]
pub fn shape_content(document: &Document, shape: &GroupShape, rect: Rect) -> AnchorContent {
    let (fill, stroke) = crate::anchor::themed_appearance(shape, document.definitions());
    crate::anchor::geometry_content(
        crate::anchor::GeometryRef::of_shape(shape),
        rect,
        fill.as_ref(),
        stroke,
    )
}

/// The frame the page draws behind a grouped text box's text, laid into
/// `rect` (twips): its geometry with its fill and outline. O(geometry).
#[must_use]
pub fn text_box_frame(text_box: &GroupTextBox, rect: Rect) -> AnchorContent {
    crate::anchor::geometry_content(
        crate::anchor::GeometryRef::of_text_box(text_box, text_box.extent),
        rect,
        text_box.fill.as_ref(),
        text_box.border,
    )
}

/// A chart composed as the page composes it, for an output that sets its own
/// text.
#[derive(Clone, Debug)]
pub struct ChartDrawing {
    /// The primitives, box-local, in paint order.
    pub primitives: Vec<ChartPrimitive>,
    /// Each label's text. A [`ChartPrimitive::Text`] run's `font` is the index
    /// of its label here, not a face: the runs carry no glyphs.
    pub labels: Vec<String>,
    /// The size every label is set at.
    pub label_size: Twip,
}

/// Composes `chart` into `size` (twips) with the page's own composer, palette
/// and label size. The page shapes each label to place it; this measures one
/// at an average advance instead (`LABEL_ADVANCE_PER_MILLE`), so an output that sets
/// the text itself places it within a few percent of the page.
///
/// Complexity: O(points + labels).
#[must_use]
pub fn chart_drawing(document: &Document, chart: &Chart, size: Size) -> ChartDrawing {
    let palette = document
        .definitions()
        .color_scheme
        .as_ref()
        .map(flow::resolve_palette);
    // A document with no theme still draws its chart in Word's Office theme.
    let chart_palette = palette.as_ref().or(Some(&flow::OFFICE_PALETTE));
    let style = flow::chart_style(chart_palette);
    let colors = |color: Color| flow::run_color(Some(color), chart_palette);
    let label_size = Twip(flow::CHART_LABEL_HALF_POINTS as i32 * 10);
    let mut labels: Vec<String> = Vec::new();
    let primitives = {
        let mut shape = |text: &str| -> Option<ChartLabel> {
            let index = u32::try_from(labels.len()).ok()?;
            labels.push(text.to_owned());
            let advance = label_size.raw() * LABEL_ADVANCE_PER_MILLE / 1000;
            let width = Twip(advance.saturating_mul(text.chars().count() as i32));
            let ascent = Twip(label_size.raw() * 4 / 5);
            let descent = Twip(label_size.raw() / 5);
            Some(ChartLabel {
                runs: vec![GlyphRun {
                    font: FontId(index),
                    size: label_size,
                    ascent,
                    descent,
                    character_scale_percent: 100,
                    color: style.text,
                    origin: Point::new(Twip::ZERO, Twip::ZERO),
                    bidi_level: 0,
                    decoration: Decoration::default(),
                    highlight: None,
                    shading: None,
                    glyphs: Vec::new(),
                    is_marker: false,
                    node: None,
                    is_leader: false,
                }],
                width,
                ascent,
                descent,
            })
        };
        crate::chart::compose_chart(chart, size, &style, &colors, &mut shape)
    };
    ChartDrawing {
        primitives,
        labels,
        label_size,
    }
}

/// The average advance of a chart label's characters, in thousandths of the
/// label size: digits and the short words axes carry are a little over half
/// an em in the faces documents use.
const LABEL_ADVANCE_PER_MILLE: i32 = 550;
