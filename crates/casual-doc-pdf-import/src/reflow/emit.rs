//! The flow to a `v1::Document`: paragraphs (joined across elements),
//! headings and the style sheet, lists and numbering, tables, footnotes,
//! comments, hyperlinks and bookmarks, header/footer parts and column
//! sections.
//!
//! Complexity: O(characters + elements log elements).

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::v1::{
    AbstractNumbering, AbstractNumberingId, Alignment, BlockNode, Bookmark, BookmarkEnd,
    BookmarkId, BookmarkStart, Color, Comment, CommentId, CommentRangeEnd, CommentRangeStart,
    CommentReference, Definitions, Document, Drawing, Extent, ExternalTarget, Field, FieldKind,
    FieldUpdateState, FontName, FontRef, GridColumn, HeaderFooter, HeaderFooterId,
    HeaderFooterKind, HeaderFooterRef, Hyperlink, HyperlinkTarget, Indentation, InlineNode,
    InternalTarget, LevelJustification, LevelSuffix, MediaId, MediaReference, Note, NoteId,
    NoteKind, NoteNumberMark, NoteReference, NumberFormat, NumberingInstance, NumberingInstanceId,
    NumberingLevel, NumberingRef, PageMargins, PageSize, Paragraph, ParagraphProperties, RgbColor,
    Run, RunProperties, SectionBoundary, SectionColumns, SectionId, SectionType, Spacing, Style,
    StyleId, StyleKind, Tab, TabAlignment, TabStop, Table, TableCell, TableCellProperties,
    TableProperties, TableRow, TableRowProperties, TableWidth, VerticalAlignment, VerticalMerge,
    WidthType,
};
use casual_doc_model::v1::{BorderEdge, NoteNumberRestart, NoteProperties, TableBorders};
// Own line: cell shading from the PDF's filled rectangles.
use casual_doc_model::v1::Shading;
// Own line: floating pictures and vector art.
use casual_doc_model::v1::{
    AnchorHorizontal, AnchorVertical, AnchoredDrawing, DrawingAnchor, HorizontalAnchor,
    HorizontalPosition, VerticalAnchor, VerticalPosition, WrapDistances, WrapMode,
};
use casual_doc_model::{IdGenerator, NodeId};
use casual_doc_pdf_read::{LinkTarget, RenderMode};

use super::tables::ITable;
use super::text::{self, FontKey, Marker, StyleSheet};
use super::{EKind, Flow, ILine, PChar, PdfSourceState, ReflowWork, Reflowed, Variant};
use super::{Float, FloatWrap, Graphic, SourceImage, SourceVector};
use crate::PdfImportError;
use crate::report::{self, Findings, PdfModelOutcome as Outcome};

fn twips(points: f64) -> i32 {
    (points * 20.0).round() as i32
}

/// Word's default distance between a square-wrapped float and the text
/// beside it (`distL`/`distR`, 0.125 in).
const FLOAT_SIDE_GAP: f64 = 9.0;

/// A float's anchor: `dx` from its column's left, `dy` from its paragraph's
/// top. `None` is vector art on its own line (`wp:wrapTopAndBottom`). O(1).
fn float_anchor(dx: f64, dy: f64, wrap: Option<FloatWrap>) -> DrawingAnchor {
    let emu = crate::fixed::emu;
    DrawingAnchor {
        horizontal: AnchorHorizontal {
            relative_from: HorizontalAnchor::Column,
            position: HorizontalPosition::Offset(emu(dx)),
        },
        vertical: AnchorVertical {
            relative_from: VerticalAnchor::Paragraph,
            position: VerticalPosition::Offset(emu(dy)),
        },
        wrap: match wrap {
            Some(FloatWrap::Beside) => WrapMode::Square,
            Some(FloatWrap::Behind) => WrapMode::None,
            None => WrapMode::TopAndBottom,
        },
        wrap_text: None,
        wrap_distances: if wrap == Some(FloatWrap::Beside) {
            WrapDistances {
                top_emu: 0,
                bottom_emu: 0,
                start_emu: emu(FLOAT_SIDE_GAP),
                end_emu: emu(FLOAT_SIDE_GAP),
            }
        } else {
            WrapDistances::default()
        },
        wrap_polygon: None,
        behind_doc: wrap == Some(FloatWrap::Behind),
    }
}

/// One paragraph before emission.
#[derive(Clone, Debug, Default)]
struct IPara {
    chars: Vec<PChar>,
    lines: usize,
    page: usize,
    /// The first line's top (floats are placed from it).
    top: f64,
    first_bottom: f64,
    first_x0: f64,
    rest_x0: Option<f64>,
    column_left: f64,
    column_right: f64,
    region: Option<usize>,
    struct_element: Option<u32>,
    role: Option<String>,
    centred: bool,
    end_aligned: bool,
    /// Every line but the last runs from the column's left to its right.
    justified: bool,
    /// The page before ended early: this paragraph starts a new page.
    page_break: bool,
    list: Option<(NumberingInstanceId, u8)>,
    marker_len: usize,
    heading: Option<u8>,
    bookmark: Vec<usize>,
    last_line: Option<ILine>,
    /// The outermost tagged `L` this paragraph sits in, and how many `L`
    /// elements deep it is.
    list_root: Option<u32>,
    list_depth: u8,
    /// Pictures and vector art floated on this paragraph.
    floats: Vec<Float>,
}

// Transient (one per paragraph, dropped at emission), so the size of the
// paragraph variant costs nothing worth a box per paragraph.
#[allow(clippy::large_enum_variant)]
enum Item {
    Para(IPara),
    Table(ITable, Option<usize>),
    Image(SourceImage, Option<usize>),
    /// Vector art on its own line.
    Vector(SourceVector, Option<usize>),
    /// Floats whose text produced no paragraph of its own (a tagged table's
    /// cells): carried by an empty paragraph.
    Floats(Vec<Float>, Option<usize>),
}

impl Item {
    fn region(&self) -> Option<usize> {
        match self {
            Self::Para(p) => p.region,
            Self::Table(_, r) | Self::Image(_, r) | Self::Vector(_, r) | Self::Floats(_, r) => *r,
        }
    }
}

struct Emitter<'a> {
    ids: IdGenerator,
    key: &'a dyn Fn(&PChar) -> FontKey,
    sheet: StyleSheet,
    styles: Vec<(StyleId, Style)>,
    heading_styles: Vec<StyleId>,
    heading_keys: Vec<FontKey>,
    normal: Option<StyleId>,
    definitions: Definitions,
    links: Vec<super::ILink>,
    bookmarks: BTreeMap<usize, (BookmarkId, String)>,
    comments: Vec<Option<(CommentId, super::IComment)>>,
    notes: Vec<Option<NoteId>>,
    /// The last float's stacking key (paint order).
    z: u32,
    /// Vector paths a clip cut into, and paths split over the command bound.
    vector_clipped: u32,
    vector_split: u32,
}

impl Emitter<'_> {
    fn id(&mut self) -> Result<NodeId, PdfImportError> {
        self.ids
            .next_id()
            .map_err(|error| PdfImportError::Model(error.to_string()))
    }

    /// A media entry for a picture's part (the part is the fixed build's,
    /// referenced, not copied). O(log media).
    fn media(&mut self, image: &SourceImage) -> Result<MediaId, PdfImportError> {
        let media = MediaId::new(self.id()?);
        let reference = MediaReference {
            relationship_id: image.media.relationship_id.clone(),
            media_type: image.media.media_type.clone(),
            part_name: image.media.part_name.clone(),
        };
        self.definitions.media.insert(media, reference);
        Ok(media)
    }

    /// Vector art as one anchored shape group (the fixed build's shapes).
    /// O(path commands).
    fn group(
        &mut self,
        vector: &SourceVector,
        anchor: DrawingAnchor,
    ) -> Result<Option<InlineNode>, PdfImportError> {
        let paths: Vec<&casual_doc_pdf_read::PathEvidence> = vector.paths.iter().collect();
        let built = crate::fixed::vector_children(&paths, &mut || self.id())?;
        self.vector_clipped += built.clipped;
        self.vector_split += built.split;
        let Some(bounds) = built.bounds else {
            return Ok(None);
        };
        self.z += 1;
        let id = self.id()?;
        Ok(Some(InlineNode::Group(Box::new(
            crate::fixed::vector_group(id, built.children, bounds, anchor, self.z),
        ))))
    }

    /// The floats of a paragraph whose first line's top is `top`, in the
    /// column starting at `left`, on `page`. A float from another page (its
    /// text continued a paragraph begun earlier) sits at the paragraph's
    /// start. O(floats + path commands).
    fn floats(
        &mut self,
        floats: &[Float],
        page: usize,
        top: f64,
        left: f64,
    ) -> Result<Vec<InlineNode>, PdfImportError> {
        let mut out = Vec::with_capacity(floats.len());
        for float in floats {
            let b = float.graphic.bounds();
            let (dx, dy) = if float.page == page {
                (b.x0 - left, b.y0 - top)
            } else {
                (0.0, 0.0)
            };
            let anchor = float_anchor(dx, dy, Some(float.wrap));
            match &float.graphic {
                Graphic::Image(image) => {
                    let media = self.media(image)?;
                    self.z += 1;
                    out.push(InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
                        id: self.id()?,
                        media,
                        extent: Extent {
                            width_emu: crate::fixed::emu(b.width()).max(1),
                            height_emu: crate::fixed::emu(b.height()).max(1),
                        },
                        anchor,
                        descr: None,
                        relative_height: Some(self.z),
                        crop: None,
                        opacity: None,
                        hyperlink: None,
                        border: None,
                        flip_h: false,
                        flip_v: false,
                        rotation: None,
                    })));
                }
                Graphic::Vector(vector) => {
                    if let Some(group) = self.group(vector, anchor)? {
                        out.push(group);
                    }
                }
            }
        }
        Ok(out)
    }

    fn style_key(&self, para: &IPara) -> Option<FontKey> {
        match para.heading {
            Some(level) => self.heading_keys.get(usize::from(level) - 1).cloned(),
            None => self.sheet.body.clone(),
        }
    }

    /// Direct run formatting where a character differs from its paragraph's
    /// style.
    fn run_properties(&self, c: &PChar, style: Option<&FontKey>) -> RunProperties {
        let key = (self.key)(c);
        let mut p = RunProperties::default();
        if style.is_none_or(|s| s.family != key.family) {
            let named = Some(FontRef::Named(FontName {
                name: key.family.clone(),
            }));
            p.font_ref = named.clone();
            p.font_ref_h_ansi = named.clone();
            p.font_ref_cs = named.clone();
            p.font_ref_east_asia = named;
        }
        if style.is_none_or(|s| s.half_points != key.half_points) && !c.sup {
            p.size_half_points = Some(key.half_points);
            p.size_complex_half_points = Some(key.half_points);
        }
        if style.is_none_or(|s| s.bold != key.bold) {
            p.bold = Some(key.bold);
        }
        if style.is_none_or(|s| s.italic != key.italic) {
            p.italic = Some(key.italic);
        }
        if let Some([r, g, b, _]) = c.style.fill
            && (r, g, b) != (0, 0, 0)
        {
            p.color = Some(Color::Rgb(RgbColor { r, g, b }));
        }
        if c.style.mode == RenderMode::Invisible {
            p.hidden = Some(true);
        }
        if c.sup {
            p.vertical_alignment = Some(VerticalAlignment::Superscript);
        }
        p
    }

    /// The inline content of a character sequence: runs split at style,
    /// links, notes and comment boundaries. O(characters).
    fn inlines(
        &mut self,
        chars: &[PChar],
        style: Option<&FontKey>,
    ) -> Result<Vec<InlineNode>, PdfImportError> {
        // Comment spans in this sequence.
        let mut first: BTreeMap<usize, usize> = BTreeMap::new();
        let mut last: BTreeMap<usize, usize> = BTreeMap::new();
        for (i, c) in chars.iter().enumerate() {
            for &k in &c.comments {
                first.entry(k).or_insert(i);
                last.insert(k, i);
            }
        }
        let mut out: Vec<InlineNode> = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            for (&k, &at) in &first {
                if at == i
                    && let Some(Some((id, _))) = self.comments.get(k)
                {
                    let id = *id;
                    out.push(InlineNode::CommentRangeStart(CommentRangeStart {
                        id: self.id()?,
                        comment: id,
                    }));
                }
            }
            let c = &chars[i];
            if let Some(note) = c.note {
                let mut j = i;
                while j < chars.len() && chars[j].note == Some(note) {
                    j += 1;
                }
                if let Some(Some(id)) = self.notes.get(note) {
                    let id = *id;
                    out.push(InlineNode::NoteReference(NoteReference {
                        id: self.id()?,
                        kind: NoteKind::Footnote,
                        note: id,
                    }));
                }
                self.close_comments(&mut out, &last, i, j)?;
                i = j;
                continue;
            }
            // A run: same properties, same link, no comment boundary inside.
            let properties = self.run_properties(c, style);
            let link = c.link;
            let mut j = i;
            let mut text_out = String::new();
            while j < chars.len() {
                let d = &chars[j];
                if j > i
                    && (d.note.is_some()
                        || d.link != link
                        || first.values().any(|&a| a == j)
                        || last.values().any(|&b| b + 1 == j)
                        || self.run_properties(d, style) != properties)
                {
                    break;
                }
                text_out.push_str(&d.text);
                j += 1;
            }
            let run = InlineNode::Run(Run {
                id: self.id()?,
                properties: properties.into(),
                text: text_out,
            });
            let node = match link
                .and_then(|l| self.links.get(l))
                .map(|l| l.target.clone())
            {
                Some(LinkTarget::Uri(url)) => InlineNode::Hyperlink(Box::new(Hyperlink {
                    id: self.id()?,
                    target: HyperlinkTarget::External(ExternalTarget { url, anchor: None }),
                    tooltip: None,
                    inlines: vec![run],
                    history: true,
                })),
                Some(LinkTarget::Page { .. }) => match link
                    .and_then(|l| self.bookmarks.get(&l))
                    .map(|(_, name)| name.clone())
                {
                    Some(name) => InlineNode::Hyperlink(Box::new(Hyperlink {
                        id: self.id()?,
                        target: HyperlinkTarget::Internal(InternalTarget { anchor: name }),
                        tooltip: None,
                        inlines: vec![run],
                        history: false,
                    })),
                    None => run,
                },
                _ => run,
            };
            // Adjacent pieces of one link share one hyperlink.
            match (out.last_mut(), node) {
                (Some(InlineNode::Hyperlink(prev)), InlineNode::Hyperlink(next))
                    if prev.target == next.target && link.is_some() =>
                {
                    prev.inlines.extend(next.inlines);
                }
                (_, node) => out.push(node),
            }
            self.close_comments(&mut out, &last, i, j)?;
            i = j;
        }
        Ok(normalize(out))
    }

    fn close_comments(
        &mut self,
        out: &mut Vec<InlineNode>,
        last: &BTreeMap<usize, usize>,
        from: usize,
        to: usize,
    ) -> Result<(), PdfImportError> {
        for (&k, &at) in last {
            if at >= from
                && at < to
                && let Some(Some((id, _))) = self.comments.get(k)
            {
                let id = *id;
                out.push(InlineNode::CommentRangeEnd(CommentRangeEnd {
                    id: self.id()?,
                    comment: id,
                }));
                out.push(InlineNode::CommentReference(CommentReference {
                    id: self.id()?,
                    comment: id,
                }));
            }
        }
        Ok(())
    }

    fn paragraph(&mut self, para: &IPara) -> Result<Paragraph, PdfImportError> {
        let style_key = self.style_key(para);
        let mut properties = ParagraphProperties {
            style_ref: match para.heading {
                Some(level) => self.heading_styles.get(usize::from(level) - 1).copied(),
                None => None,
            },
            ..ParagraphProperties::default()
        };
        if let Some((instance, level)) = para.list {
            properties.numbering = Some(NumberingRef { instance, level });
        } else if para.heading.is_none() {
            let start = para.rest_x0.unwrap_or(para.first_x0) - para.column_left;
            let first = para.rest_x0.map(|rest| para.first_x0 - rest);
            if start > 6.0 || first.is_some_and(|f| f.abs() > 1.0) {
                properties.indentation = Some(Indentation {
                    start_twips: (start > 6.0).then(|| twips(start)),
                    end_twips: None,
                    first_line_twips: first.filter(|f| *f > 1.0).map(twips),
                    hanging_twips: first.filter(|f| *f < -1.0).map(|f| twips(-f)),
                });
            }
            if para.centred {
                properties.alignment = Some(Alignment::Center);
                properties.indentation = None;
            } else if para.end_aligned {
                properties.alignment = Some(Alignment::End);
                properties.indentation = None;
            } else if para.justified {
                properties.alignment = Some(Alignment::Justify);
            }
            // Text beside a float starts where the float's wrap lets it, not
            // at an indent that only measured the float's width.
            if para.floats.iter().any(|f| f.wrap == FloatWrap::Beside) {
                properties.indentation = None;
            }
        }
        if para.page_break {
            properties.page_break_before = Some(true);
        }
        let chars = &para.chars[para.marker_len.min(para.chars.len())..];
        let start = chars
            .iter()
            .take_while(|c| c.text.trim().is_empty())
            .count();
        let mut inlines = Vec::new();
        for &dest in &para.bookmark {
            if let Some((bookmark, _)) = self.bookmarks.get(&dest).cloned() {
                inlines.push(InlineNode::BookmarkStart(BookmarkStart {
                    id: self.id()?,
                    bookmark,
                }));
                inlines.push(InlineNode::BookmarkEnd(BookmarkEnd {
                    id: self.id()?,
                    bookmark,
                }));
            }
        }
        let floats = self.floats(&para.floats, para.page, para.top, para.column_left)?;
        inlines.extend(floats);
        inlines.extend(self.inlines(&chars[start..], style_key.as_ref())?);
        Ok(Paragraph {
            id: self.id()?,
            properties: properties.into(),
            inlines,
        })
    }

    fn table(&mut self, table: &ITable) -> Result<Table, PdfImportError> {
        let mut rows = Vec::with_capacity(table.rows.len());
        // The rules' own thickness and colour (Word's floor is a quarter
        // point, `w:sz` 2), so a hairline grid stays a hairline.
        let border = BorderEdge {
            style: "single".to_owned(),
            size_eighth_points: Some(((table.border_width * 8.0).round() as u32).clamp(2, 96)),
            color: table.border_color.map(|[r, g, b]| RgbColor { r, g, b }),
            theme_color: None,
            space_points: None,
        };
        let body = self.sheet.body.clone();
        for (r, row) in table.rows.iter().enumerate() {
            let mut cells = Vec::with_capacity(row.len());
            for cell in row {
                let mut blocks = Vec::new();
                if cell.vmerge != Some(false) {
                    let lines: Vec<&ILine> = cell.lines.iter().collect();
                    for chunk in
                        lines.chunk_by(|a, b| b.top - a.bottom < 0.6 * (a.bottom - a.top).max(1.0))
                    {
                        let (chars, _) = text::join(chunk);
                        let id = self.id()?;
                        let inlines = self.inlines(&chars, body.as_ref())?;
                        blocks.push(BlockNode::Paragraph(Paragraph {
                            id,
                            properties: ParagraphProperties::default().into(),
                            inlines,
                        }));
                    }
                }
                if blocks.is_empty() {
                    blocks.push(BlockNode::Paragraph(Paragraph {
                        id: self.id()?,
                        properties: ParagraphProperties::default().into(),
                        inlines: Vec::new(),
                    }));
                }
                cells.push(TableCell {
                    id: self.id()?,
                    properties: TableCellProperties {
                        grid_span: (cell.span > 1).then_some(cell.span),
                        vertical_merge: cell.vmerge.map(|restart| {
                            if restart {
                                VerticalMerge::Restart
                            } else {
                                VerticalMerge::Continue
                            }
                        }),
                        width: Some(TableWidth {
                            value: twips(cell.x1 - cell.x0),
                            width_type: WidthType::Dxa,
                        }),
                        shading: Shading {
                            fill: cell.shading.map(|[r, g, b]| RgbColor { r, g, b }),
                            ..Shading::default()
                        },
                        ..TableCellProperties::default()
                    },
                    blocks,
                });
            }
            rows.push(TableRow {
                id: self.id()?,
                properties: TableRowProperties {
                    header: r < table.header_rows,
                    ..TableRowProperties::default()
                },
                cells,
            });
        }
        let borders = if table.ruled {
            TableBorders {
                top: Some(border.clone()),
                start: Some(border.clone()),
                bottom: Some(border.clone()),
                end: Some(border.clone()),
                inside_h: Some(border.clone()),
                inside_v: Some(border),
            }
        } else {
            TableBorders::default()
        };
        Ok(Table {
            id: self.id()?,
            grid: table
                .columns
                .iter()
                .map(|w| GridColumn {
                    width_twips: Some(twips(*w).max(1)),
                })
                .collect(),
            grid_change: None,
            properties: TableProperties {
                borders,
                ..TableProperties::default()
            },
            rows,
        })
    }
}

fn level(
    format: NumberFormat,
    text: String,
    level: u8,
    start: u16,
    indent: f64,
    hanging: f64,
) -> NumberingLevel {
    NumberingLevel {
        picture_bullet: None,
        level,
        start,
        num_fmt: Some(format),
        lvl_text: Some(text),
        lvl_jc: Some(LevelJustification::Start),
        suff: Some(LevelSuffix::Tab),
        is_lgl: false,
        paragraph_properties: Some(ParagraphProperties {
            indentation: Some(Indentation {
                start_twips: Some(twips(indent).max(0)),
                end_twips: None,
                first_line_twips: None,
                hanging_twips: Some(twips(hanging).max(0)),
            }),
            ..ParagraphProperties::default()
        }),
        run_properties: None,
        style_ref: None,
        lvl_restart: None,
        pstyle: None,
        template_code: None,
        tentative: false,
    }
}

fn style(kind: StyleKind, name: &str, paragraph: ParagraphProperties, run: RunProperties) -> Style {
    Style {
        kind,
        is_default: false,
        custom_style: false,
        name: Some(name.to_owned()),
        aliases: None,
        based_on: None,
        next: None,
        link: None,
        hidden: false,
        ui_priority: None,
        semi_hidden: false,
        unhide_when_used: false,
        q_format: true,
        locked: false,
        paragraph: Some(paragraph),
        run: Some(run),
        table: None,
        table_row: None,
        table_cell: None,
        conditional: Vec::new(),
    }
}

fn key_run(key: &FontKey) -> RunProperties {
    let named = Some(FontRef::Named(FontName {
        name: key.family.clone(),
    }));
    RunProperties {
        font_ref: named.clone(),
        font_ref_h_ansi: named.clone(),
        font_ref_cs: named.clone(),
        font_ref_east_asia: named,
        size_half_points: Some(key.half_points),
        size_complex_half_points: Some(key.half_points),
        bold: Some(key.bold),
        italic: Some(key.italic),
        ..RunProperties::default()
    }
}

/// The column holding `x` in `region` (the body when there is none):
/// `(left, right)`. O(1).
fn column(flow: &Flow, region: Option<usize>, x: f64, page_width: f64) -> (f64, f64) {
    match region.and_then(|r| flow.regions.get(r)) {
        Some(region) => {
            let c = region.columns;
            let step = c.width + c.gutter;
            let k = ((x - c.left + c.gutter / 2.0) / step).floor().max(0.0);
            let left = c.left + k * step;
            (left, left + c.width)
        }
        None => (
            flow.body.0,
            flow.body.2.max(flow.body.0 + 1.0).min(page_width),
        ),
    }
}

/// A paragraph's bounds, alignment and column from its lines.
fn shape(
    lines: &[&ILine],
    flow: &Flow,
    region: Option<usize>,
    page_width: f64,
) -> (f64, Option<f64>, f64, f64, f64, bool, bool) {
    let first_x0 = lines.first().map_or(0.0, |l| l.x0);
    let rest_x0 = (lines.len() > 1).then(|| {
        lines[1..]
            .iter()
            .map(|l| l.x0)
            .fold(f64::INFINITY, f64::min)
    });
    let right = lines.iter().map(|l| l.x1).fold(f64::NEG_INFINITY, f64::max);
    let (column_left, column_right) = column(flow, region, first_x0, page_width);
    let width = column_right - column_left;
    let centre = (column_left + column_right) / 2.0;
    let indented = first_x0 > column_left + 0.05 * width;
    let centred = indented
        && lines
            .iter()
            .all(|l| ((l.x0 + l.x1) / 2.0 - centre).abs() < 3.0)
        && right < column_right - 3.0;
    let end_aligned =
        indented && !centred && lines.iter().all(|l| (l.x1 - column_right).abs() < 2.0);
    (
        first_x0,
        rest_x0,
        right,
        column_left,
        column_right,
        centred,
        end_aligned,
    )
}

/// A page is "ended early" when its content stops above this share of the
/// body's height.
pub const PAGE_BREAK_FILL: f64 = 0.75;

/// Justified: at least three lines, every line but the last ending at the
/// same right edge (within a point) at the column's right, and the last line
/// shorter. Ragged-right text does not end every line within a point. O(lines).
pub(super) fn justified(lines: &[&ILine], column_left: f64, column_right: f64) -> bool {
    if lines.len() < 3 {
        return false;
    }
    let (body, last) = lines.split_at(lines.len() - 1);
    let right = body.iter().map(|l| l.x1).fold(f64::NEG_INFINITY, f64::max);
    let width = (column_right - column_left).max(1.0);
    body.iter().all(|l| (l.x1 - right).abs() <= 1.0)
        && column_right - right <= 0.05 * width
        && last[0].x1 < right - 2.0 * last[0].size.max(1.0)
}

/// Emits the document. O(characters + elements log elements).
#[allow(clippy::too_many_lines)]
pub(crate) fn emit(
    state: &PdfSourceState,
    resources: &BTreeMap<String, Vec<u8>>,
    flow: Flow,
    key: &dyn Fn(&PChar) -> FontKey,
    findings: &mut Findings,
    work: &mut ReflowWork,
    namespace: u64,
) -> Result<Reflowed, PdfImportError> {
    let mut items: Vec<Item> = Vec::new();
    let mut joins = 0;
    let mut dehyphenated = 0;
    let mut multi_line = 0;
    let page_width = flow.page_size.0;
    // Paragraphs: segment each text element, then join across elements.
    let mut last_page: Option<usize> = None;
    let mut page_breaks = 0;
    let mut justified_count = 0;
    let (_, body_top, _, body_bottom) = flow.body;
    for element in &flow.elements {
        // A page whose content stopped well above the body's bottom ended
        // in a break: the next page's first new paragraph starts a page.
        let new_page = last_page.is_some_and(|p| p != element.page);
        let broke = new_page
            && last_page
                .and_then(|p| flow.page_bottoms.get(p))
                .is_some_and(|bottom| {
                    *bottom < body_top + PAGE_BREAK_FILL * (body_bottom - body_top)
                });
        last_page = Some(element.page);
        match &element.kind {
            EKind::Text(lines) => {
                let (left, right) = {
                    let refs: Vec<&ILine> = lines.iter().collect();
                    let s = shape(&refs, &flow, element.region, page_width);
                    (s.3, s.4)
                };
                let segments = text::segment(lines, left, right, key);
                work.paragraphs += lines.len() as u64;
                // Its floats go on the first paragraph it starts.
                let mut floats = element.floats.clone();
                for (n, segment) in segments.iter().enumerate() {
                    let refs: Vec<&ILine> = segment.lines.iter().map(|&i| &lines[i]).collect();
                    // Continue the previous paragraph: the same structure
                    // element, or a run-on across a column or page.
                    if n == 0
                        && let Some(Item::Para(prev)) = items.last_mut()
                    {
                        let same_struct = element.struct_element.is_some()
                            && prev.struct_element == element.struct_element
                            && element.role.as_deref() != Some("LI");
                        let continued = same_struct
                            || prev.last_line.as_ref().is_some_and(|last| {
                                text::continues(last, prev.column_right, refs[0], left, key)
                            });
                        if continued {
                            let (more, removed) = text::join(&refs);
                            let (glued, removed2) = glue(std::mem::take(&mut prev.chars), more);
                            prev.chars = glued;
                            prev.lines += refs.len();
                            prev.last_line = refs.last().map(|l| (*l).clone());
                            dehyphenated += removed + removed2;
                            joins += 1;
                            continue;
                        }
                    }
                    let (chars, removed) = text::join(&refs);
                    dehyphenated += removed;
                    if refs.len() > 1 {
                        multi_line += 1;
                    }
                    let page_break = broke && n == 0;
                    page_breaks += u32::from(page_break);
                    let justified = {
                        let s = shape(&refs, &flow, element.region, page_width);
                        justified(&refs, s.3, s.4) && !s.5 && !s.6
                    };
                    justified_count += u32::from(justified);
                    let (first_x0, rest_x0, _, column_left, column_right, centred, end_aligned) =
                        shape(&refs, &flow, element.region, page_width);
                    let heading = element.role.as_deref().and_then(|role| match role {
                        "H1" | "H" | "Title" => Some(1),
                        "H2" => Some(2),
                        "H3" => Some(3),
                        "H4" => Some(4),
                        "H5" => Some(5),
                        "H6" => Some(6),
                        _ => None,
                    });
                    items.push(Item::Para(IPara {
                        chars,
                        lines: refs.len(),
                        page: element.page,
                        top: refs[0].top,
                        first_bottom: refs[0].bottom,
                        first_x0,
                        rest_x0,
                        column_left,
                        column_right,
                        region: element.region,
                        struct_element: element.struct_element,
                        role: element.role.clone(),
                        centred,
                        end_aligned,
                        heading,
                        last_line: refs.last().map(|l| (*l).clone()),
                        justified,
                        page_break,
                        list_root: element
                            .lineage
                            .iter()
                            .find(|n| n.role == "L")
                            .map(|n| n.ordinal),
                        list_depth: u8::try_from(
                            element.lineage.iter().filter(|n| n.role == "L").count(),
                        )
                        .unwrap_or(u8::MAX),
                        floats: std::mem::take(&mut floats),
                        ..IPara::default()
                    }));
                }
                // Every line continued the paragraph before: it carries them.
                if !floats.is_empty() {
                    match items.last_mut() {
                        Some(Item::Para(prev)) => prev.floats.append(&mut floats),
                        _ => items.push(Item::Floats(floats, element.region)),
                    }
                }
            }
            EKind::Table(table) => {
                if !element.floats.is_empty() {
                    items.push(Item::Floats(element.floats.clone(), element.region));
                }
                items.push(Item::Table(table.clone(), element.region));
            }
            EKind::Image(image) => items.push(Item::Image(image.clone(), element.region)),
            EKind::Vector(vector) => items.push(Item::Vector(vector.clone(), element.region)),
        }
    }
    findings.record(
        report::PAGE_BREAK_INFERRED,
        Outcome::Mapped,
        page_breaks,
        None,
    );
    findings.record(
        report::JUSTIFICATION_INFERRED,
        Outcome::Mapped,
        justified_count,
        None,
    );
    findings.record(
        report::PARAGRAPH_BOUNDARY_INFERRED,
        Outcome::Mapped,
        multi_line + joins,
        None,
    );
    findings.record(
        report::DEHYPHENATION_INFERRED,
        Outcome::Mapped,
        dehyphenated,
        None,
    );

    // Styles from the paragraphs.
    let mut stats = text::KeyStats::new();
    for item in &items {
        if let Item::Para(p) = item {
            text::count(&mut stats, &p.chars, p.lines, key);
        }
    }
    let sheet = text::sheet(&stats);

    let mut emitter = Emitter {
        ids: IdGenerator::new(namespace),
        key,
        sheet,
        styles: Vec::new(),
        heading_styles: Vec::new(),
        heading_keys: Vec::new(),
        normal: None,
        definitions: Definitions::default(),
        links: flow.links.clone(),
        bookmarks: BTreeMap::new(),
        comments: Vec::new(),
        notes: Vec::new(),
        z: 0,
        vector_clipped: 0,
        vector_split: 0,
    };

    // Lists, then headings for what is not a list.
    let list_items = tagged_lists(&mut items, &mut emitter)? + lists(&mut items, &mut emitter)?;
    findings.record(report::LIST_INFERRED, Outcome::Mapped, list_items, None);
    let mut headings = 0;
    for item in &mut items {
        if let Item::Para(p) = item {
            if p.list.is_some() {
                p.heading = None;
                continue;
            }
            // A tagged paragraph's role decides; font clusters only classify
            // untagged ones.
            if p.heading.is_none() && p.role.is_none() {
                p.heading =
                    text::dominant(p.chars.iter(), key).and_then(|k| emitter.sheet.level(&k));
                if p.heading.is_some() {
                    headings += 1;
                }
            }
        }
    }
    findings.record(report::HEADING_INFERRED, Outcome::Mapped, headings, None);
    // Every level used needs a style: a tagged level the fonts did not
    // produce takes the font of its first paragraph.
    emitter.heading_keys = emitter.sheet.headings.clone();
    let deepest = items
        .iter()
        .filter_map(|i| match i {
            Item::Para(p) => p.heading,
            _ => None,
        })
        .max()
        .unwrap_or(0);
    for level in emitter.heading_keys.len() + 1..=usize::from(deepest) {
        let from = items.iter().find_map(|i| match i {
            Item::Para(p) if p.heading == u8::try_from(level).ok() => {
                text::dominant(p.chars.iter(), key)
            }
            _ => None,
        });
        let fallback = emitter.sheet.body.clone().unwrap_or(FontKey {
            family: "Times New Roman".to_owned(),
            half_points: 24,
            bold: true,
            italic: false,
        });
        emitter.heading_keys.push(from.unwrap_or(fallback));
    }

    // The style sheet.
    if let Some(body) = emitter.sheet.body.clone() {
        let id = StyleId::new(emitter.id()?);
        let mut normal = style(
            StyleKind::Paragraph,
            "Normal",
            ParagraphProperties {
                spacing: Some(Spacing {
                    before_twips: Some(0),
                    after_twips: Some(twips(f64::from(body.half_points) / 2.0 * 0.6)),
                    ..Spacing::default()
                }),
                ..ParagraphProperties::default()
            },
            key_run(&body),
        );
        normal.is_default = true;
        emitter.styles.push((id, normal));
        emitter.normal = Some(id);
        let headings = emitter.heading_keys.clone();
        for (i, heading) in headings.iter().enumerate() {
            let hid = StyleId::new(emitter.id()?);
            let mut s = style(
                StyleKind::Paragraph,
                &format!("heading {}", i + 1),
                ParagraphProperties {
                    outline_level: u8::try_from(i).ok(),
                    keep_next: Some(true),
                    spacing: Some(Spacing {
                        before_twips: Some(twips(f64::from(heading.half_points) / 2.0 * 0.8)),
                        after_twips: Some(twips(f64::from(heading.half_points) / 2.0 * 0.3)),
                        ..Spacing::default()
                    }),
                    ..ParagraphProperties::default()
                },
                key_run(heading),
            );
            s.based_on = Some(id);
            s.next = Some(id);
            s.ui_priority = Some(9);
            emitter.styles.push((hid, s));
            emitter.heading_styles.push(hid);
        }
        findings.record(
            report::STYLE_INFERRED,
            Outcome::Mapped,
            1 + headings.len() as u32,
            None,
        );
    }

    // Bookmarks for internal links: the first paragraph at or below the
    // destination.
    let mut placed_bookmarks = 0;
    for (index, link) in flow.links.iter().enumerate() {
        if let LinkTarget::Page { page, top } = link.target {
            let target = items.iter_mut().find_map(|item| match item {
                Item::Para(p) if p.page == page && top.is_none_or(|t| p.first_bottom >= t) => {
                    Some(p)
                }
                _ => None,
            });
            match target {
                Some(p) => {
                    let name = format!("_PdfDest{}", index + 1);
                    let bookmark = BookmarkId::new(emitter.id()?);
                    emitter
                        .definitions
                        .bookmarks
                        .insert(bookmark, Bookmark { name: name.clone() });
                    emitter.bookmarks.insert(index, (bookmark, name));
                    p.bookmark.push(index);
                    placed_bookmarks += 1;
                }
                None => findings.record(report::LINK_UNRESOLVED, Outcome::Omitted, 1, Some(page)),
            }
        }
    }
    findings.record(
        report::BOOKMARK_INFERRED,
        Outcome::Mapped,
        placed_bookmarks,
        None,
    );
    let linked = items
        .iter()
        .filter_map(|i| if let Item::Para(p) = i { Some(p) } else { None })
        .flat_map(|p| p.chars.iter().filter_map(|c| c.link))
        .collect::<std::collections::BTreeSet<_>>();
    findings.record(
        report::HYPERLINK_INFERRED,
        Outcome::Mapped,
        linked.len() as u32,
        None,
    );
    let unplaced = flow
        .links
        .iter()
        .enumerate()
        .filter(|(i, l)| matches!(l.target, LinkTarget::Uri(_)) && !linked.contains(i))
        .count();
    findings.record(
        report::LINK_UNRESOLVED,
        Outcome::Omitted,
        unplaced as u32,
        None,
    );

    // Comments and notes get their ids before any reference is written.
    for comment in &flow.comments {
        let entry = if comment.text.trim().is_empty() {
            None
        } else {
            Some((CommentId::new(emitter.id()?), comment.clone()))
        };
        emitter.comments.push(entry);
    }
    for _ in &flow.notes {
        let id = NoteId::new(emitter.id()?);
        emitter.notes.push(Some(id));
    }

    // Sections: a new one wherever the column shape changes.
    let shape_of = |region: Option<usize>| -> (u16, i32) {
        region
            .and_then(|r| flow.regions.get(r))
            .map_or((1, 0), |r| (r.columns.count, twips(r.columns.gutter)))
    };
    let (page_w, page_h) = flow.page_size;
    let (bx0, by0, bx1, by1) = flow.body;
    let header_top = flow
        .furniture
        .parts
        .get(&(false, Variant::Default))
        .and_then(|part| part.rows.first())
        .and_then(|row| row.first())
        .map(|l| l.top);
    let footer_bottom = flow
        .furniture
        .parts
        .get(&(true, Variant::Default))
        .and_then(|part| part.rows.last())
        .and_then(|row| row.last())
        .map(|l| l.bottom);
    let margins = PageMargins {
        top_twips: twips(by0.max(0.0)),
        bottom_twips: twips((page_h - by1).max(0.0)),
        start_twips: twips(bx0.max(0.0)),
        end_twips: twips((page_w - bx1).max(0.0)),
        header_twips: Some(twips(header_top.unwrap_or(36.0).max(0.0))),
        footer_twips: Some(twips(footer_bottom.map_or(36.0, |b| page_h - b).max(0.0))),
        gutter_twips: None,
    };

    // Header and footer parts: default, even and first-page variants.
    let mut parts: Vec<(bool, HeaderFooterKind, HeaderFooterId, HeaderFooter)> = Vec::new();
    for ((footer, variant), part) in &flow.furniture.parts {
        let kind = match variant {
            Variant::Default => HeaderFooterKind::Default,
            Variant::Even => HeaderFooterKind::Even,
            Variant::First => HeaderFooterKind::First,
        };
        let (id, content) =
            match furniture_part(&mut emitter, &part.rows, &part.numbers, page_w, margins)? {
                Some(built) => built,
                // An empty first-page part: the title page shows nothing there.
                None => (
                    HeaderFooterId::new(emitter.id()?),
                    HeaderFooter {
                        blocks: vec![BlockNode::Paragraph(Paragraph {
                            id: emitter.id()?,
                            properties: ParagraphProperties::default().into(),
                            inlines: Vec::new(),
                        })],
                    },
                ),
            };
        parts.push((*footer, kind, id, content));
    }
    let refs = |footer: bool| -> Vec<HeaderFooterRef> {
        parts
            .iter()
            .filter(|p| p.0 == footer)
            .map(|p| HeaderFooterRef {
                kind: p.1,
                reference: p.2,
            })
            .collect()
    };
    let (header_refs, footer_refs) = (refs(false), refs(true));
    let title_page = flow.furniture.title_page.then_some(true);

    let mut body: Vec<BlockNode> = Vec::new();
    let mut sections: Vec<SectionBoundary> = Vec::new();
    let mut current_shape = items.first().map_or((1, 0), |i| shape_of(i.region()));
    let mut column_sections = 0;
    let section = |emitter: &mut Emitter<'_>,
                   shape: (u16, i32),
                   first: bool|
     -> Result<SectionBoundary, PdfImportError> {
        let id = SectionId::new(emitter.id()?);
        Ok(SectionBoundary {
            id,
            page_size: PageSize {
                width_twips: twips(page_w).clamp(1, 31_680),
                height_twips: twips(page_h).clamp(1, 31_680),
            },
            page_margins: margins,
            columns: SectionColumns {
                count: shape.0,
                space_twips: (shape.0 > 1).then_some(shape.1),
                separator: None,
                equal_width: (shape.0 > 1).then_some(true),
                columns: Vec::new(),
            },
            headers: header_refs.clone(),
            footers: footer_refs.clone(),
            section_type: (!first).then_some(SectionType::Continuous),
            title_page,
            vertical_alignment: None,
            page_numbering: casual_doc_model::v1::PageNumbering::default(),
            doc_grid: casual_doc_model::v1::DocGrid::default(),
            orientation: None,
            paper_source: casual_doc_model::v1::PaperSource::default(),
            page_borders: casual_doc_model::v1::PageBorders::default(),
            line_numbering: casual_doc_model::v1::LineNumbering::default(),
            watermark: None,
            footnote_props: casual_doc_model::v1::NoteProperties::default(),
            endnote_props: casual_doc_model::v1::NoteProperties::default(),
            text_direction: None,
            bidi: false,
            section_change: None,
        })
    };
    if current_shape.0 > 1 {
        column_sections += 1;
    }
    for item in &items {
        let shape = shape_of(item.region());
        if shape.0 != current_shape.0 && !body.is_empty() {
            let boundary = section(&mut emitter, current_shape, sections.is_empty())?;
            // The section ends at a paragraph carrying its break.
            let needs_carrier = !matches!(body.last(), Some(BlockNode::Paragraph(_)));
            if needs_carrier {
                body.push(BlockNode::Paragraph(Paragraph {
                    id: emitter.id()?,
                    properties: ParagraphProperties::default().into(),
                    inlines: Vec::new(),
                }));
            }
            if let Some(BlockNode::Paragraph(p)) = body.last_mut() {
                let mut properties = (*p.properties).clone();
                properties.section_break = Some(boundary.id);
                p.properties = properties.into();
            }
            sections.push(boundary);
            current_shape = shape;
            if shape.0 > 1 {
                column_sections += 1;
            }
        }
        match item {
            Item::Para(p) => {
                let paragraph = emitter.paragraph(p)?;
                body.push(BlockNode::Paragraph(paragraph));
            }
            Item::Table(t, _) => {
                let table = emitter.table(t)?;
                body.push(BlockNode::Table(Box::new(table)));
            }
            Item::Vector(vector, region) => {
                let (left, _) = column(&flow, *region, vector.bounds.x0, page_w);
                let anchor = float_anchor(vector.bounds.x0 - left, 0.0, None);
                let inlines = emitter.group(vector, anchor)?.into_iter().collect();
                body.push(BlockNode::Paragraph(Paragraph {
                    id: emitter.id()?,
                    properties: ParagraphProperties::default().into(),
                    inlines,
                }));
            }
            Item::Floats(floats, region) => {
                let (page, top, x0) = floats
                    .iter()
                    .map(|f| {
                        let b = f.graphic.bounds();
                        (f.page, b.y0, b.x0)
                    })
                    .fold((usize::MAX, f64::INFINITY, f64::INFINITY), |a, b| {
                        (a.0.min(b.0), a.1.min(b.1), a.2.min(b.2))
                    });
                let (left, _) = column(&flow, *region, x0, page_w);
                let inlines = emitter.floats(floats, page, top, left)?;
                body.push(BlockNode::Paragraph(Paragraph {
                    id: emitter.id()?,
                    properties: ParagraphProperties::default().into(),
                    inlines,
                }));
            }
            Item::Image(image, _) => {
                let media = emitter.media(image)?;
                let b = image.bounds;
                let drawing = Drawing {
                    id: emitter.id()?,
                    media,
                    extent: Some(Extent {
                        width_emu: ((b.x1 - b.x0) * 12_700.0).round().max(1.0) as i64,
                        height_emu: ((b.y1 - b.y0) * 12_700.0).round().max(1.0) as i64,
                    }),
                    descr: None,
                    crop: None,
                    opacity: None,
                    hyperlink: None,
                    border: None,
                    flip_h: false,
                    flip_v: false,
                    rotation: None,
                };
                body.push(BlockNode::Paragraph(Paragraph {
                    id: emitter.id()?,
                    properties: ParagraphProperties::default().into(),
                    inlines: vec![InlineNode::Drawing(Box::new(drawing))],
                }));
            }
        }
    }
    findings.record(
        report::COLUMNS_INFERRED,
        Outcome::Mapped,
        column_sections,
        None,
    );
    findings.record(
        report::CLIP_IGNORED,
        Outcome::Degraded,
        emitter.vector_clipped,
        None,
    );
    findings.record(
        report::VECTOR_SPLIT,
        Outcome::Degraded,
        emitter.vector_split,
        None,
    );
    let last = section(&mut emitter, current_shape, sections.is_empty())?;
    sections.push(last);
    // Footnote numbering that reproduces the PDF's own markers.
    match note_numbering(&flow.notes) {
        Some(props) => {
            for section in &mut sections {
                section.footnote_props = props.clone();
            }
        }
        None => findings.record(
            report::FOOTNOTE_MARKER_RENUMBERED,
            Outcome::Degraded,
            flow.notes.len() as u32,
            None,
        ),
    }
    if body.is_empty() {
        body.push(BlockNode::Paragraph(Paragraph {
            id: emitter.id()?,
            properties: ParagraphProperties::default().into(),
            inlines: Vec::new(),
        }));
    }

    // Notes.
    let body_key = emitter.sheet.body.clone();
    for (i, note) in flow.notes.iter().enumerate() {
        let Some(Some(id)) = emitter.notes.get(i).copied() else {
            continue;
        };
        let refs: Vec<&ILine> = note.lines.iter().collect();
        let (chars, _) = text::join(&refs);
        let note_key = text::dominant(chars.iter(), key);
        let mut inlines = vec![InlineNode::NoteNumberMark(NoteNumberMark {
            id: emitter.id()?,
            kind: NoteKind::Footnote,
            properties: RunProperties {
                vertical_alignment: Some(VerticalAlignment::Superscript),
                ..RunProperties::default()
            }
            .into(),
        })];
        inlines.push(InlineNode::Run(Run {
            id: emitter.id()?,
            properties: RunProperties::default().into(),
            text: " ".to_owned(),
        }));
        let start = chars
            .iter()
            .take_while(|c| c.text.trim().is_empty())
            .count();
        inlines.extend(emitter.inlines(&chars[start..], note_key.as_ref().or(body_key.as_ref()))?);
        let paragraph = Paragraph {
            id: emitter.id()?,
            properties: ParagraphProperties {
                spacing: Some(Spacing {
                    before_twips: Some(0),
                    after_twips: Some(0),
                    ..Spacing::default()
                }),
                ..ParagraphProperties::default()
            }
            .into(),
            inlines: normalize(inlines),
        };
        // A note's run size is its own: set it on every run that has none.
        emitter.definitions.footnotes.insert(
            id,
            Note {
                blocks: vec![BlockNode::Paragraph(set_size(paragraph, note_key.as_ref()))],
            },
        );
    }

    // Comments that were anchored, threaded by `/IRT` through `para_id`s.
    let referenced: std::collections::BTreeSet<CommentId> = collect_comment_refs(&body);
    let mut threads: BTreeMap<(i32, i32), String> = BTreeMap::new();
    let mut thread_count = 0usize;
    for (id, comment) in emitter.comments.clone().into_iter().flatten() {
        if !referenced.contains(&id) {
            continue;
        }
        thread_count += 1;
        let para_id = format!("{thread_count:08X}");
        let parent_para_id = comment
            .reply_to
            .and_then(|parent| threads.get(&parent).cloned());
        if let Some(object) = comment.object {
            threads.insert(object, para_id.clone());
        }
        let paragraphs: Vec<BlockNode> = comment
            .text
            .split('\n')
            .map(|line| -> Result<BlockNode, PdfImportError> {
                Ok(BlockNode::Paragraph(Paragraph {
                    id: emitter.id()?,
                    properties: ParagraphProperties::default().into(),
                    inlines: vec![InlineNode::Run(Run {
                        id: emitter.id()?,
                        properties: RunProperties::default().into(),
                        text: line.to_owned(),
                    })],
                }))
            })
            .collect::<Result<_, _>>()?;
        let author = comment
            .author
            .clone()
            .map(|a| a.chars().take(255).collect::<String>())
            .filter(|a| !a.is_empty());
        let initials = author.as_ref().map(|a| {
            a.split_whitespace()
                .filter_map(|w| w.chars().next())
                .take(4)
                .collect::<String>()
        });
        emitter.definitions.comments.insert(
            id,
            Comment {
                blocks: paragraphs,
                author,
                initials: initials.filter(|i| !i.is_empty()),
                date: comment.date.clone(),
                para_id: Some(para_id),
                parent_para_id,
                ..Comment::default()
            },
        );
    }

    // Definitions.
    let mut definitions = std::mem::take(&mut emitter.definitions);
    for (id, style) in std::mem::take(&mut emitter.styles) {
        definitions.styles.insert(id, style);
    }
    definitions.sections = sections;
    definitions.font_programs = state.font_programs.clone();
    let mut out_resources = BTreeMap::new();
    for program in &state.font_programs {
        if let Some(bytes) = resources.get(&program.part_name) {
            out_resources.insert(program.part_name.clone(), bytes.clone());
        }
    }
    for (_, media) in definitions.media.iter() {
        if let Some(bytes) = resources.get(&media.part_name) {
            out_resources.insert(media.part_name.clone(), bytes.clone());
        }
    }
    for (footer, _, id, part) in parts {
        if footer {
            definitions.footers.insert(id, part);
        } else {
            definitions.headers.insert(id, part);
        }
    }
    definitions.settings.even_and_odd_headers = flow.furniture.even_odd;
    let document_id = emitter.id()?;
    let document = Document::new(document_id, body, definitions)
        .map_err(|error| PdfImportError::Model(error.to_string()))?;
    Ok(Reflowed {
        document,
        resources: out_resources,
        report: std::mem::take(findings).finish(),
        work: *work,
    })
}

/// Appends `more` to `chars` with a space, or de-hyphenated. Returns the
/// joined characters and the hyphens removed.
fn glue(mut chars: Vec<PChar>, more: Vec<PChar>) -> (Vec<PChar>, u32) {
    while chars.last().is_some_and(|c| c.text.trim().is_empty()) {
        chars.pop();
    }
    let starts_lower = more
        .iter()
        .find(|c| !c.text.trim().is_empty())
        .and_then(|c| c.text.chars().next())
        .is_some_and(char::is_lowercase);
    let hyphen = chars
        .last()
        .is_some_and(|c| c.text == "-" || c.text == "\u{ad}");
    let mut removed = 0;
    if hyphen && starts_lower {
        chars.pop();
        removed = 1;
    } else if let Some(last) = chars.last() {
        let mut space = last.clone();
        space.text = " ".to_owned();
        space.sup = false;
        space.note = None;
        space.comments = Vec::new();
        chars.push(space);
    }
    chars.extend(more);
    (chars, removed)
}

/// A parsed marker: the marker, its characters (with the space after it),
/// the marker's left edge and the text's left edge.
type Found = Option<(Marker, usize, f64, f64)>;

/// Bullet and number lists: runs of consecutive marker paragraphs of one
/// kind, with a hanging indent (`docs/182` §4.2; `docs/131` §10.5: a visual
/// bullet alone does not make a list). Returns how many items became list
/// paragraphs. O(paragraphs).
/// Tagged lists (`L`/`LI`/`Lbl`/`LBody`, ISO 32000-1 §14.8.4.3.3): the
/// consecutive paragraphs under one outermost `L` are one list, each nested
/// `L` one level deeper. A level's format is its first item's marker (the
/// `Lbl` text, put back on its line by `attach_markers`), a bullet when it has
/// none. The tree, not the hanging indent, decides membership. O(paragraphs).
fn tagged_lists(items: &mut [Item], emitter: &mut Emitter<'_>) -> Result<u32, PdfImportError> {
    let mut count = 0;
    let mut i = 0;
    while i < items.len() {
        let root = match &items[i] {
            Item::Para(p) => p.list_root,
            _ => None,
        };
        let Some(root) = root else {
            i += 1;
            continue;
        };
        let mut j = i;
        while j < items.len() && matches!(&items[j], Item::Para(p) if p.list_root == Some(root)) {
            j += 1;
        }
        let mut levels: BTreeMap<u8, NumberingLevel> = BTreeMap::new();
        let mut placed: Vec<(usize, u8, usize)> = Vec::new();
        for (k, item) in items.iter().enumerate().take(j).skip(i) {
            let Item::Para(p) = item else { continue };
            let at = p.list_depth.saturating_sub(1).min(8);
            let parsed = text::marker(&p.chars, false);
            let marker_x0 = p.chars.first().map_or(p.first_x0, |c| c.x0);
            let (n, text_x0) = match &parsed {
                Some((_, n)) => (*n, p.chars.get(*n).map_or(marker_x0, |c| c.x0)),
                None => (0, p.first_x0),
            };
            levels.entry(at).or_insert_with(|| {
                let indent = (text_x0 - p.column_left).max(18.0 * f64::from(at + 1));
                let hanging = if parsed.is_some() {
                    (text_x0 - marker_x0).max(0.0)
                } else {
                    18.0
                };
                match parsed.map(|m| m.0) {
                    Some(Marker::Number {
                        value,
                        format,
                        pattern,
                    }) => level(
                        format,
                        pattern.replace("%1", &format!("%{}", at + 1)),
                        at,
                        u16::try_from(value).unwrap_or(1),
                        indent,
                        hanging,
                    ),
                    Some(Marker::Bullet(glyph)) => level(
                        NumberFormat::Bullet,
                        glyph.to_string(),
                        at,
                        1,
                        indent,
                        hanging,
                    ),
                    None => level(
                        NumberFormat::Bullet,
                        "\u{2022}".to_owned(),
                        at,
                        1,
                        indent,
                        hanging,
                    ),
                }
            });
            placed.push((k, at, n));
        }
        let deepest = levels.keys().copied().max().unwrap_or(0);
        for at in 0..=deepest {
            levels.entry(at).or_insert_with(|| {
                level(
                    NumberFormat::Bullet,
                    "\u{2022}".to_owned(),
                    at,
                    1,
                    36.0 * f64::from(at + 1),
                    18.0,
                )
            });
        }
        let abstract_id = AbstractNumberingId::new(emitter.id()?);
        let instance = NumberingInstanceId::new(emitter.id()?);
        emitter.definitions.abstract_numbering.insert(
            abstract_id,
            AbstractNumbering {
                levels: levels.into_values().collect(),
                multi_level_type: None,
                num_style_link: None,
                style_link: None,
            },
        );
        emitter.definitions.numbering.insert(
            instance,
            NumberingInstance {
                abstract_ref: abstract_id,
                overrides: Vec::new(),
            },
        );
        for (k, at, n) in placed {
            if let Item::Para(p) = &mut items[k] {
                p.list = Some((instance, at));
                p.marker_len = n;
                count += 1;
            }
        }
        i = j;
    }
    Ok(count)
}

fn lists(items: &mut [Item], emitter: &mut Emitter<'_>) -> Result<u32, PdfImportError> {
    let mut found: Vec<Found> = Vec::with_capacity(items.len());
    let mut previous_letter = false;
    for item in items.iter() {
        let parsed = match item {
            Item::Para(p) if p.list.is_some() => None,
            Item::Para(p) => text::marker(&p.chars, previous_letter).map(|(m, n)| {
                let marker_x0 = p.chars.first().map_or(0.0, |c| c.x0);
                let text_x0 = p.chars.get(n).map_or(marker_x0, |c| c.x0);
                (m, n, marker_x0, text_x0)
            }),
            _ => None,
        };
        previous_letter = matches!(
            parsed,
            Some((
                Marker::Number {
                    format: NumberFormat::LowerLetter | NumberFormat::UpperLetter,
                    ..
                },
                ..
            ))
        );
        found.push(parsed);
    }
    let mut count = 0;
    let mut i = 0;
    while i < items.len() {
        let Some((first, _, base_x0, _)) = found[i].clone() else {
            i += 1;
            continue;
        };
        // The run: consecutive marker paragraphs; level 0 is the first one's
        // marker position and kind, deeper markers start further right.
        let mut j = i + 1;
        while j < items.len() {
            match &found[j] {
                Some((m, _, x0, _))
                    if *x0 >= base_x0 - 2.0 && (*x0 > base_x0 + 4.0 || m.same_kind(&first)) =>
                {
                    j += 1;
                }
                _ => break,
            }
        }
        let run = i..j;
        // Hanging indent: a multi-line item whose continuation aligns with
        // its text, or consecutive items whose markers and text starts align.
        let multi_line_hanging = run.clone().any(|k| match (&items[k], &found[k]) {
            (Item::Para(p), Some((_, _, mx, tx))) => p
                .rest_x0
                .is_some_and(|r| (r - tx).abs() <= 2.0 && *tx > mx + 1.0),
            _ => false,
        });
        let aligned_items = run.len() >= 2
            && found[run.clone()]
                .iter()
                .flatten()
                .filter(|f| (f.2 - base_x0).abs() <= 2.0)
                .map(|f| f.3)
                .collect::<Vec<_>>()
                .windows(2)
                .all(|w| (w[0] - w[1]).abs() <= 3.0)
            && found[i].as_ref().is_some_and(|f| f.3 > f.2);
        let tagged = run.clone().any(|k| match &items[k] {
            Item::Para(p) => matches!(p.role.as_deref(), Some("LI" | "Lbl" | "LBody")),
            _ => false,
        });
        // Levels by marker position.
        let mut positions: Vec<f64> = found[run.clone()].iter().flatten().map(|f| f.2).collect();
        positions.sort_by(f64::total_cmp);
        positions.dedup_by(|a, b| (*a - *b).abs() <= 3.0);
        let level_of = |x: f64| -> u8 {
            positions
                .iter()
                .position(|p| (p - x).abs() <= 3.0)
                .unwrap_or(0)
                .min(8) as u8
        };
        // Numbers must run on at each level.
        let mut expect: BTreeMap<u8, u32> = BTreeMap::new();
        let mut consecutive = true;
        for k in run.clone() {
            if let Some((Marker::Number { value, .. }, _, x, _)) = &found[k] {
                let level = level_of(*x);
                if let Some(next) = expect.get(&level)
                    && next != value
                {
                    consecutive = false;
                }
                expect.insert(level, value + 1);
                expect.retain(|l, _| *l <= level);
            }
        }
        if !((multi_line_hanging || aligned_items || tagged) && consecutive) {
            i = j;
            continue;
        }
        // One abstract definition per run, a level per marker position.
        let mut levels: BTreeMap<u8, NumberingLevel> = BTreeMap::new();
        for k in run.clone() {
            let (Item::Para(p), Some((m, _, mx, tx))) = (&items[k], &found[k]) else {
                continue;
            };
            let at = level_of(*mx);
            levels.entry(at).or_insert_with(|| {
                let indent = tx - p.column_left;
                let hanging = tx - mx;
                match m {
                    Marker::Bullet(glyph) => level(
                        NumberFormat::Bullet,
                        glyph.to_string(),
                        at,
                        1,
                        indent,
                        hanging,
                    ),
                    Marker::Number {
                        value,
                        format,
                        pattern,
                    } => level(
                        format.clone(),
                        pattern.replace("%1", &format!("%{}", at + 1)),
                        at,
                        u16::try_from(*value).unwrap_or(1),
                        indent,
                        hanging,
                    ),
                }
            });
        }
        // Word needs a level for every index up to the deepest one used.
        let deepest = levels.keys().copied().max().unwrap_or(0);
        for at in 0..=deepest {
            levels.entry(at).or_insert_with(|| {
                level(
                    NumberFormat::Bullet,
                    "\u{2022}".to_owned(),
                    at,
                    1,
                    36.0 * f64::from(at + 1),
                    18.0,
                )
            });
        }
        let abstract_id = AbstractNumberingId::new(emitter.id()?);
        let instance = NumberingInstanceId::new(emitter.id()?);
        emitter.definitions.abstract_numbering.insert(
            abstract_id,
            AbstractNumbering {
                levels: levels.into_values().collect(),
                multi_level_type: None,
                num_style_link: None,
                style_link: None,
            },
        );
        emitter.definitions.numbering.insert(
            instance,
            NumberingInstance {
                abstract_ref: abstract_id,
                overrides: Vec::new(),
            },
        );
        for k in run {
            if let (Item::Para(p), Some((_, n, mx, _))) = (&mut items[k], &found[k]) {
                p.list = Some((instance, level_of(*mx)));
                p.marker_len = *n;
                count += 1;
            }
        }
        i = j;
    }
    Ok(count)
}

/// A header or footer part: one paragraph per baseline row, its pieces
/// separated by tabs at centre or right stops, the page number a PAGE field.
fn furniture_part(
    emitter: &mut Emitter<'_>,
    rows: &[Vec<ILine>],
    numbers: &[(usize, usize, usize, usize)],
    page_width: f64,
    margins: PageMargins,
) -> Result<Option<(HeaderFooterId, HeaderFooter)>, PdfImportError> {
    if rows.is_empty() {
        return Ok(None);
    }
    let left = f64::from(margins.start_twips) / 20.0;
    let right = page_width - f64::from(margins.end_twips) / 20.0;
    let width = (right - left).max(1.0);
    let body = emitter.sheet.body.clone();
    let mut blocks = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        let mut inlines = Vec::new();
        let mut tabs = Vec::new();
        let mut alignment = None;
        for (l, line) in row.iter().enumerate() {
            let centre = (line.x0 + line.x1) / 2.0;
            let place = if (centre - (left + right) / 2.0).abs() < 0.1 * width {
                TabAlignment::Center
            } else if line.x1 > right - 0.1 * width {
                TabAlignment::End
            } else {
                TabAlignment::Start
            };
            if l == 0 {
                if row.len() == 1 {
                    alignment = match place {
                        TabAlignment::Center => Some(Alignment::Center),
                        TabAlignment::End => Some(Alignment::End),
                        _ => None,
                    };
                }
            } else {
                let position = match place {
                    TabAlignment::Center => width / 2.0,
                    TabAlignment::End => width,
                    _ => line.x0 - left,
                };
                tabs.push(TabStop {
                    position_twips: twips(position),
                    alignment: place,
                    leader: None,
                });
                inlines.push(InlineNode::Tab(Tab { id: emitter.id()? }));
            }
            let number = numbers.iter().find(|n| n.0 == r && n.1 == l).copied();
            match number {
                Some((_, _, start, end)) => {
                    let before = &line.chars[..start];
                    let digits = &line.chars[start..end];
                    let after = &line.chars[end..];
                    inlines.extend(emitter.inlines(before, body.as_ref())?);
                    let cached = emitter.inlines(digits, body.as_ref())?;
                    inlines.push(InlineNode::Field(Box::new(Field {
                        id: emitter.id()?,
                        instruction: " PAGE ".to_owned(),
                        kind: FieldKind::Page,
                        inlines: cached,
                        form: None,
                        update: FieldUpdateState::default(),
                    })));
                    inlines.extend(emitter.inlines(after, body.as_ref())?);
                }
                None => inlines.extend(emitter.inlines(&line.chars, body.as_ref())?),
            }
        }
        blocks.push(BlockNode::Paragraph(Paragraph {
            id: emitter.id()?,
            properties: ParagraphProperties {
                alignment,
                tabs,
                spacing: Some(Spacing {
                    before_twips: Some(0),
                    after_twips: Some(0),
                    ..Spacing::default()
                }),
                ..ParagraphProperties::default()
            }
            .into(),
            inlines,
        }));
    }
    let id = HeaderFooterId::new(emitter.id()?);
    Ok(Some((id, HeaderFooter { blocks })))
}

/// Merges adjacent runs with equal properties (the model's normal form:
/// `AdjacentEquivalentTextRuns`), inside hyperlinks too, and drops empty
/// runs. O(inlines).
fn normalize(inlines: Vec<InlineNode>) -> Vec<InlineNode> {
    let mut out: Vec<InlineNode> = Vec::with_capacity(inlines.len());
    for inline in inlines {
        let inline = match inline {
            InlineNode::Hyperlink(mut h) => {
                h.inlines = normalize(std::mem::take(&mut h.inlines));
                InlineNode::Hyperlink(h)
            }
            other => other,
        };
        match (out.last_mut(), inline) {
            (Some(InlineNode::Run(prev)), InlineNode::Run(next))
                if prev.properties == next.properties =>
            {
                prev.text.push_str(&next.text);
            }
            (_, InlineNode::Run(run)) if run.text.is_empty() => {}
            (_, inline) => out.push(inline),
        }
    }
    out
}

/// The value a footnote marker stands for in its sequence: digits for
/// themselves, the `chicago` marks (`*` `†` `‡` `§`, then doubled) for their
/// position. `None` for anything else.
fn marker_value(marker: &str) -> Option<(bool, u32)> {
    if let Ok(v) = marker.parse::<u32>() {
        return Some((false, v));
    }
    let mut chars = marker.chars();
    let first = chars.next()?;
    let index = ['*', '\u{2020}', '\u{2021}', '\u{a7}']
        .iter()
        .position(|c| *c == first)?;
    let repeats = marker.chars().count();
    marker.chars().all(|c| c == first).then(|| {
        (
            true,
            u32::try_from(index + 1 + 4 * (repeats - 1)).unwrap_or(u32::MAX),
        )
    })
}

/// The footnote properties that print the PDF's markers: a number or
/// `chicago` format, starting where the PDF starts, continuous or restarting
/// on each page. `None` when the markers follow no such rule (they are then
/// renumbered and reported). O(notes).
fn note_numbering(notes: &[super::notes::INote]) -> Option<NoteProperties> {
    if notes.is_empty() {
        return Some(NoteProperties::default());
    }
    let values: Vec<(bool, u32, usize)> = notes
        .iter()
        .map(|n| marker_value(&n.marker).map(|(s, v)| (s, v, n.page)))
        .collect::<Option<_>>()?;
    let symbols = values[0].0;
    if values.iter().any(|v| v.0 != symbols) {
        return None;
    }
    let continuous = values.windows(2).all(|w| w[1].1 == w[0].1 + 1);
    let each_page = values.windows(2).all(|w| {
        if w[1].2 == w[0].2 {
            w[1].1 == w[0].1 + 1
        } else {
            w[1].1 == values[0].1
        }
    });
    let restart = if continuous {
        None
    } else if each_page {
        Some(NoteNumberRestart::EachPage)
    } else {
        return None;
    };
    Some(NoteProperties {
        position: None,
        number_format: symbols.then(|| NumberFormat::Other("chicago".to_owned())),
        number_start: (values[0].1 != 1).then(|| i32::try_from(values[0].1).unwrap_or(1)),
        number_restart: restart,
    })
}

/// Gives every run in a note paragraph without a size the note's own size.
fn set_size(mut paragraph: Paragraph, key: Option<&FontKey>) -> Paragraph {
    let Some(key) = key else {
        return paragraph;
    };
    for inline in &mut paragraph.inlines {
        if let InlineNode::Run(run) = inline
            && run.properties.size_half_points.is_none()
        {
            let mut properties = (*run.properties).clone();
            properties.size_half_points = Some(key.half_points);
            properties.size_complex_half_points = Some(key.half_points);
            run.properties = properties.into();
        }
    }
    paragraph.inlines = normalize(std::mem::take(&mut paragraph.inlines));
    paragraph
}

fn collect_inline_refs(inlines: &[InlineNode], out: &mut BTreeSet<CommentId>) {
    for inline in inlines {
        match inline {
            InlineNode::CommentReference(r) => {
                out.insert(r.comment);
            }
            InlineNode::Hyperlink(h) => collect_inline_refs(&h.inlines, out),
            _ => {}
        }
    }
}

fn collect_block_refs(block: &BlockNode, out: &mut BTreeSet<CommentId>) {
    match block {
        BlockNode::Paragraph(p) => collect_inline_refs(&p.inlines, out),
        BlockNode::Table(t) => {
            for row in &t.rows {
                for cell in &row.cells {
                    for b in &cell.blocks {
                        collect_block_refs(b, out);
                    }
                }
            }
        }
        _ => {}
    }
}

/// The comments the body references. O(body).
fn collect_comment_refs(body: &[BlockNode]) -> BTreeSet<CommentId> {
    let mut out = BTreeSet::new();
    for block in body {
        collect_block_refs(block, &mut out);
    }
    out
}
