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
// Own line: a drawing's labels as text boxes in its group.
use casual_doc_model::v1::{
    GroupChild, GroupTextBox, PointEmu, ShapeGeometry, TextBoxBodyProperties, TextBoxInsets,
};
// Own line: a figure's pictures in its group.
use casual_doc_model::v1::GroupPicture;
// Own line: paragraph borders from the PDF's rules.
use casual_doc_model::v1::ParagraphBorders;
// Own line: ruled rows keep the PDF's height.
use casual_doc_model::v1::{CellMargins, HeightRule, RowHeight};
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

/// A running header table's page numbering: the kept instance's page
/// (one-based), the page count, and its digit runs that vary from page to
/// page, `(row, cell, run)`.
type PageNumbers<'a> = (usize, usize, &'a BTreeSet<(usize, usize, usize)>);

fn twips(points: f64) -> i32 {
    (points * 20.0).round() as i32
}

/// An image-text line box's padding around its line's ink, as a share of
/// the line's height (`Emitter::image_text`): the rim and the descenders a
/// recogniser's box leaves out, and no more, so a box does not reach the
/// line above or below.
const LINE_BOX_PAD: f64 = 0.12;

/// Marks every run in `inlines` as laid out and not painted
/// (`RunProperties::no_text_fill`). O(inlines).
/// `cap` (half-points), when given, is the largest size a run may keep, so
/// the unpainted line does not wrap inside its box. O(inlines).
fn unpainted(inlines: &mut [InlineNode], cap: Option<u32>) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => {
                let mut properties = (*run.properties).clone();
                properties.no_text_fill = Some(true);
                if let Some(cap) = cap {
                    let size = properties.size_half_points.unwrap_or(22).min(cap);
                    properties.size_half_points = Some(size.max(2));
                }
                run.properties = properties.into();
            }
            InlineNode::Hyperlink(link) => unpainted(&mut link.inlines, cap),
            _ => {}
        }
    }
}

/// A substitute face's mean advance, in ems, generously: what the size of an
/// unpainted line is capped by so it stays on one line in its box (the caret
/// walks it as the line it is; the size is set from the ink when it is
/// edited).
const MEAN_ADVANCE_EM: f64 = 0.6;

/// Word's default distance between a square-wrapped float and the text
/// beside it (`distL`/`distR`, 0.125 in).
const FLOAT_SIDE_GAP: f64 = 9.0;

/// A float's anchor: `dx` from its column's left, `dy` from its paragraph's
/// top. `None` is vector art on its own line (`wp:wrapTopAndBottom`). O(1).
fn float_anchor(dx: f64, dy: f64, wrap: Option<FloatWrap>) -> DrawingAnchor {
    let emu = crate::fixed::emu;
    // A background is placed on the page itself (`dx`, `dy` from its
    // corner), whatever paragraph carries it.
    let page = wrap == Some(FloatWrap::Background);
    DrawingAnchor {
        horizontal: AnchorHorizontal {
            relative_from: if page {
                HorizontalAnchor::Page
            } else {
                HorizontalAnchor::Column
            },
            position: HorizontalPosition::Offset(emu(dx)),
        },
        vertical: AnchorVertical {
            relative_from: if page {
                VerticalAnchor::Page
            } else {
                VerticalAnchor::Paragraph
            },
            position: VerticalPosition::Offset(emu(dy)),
        },
        wrap: match wrap {
            Some(FloatWrap::Beside) => WrapMode::Square,
            Some(FloatWrap::Behind | FloatWrap::Background) => WrapMode::None,
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
        behind_doc: matches!(wrap, Some(FloatWrap::Behind | FloatWrap::Background)),
        allow_overlap: true,
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
    /// The first line's baseline (the space above it is measured from it).
    first_baseline: f64,
    /// The median distance from one baseline to the next inside it, when it
    /// has two lines on one page.
    pitch: Option<f64>,
    /// Every baseline-to-baseline step inside it, its joined pieces'
    /// included (a producer that writes each line as its own text object
    /// leaves every paragraph in one-line pieces).
    steps: Vec<f64>,
    /// The space between a table, picture or drawing above it on its page
    /// and its first line.
    gap_above: Option<f64>,
    /// The space between its last line and the paragraph under it in the
    /// same column (`rhythm`).
    gap_below: Option<f64>,
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
    /// Its shading: every line of it sits on one fill (`decorate`).
    shade: Option<[u8; 3]>,
}

// Transient (one per paragraph, dropped at emission), so the size of the
// paragraph variant costs nothing worth a box per paragraph.
#[allow(clippy::large_enum_variant)]
enum Item {
    Para(IPara),
    Table(ITable, Option<usize>),
    /// A picture; with lines, the lines OCR read from it, drawn over it as
    /// line boxes (`ScanText::LineBoxes`).
    Image(SourceImage, Option<usize>, Vec<ILine>),
    /// Vector art on its own line, with the text drawn inside it and the
    /// pictures that are its pieces.
    Vector(SourceVector, Option<usize>, Vec<ILine>, Vec<SourceImage>),
    /// Floats whose text produced no paragraph of its own (a tagged table's
    /// cells): carried by an empty paragraph.
    Floats(Vec<Float>, Option<usize>),
}

impl Item {
    /// The page it was on, when it says (a table by its cells' text;
    /// pictures and drawings follow what is around them).
    fn page(&self) -> Option<usize> {
        match self {
            Self::Para(p) => Some(p.page),
            Self::Table(t, _) => t
                .rows
                .iter()
                .flatten()
                .flat_map(|c| c.lines.first())
                .map(|l| l.page)
                .next(),
            // A picture with line boxes is its page's (a scan): it decides
            // that page's size.
            Self::Image(_, _, lines) => lines.first().map(|l| l.page),
            Self::Vector(..) | Self::Floats(..) => None,
        }
    }

    fn region(&self) -> Option<usize> {
        match self {
            Self::Para(p) => p.region,
            Self::Table(_, r)
            | Self::Image(_, r, _)
            | Self::Vector(_, r, ..)
            | Self::Floats(_, r) => *r,
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
    /// The Title style (heading level 0) and its key, when used.
    title_style: Option<StyleId>,
    title_key: Option<FontKey>,
    normal: Option<StyleId>,
    definitions: Definitions,
    links: Vec<super::ILink>,
    bookmarks: BTreeMap<usize, (BookmarkId, String)>,
    comments: Vec<Option<(CommentId, super::IComment)>>,
    notes: Vec<Option<NoteId>>,
    /// The last float's stacking key (paint order).
    z: u32,
    /// The measured vertical rhythm (line pitch, the space between
    /// paragraphs).
    rhythm: Rhythm,
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
        labels: &[ILine],
        pictures: &[SourceImage],
    ) -> Result<Option<InlineNode>, PdfImportError> {
        let paths: Vec<&casual_doc_pdf_read::PathEvidence> = vector.paths.iter().collect();
        let mut built = crate::fixed::vector_children(&paths, &mut || self.id())?;
        self.vector_clipped += built.clipped;
        self.vector_split += built.split;
        // The figure's pictures, over its shapes (a screenshot over the card
        // it sits on) and under its labels, where the PDF drew them.
        let mut children = Vec::with_capacity(pictures.len() + built.children.len());
        let mut extent: Option<(f64, f64, f64, f64)> = built.bounds;
        for picture in pictures {
            let b = picture.bounds;
            let media = self.media(picture)?;
            children.push(GroupChild::Picture(GroupPicture {
                id: self.id()?,
                media,
                offset: PointEmu {
                    x_emu: crate::fixed::emu(b.x0),
                    y_emu: crate::fixed::emu(b.y0),
                },
                extent: Extent {
                    width_emu: crate::fixed::emu(b.width()).max(1),
                    height_emu: crate::fixed::emu(b.height()).max(1),
                },
                descr: None,
                crop: None,
                opacity: None,
                hyperlink: None,
                border: None,
                flip_h: false,
                flip_v: false,
                rotation: None,
            }));
            extent = Some(match extent {
                Some(e) => (e.0.min(b.x0), e.1.min(b.y0), e.2.max(b.x1), e.3.max(b.y1)),
                None => (b.x0, b.y0, b.x1, b.y1),
            });
        }
        built.children.append(&mut children);
        let Some(mut bounds) = extent else {
            return Ok(None);
        };
        // The text drawn inside it, each line a text box where the PDF set
        // it, painted over the shapes (a chart's labels and legend).
        for line in labels {
            let (chars, _) = text::join(&[line]);
            let inlines = self.inlines(&chars, None)?;
            // Wider than the line by a third: a substitute face sets it
            // wider, and a label must not wrap.
            let width = (line.x1 - line.x0) * 4.0 / 3.0 + 2.0;
            let height = (line.bottom - line.top).max(1.0) + 1.0;
            bounds = (
                bounds.0.min(line.x0),
                bounds.1.min(line.top),
                bounds.2.max(line.x0 + width),
                bounds.3.max(line.top + height),
            );
            built.children.push(GroupChild::TextBox(GroupTextBox {
                id: self.id()?,
                offset: PointEmu {
                    x_emu: crate::fixed::emu(line.x0),
                    y_emu: crate::fixed::emu(line.top),
                },
                extent: Extent {
                    width_emu: crate::fixed::emu(width).max(1),
                    height_emu: crate::fixed::emu(height).max(1),
                },
                geometry: ShapeGeometry::Rectangle,
                preset: None,
                adjustments: Vec::new(),
                blocks: vec![BlockNode::Paragraph(Paragraph {
                    id: self.id()?,
                    properties: ParagraphProperties {
                        spacing: Some(Spacing {
                            before_twips: Some(0),
                            after_twips: Some(0),
                            line_percent: Some(100),
                            ..Spacing::default()
                        }),
                        ..ParagraphProperties::default()
                    }
                    .into(),
                    inlines,
                })],
                fill: None,
                border: None,
                shadow: None,
                body_properties: TextBoxBodyProperties {
                    insets: TextBoxInsets {
                        left_emu: 0,
                        top_emu: 0,
                        right_emu: 0,
                        bottom_emu: 0,
                    },
                    ..TextBoxBodyProperties::default()
                },
                hyperlink: None,
                flip_h: false,
                flip_v: false,
                rotation: None,
            }));
        }
        self.z += 1;
        let id = self.id()?;
        Ok(Some(InlineNode::Group(Box::new(
            crate::fixed::vector_group(id, built.children, bounds, anchor, self.z),
        ))))
    }

    /// A picture with the lines OCR read from it (`ScanText::LineBoxes`,
    /// `docs/182` §20b): one group of the picture and, over it, one text box
    /// per line where the line's ink is, turned by the line's skew, its text
    /// laid out but not painted (`no_text_fill`) so the page shows the
    /// picture. Each box is padded by [`LINE_BOX_PAD`] of its height on every
    /// side, so the anti-aliased rim and the descenders are inside it: the box
    /// is the region an edit erases. The group is `size` on the page; its
    /// child space is the page's points. O(characters of the lines).
    fn image_text(
        &mut self,
        image: &SourceImage,
        lines: &[ILine],
        skew: &dyn Fn(&ILine) -> f64,
        anchor: DrawingAnchor,
        size: (f64, f64),
    ) -> Result<InlineNode, PdfImportError> {
        let emu = crate::fixed::emu;
        let b = image.bounds;
        let media = self.media(image)?;
        let mut children = Vec::with_capacity(lines.len() + 1);
        children.push(GroupChild::Picture(GroupPicture {
            id: self.id()?,
            media,
            offset: PointEmu {
                x_emu: emu(b.x0),
                y_emu: emu(b.y0),
            },
            extent: Extent {
                width_emu: emu(b.width()).max(1),
                height_emu: emu(b.height()).max(1),
            },
            descr: None,
            crop: None,
            opacity: None,
            hyperlink: None,
            border: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
        }));
        for line in lines {
            let (chars, _) = text::join(&[line]);
            let mut inlines = self.inlines(&chars, None)?;
            // At most the size whose mean advance fits the line's width.
            let count = chars
                .iter()
                .map(|c| c.text.chars().count())
                .sum::<usize>()
                .max(1);
            let cap = ((line.x1 - line.x0) / (count as f64 * MEAN_ADVANCE_EM) * 2.0).floor();
            unpainted(&mut inlines, (cap >= 2.0).then_some(cap as u32));
            let height = (line.bottom - line.top).max(1.0);
            let pad = (height * LINE_BOX_PAD).max(0.5);
            let (x0, y0, x1, y1) = (
                line.x0 - pad,
                line.top - pad,
                line.x1 + pad,
                line.bottom + pad,
            );
            let (w, h) = (x1 - x0, y1 - y0);
            // The line was laid level about its first glyph's origin; its
            // ink runs from there at its angle. The box turns about its own
            // centre (`a:xfrm@rot`), so its centre is the turned one.
            let degrees = skew(line);
            let (cx, cy) = {
                let (px, py) = (line.x0, line.baseline);
                let (dx, dy) = ((x0 + x1) / 2.0 - px, (y0 + y1) / 2.0 - py);
                let (sin, cos) = degrees.to_radians().sin_cos();
                (px + dx * cos - dy * sin, py + dx * sin + dy * cos)
            };
            children.push(GroupChild::TextBox(GroupTextBox {
                id: self.id()?,
                offset: PointEmu {
                    x_emu: emu(cx - w / 2.0),
                    y_emu: emu(cy - h / 2.0),
                },
                extent: Extent {
                    width_emu: emu(w).max(1),
                    height_emu: emu(h).max(1),
                },
                geometry: ShapeGeometry::Rectangle,
                preset: None,
                adjustments: Vec::new(),
                blocks: vec![BlockNode::Paragraph(Paragraph {
                    id: self.id()?,
                    properties: ParagraphProperties {
                        spacing: Some(Spacing {
                            before_twips: Some(0),
                            after_twips: Some(0),
                            line_percent: Some(100),
                            ..Spacing::default()
                        }),
                        ..ParagraphProperties::default()
                    }
                    .into(),
                    inlines,
                })],
                fill: None,
                border: None,
                shadow: None,
                body_properties: TextBoxBodyProperties {
                    insets: TextBoxInsets {
                        left_emu: i32::try_from(emu(pad)).unwrap_or(0),
                        top_emu: 0,
                        right_emu: 0,
                        bottom_emu: 0,
                    },
                    ..TextBoxBodyProperties::default()
                },
                hyperlink: None,
                flip_h: false,
                flip_v: false,
                rotation: (degrees.abs() > 0.0).then(|| (degrees * 60_000.0).round() as i32),
            }));
        }
        self.z += 1;
        let id = self.id()?;
        let mut group =
            crate::fixed::vector_group(id, children, (b.x0, b.y0, b.x1, b.y1), anchor, self.z);
        group.extent = Extent {
            width_emu: emu(size.0).max(1),
            height_emu: emu(size.1).max(1),
        };
        group.transform.extent = group.extent;
        Ok(InlineNode::Group(Box::new(group)))
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
            let (dx, dy) = if float.wrap == FloatWrap::Background {
                (b.x0, b.y0)
            } else if float.page == page {
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
                    if let Some(group) = self.group(vector, anchor, &[], &[])? {
                        out.push(group);
                    }
                }
            }
        }
        Ok(out)
    }

    /// A body paragraph's own spacing where it is not Normal's: set in
    /// another size (a title or subtitle the sheet did not make a style, a
    /// caption, a note), its lines are single, or its own measured pitch;
    /// and the space under it, where the PDF left clearly more or less than
    /// Normal's (a block set apart, an address packed tight). Small
    /// differences are Normal's: a position rounding is not a choice. O(1).
    fn own_spacing(&self, para: &IPara) -> Option<Spacing> {
        let r = &self.rhythm;
        let size = size_of(para)?;
        let mut spacing = Spacing::default();
        let mut set = false;
        // Its own leading: its lines' pitch, else the pitch the document
        // sets its size at; where that is not what Normal's spacing gives it,
        // its own percentage (a 10 pt block in an 11 pt document set at the
        // same 14.5 pt is 126%, not Normal's single).
        let inherited = f64::from(r.line_percent.unwrap_or(100)) / 100.0;
        let half = (size * 2.0).round() as u32;
        let own = para
            .pitch
            .map(|p| p / size)
            .or_else(|| r.ratio_by_size.get(&half).copied())
            .filter(|ratio| (0.8..=3.5).contains(ratio))
            .map(|ratio| ratio / SINGLE_LINE_EMS);
        let wanted = match own {
            Some(f) => Some(f),
            // Not measured: a size other than the body's sets single lines.
            None if (size - r.body_size).abs() > 0.5 => Some(1.0),
            None => None,
        };
        if let Some(f) = wanted
            && (f - inherited).abs() > SINGLE_TOLERANCE
        {
            spacing.line_percent = Some((f * 100.0).round().clamp(80.0, 300.0) as u16);
            set = true;
        }
        let normal = r.body_after.unwrap_or(r.body_size * 0.6);
        if let Some(gap) = para.gap_below
            && (gap - normal).abs() > (0.25 * r.body_size).max(3.0)
        {
            spacing.after_twips = Some(twips(gap));
            set = true;
        }
        set.then_some(spacing)
    }

    fn style_key(&self, para: &IPara) -> Option<FontKey> {
        match para.heading {
            Some(0) => self.title_key.clone(),
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
        if c.underline {
            p.underline = Some(true);
        }
        if c.strike {
            p.strike = Some(true);
        }
        if let Some([r, g, b]) = c.shade {
            p.shading.fill = Some(RgbColor { r, g, b });
        }
        if c.tracking.abs() >= 0.1 {
            p.character_spacing_twips = Some(twips(c.tracking));
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
                Some(0) => self.title_style,
                Some(level) => self.heading_styles.get(usize::from(level) - 1).copied(),
                None => None,
            },
            ..ParagraphProperties::default()
        };
        if let Some((instance, level)) = para.list {
            properties.numbering = Some(NumberingRef { instance, level });
            // The space the PDF left between items, where it differs from
            // Normal's (a list set apart item by item, or packed tight).
            if let Some(after) = self.rhythm.list_after
                && (after - self.rhythm.body_after.unwrap_or(after)).abs() > 1.0
            {
                properties.spacing = Some(Spacing {
                    after_twips: Some(twips(after)),
                    ..Spacing::default()
                });
            }
        } else if para.heading.is_none() {
            let start = para.rest_x0.unwrap_or(para.first_x0) - para.column_left;
            let first = para.rest_x0.map(|rest| para.first_x0 - rest);
            if start > 6.0 || first.is_some_and(|f| f.abs() > 1.0) {
                properties.indentation = Some(Indentation {
                    start_twips: (start > 6.0).then(|| indent_twips(start)),
                    end_twips: None,
                    first_line_twips: first.filter(|f| *f > 1.0).map(indent_twips),
                    hanging_twips: first.filter(|f| *f < -1.0).map(|f| indent_twips(-f)),
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
        } else if para.centred {
            // A centred heading or title stays centred (its style is the
            // document's, its alignment is the paragraph's own).
            properties.alignment = Some(Alignment::Center);
        } else if para.end_aligned {
            properties.alignment = Some(Alignment::End);
        }
        if para.page_break {
            properties.page_break_before = Some(true);
        }
        // Fills and rules the PDF drew for the paragraph (`decorate`).
        if let Some([r, g, b]) = para.shade {
            properties.shading.fill = Some(RgbColor { r, g, b });
        }
        if let Some((width, color)) = para.last_line.as_ref().and_then(|l| l.border_below) {
            properties.borders = ParagraphBorders {
                bottom: Some(BorderEdge {
                    style: "single".to_owned(),
                    size_eighth_points: Some(((width * 8.0).round() as u32).clamp(2, 96)),
                    color: color.map(|[r, g, b]| RgbColor { r, g, b }),
                    theme_color: None,
                    space_points: Some(1),
                }),
                ..ParagraphBorders::default()
            }
            .into();
        }
        if para.heading.is_none() && para.list.is_none() {
            properties.spacing = self.own_spacing(para);
        }
        // Below a table, a picture or a drawing, the space the PDF left
        // (nothing in Word's model puts space after a table: the paragraph
        // under it carries it, as Word's own users do).
        if let Some(gap) = para.gap_above.filter(|g| *g > 1.0) {
            properties.spacing = Some(Spacing {
                before_twips: Some(twips(gap)),
                ..properties.spacing.unwrap_or_default()
            });
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
        self.numbered_table(table, None)
    }

    /// The runs of `chars` that are page numbers: the digit runs at the
    /// `varying` indices equal to `page` are `PAGE`, and a run after one of
    /// them equal to `total` is `NUMPAGES` ("Page 2 of 6"). O(characters).
    fn page_runs(
        chars: &[PChar],
        varying: &[usize],
        page: usize,
        total: usize,
    ) -> Vec<(usize, usize, FieldKind)> {
        let digit = |c: &PChar| !c.text.is_empty() && c.text.chars().all(|d| d.is_ascii_digit());
        let mut runs = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            if digit(&chars[i]) {
                let mut j = i;
                while j < chars.len() && digit(&chars[j]) {
                    j += 1;
                }
                runs.push((i, j));
                i = j;
            } else {
                i += 1;
            }
        }
        let value = |(a, b): (usize, usize)| -> String {
            chars[a..b].iter().map(|c| c.text.as_str()).collect()
        };
        let mut out = Vec::new();
        let mut after_page = false;
        for (k, &run) in runs.iter().enumerate() {
            if varying.contains(&k) && value(run) == page.to_string() {
                out.push((run.0, run.1, FieldKind::Page));
                after_page = true;
            } else if after_page && value(run) == total.to_string() {
                out.push((run.0, run.1, FieldKind::NumPages));
                after_page = false;
            }
        }
        out
    }

    /// Inline content where a digit run equal to `page` (one-based) is a
    /// `PAGE` field and one equal to `total` a `NUMPAGES` field (a running
    /// header's "Page 2 of 6"); without numbers, [`Self::inlines`].
    /// O(characters).
    fn numbered_inlines(
        &mut self,
        chars: &[PChar],
        style: Option<&FontKey>,
        numbers: Option<(usize, usize, &[usize])>,
    ) -> Result<Vec<InlineNode>, PdfImportError> {
        let Some((page, total, varying)) = numbers else {
            return self.inlines(chars, style);
        };
        let mut out = Vec::new();
        let mut from = 0;
        for (i, j, kind) in Self::page_runs(chars, varying, page, total) {
            out.extend(self.inlines(&chars[from..i], style)?);
            let cached = self.inlines(&chars[i..j], style)?;
            let instruction = if kind == FieldKind::Page {
                " PAGE "
            } else {
                " NUMPAGES "
            };
            out.push(InlineNode::Field(Box::new(Field {
                id: self.id()?,
                instruction: instruction.to_owned(),
                kind,
                inlines: cached,
                form: None,
                update: FieldUpdateState::default(),
            })));
            from = j;
        }
        out.extend(self.inlines(&chars[from..], style)?);
        Ok(out)
    }

    /// [`Self::table`], with a running header's page numbers as fields.
    fn numbered_table(
        &mut self,
        table: &ITable,
        numbers: Option<PageNumbers<'_>>,
    ) -> Result<Table, PdfImportError> {
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
            for (c, cell) in row.iter().enumerate() {
                let mut blocks = Vec::new();
                if cell.vmerge != Some(false) {
                    let lines: Vec<&ILine> = cell.lines.iter().collect();
                    for chunk in
                        lines.chunk_by(|a, b| b.top - a.bottom < 0.6 * (a.bottom - a.top).max(1.0))
                    {
                        let (chars, _) = text::join(chunk);
                        let id = self.id()?;
                        // The cell's varying digit runs, by their index in
                        // the cell's whole text.
                        let varying: Vec<usize> = numbers
                            .map(|(_, _, v)| {
                                v.iter()
                                    .filter(|(vr, vc, _)| *vr == r && *vc == c)
                                    .map(|(_, _, k)| *k)
                                    .collect()
                            })
                            .unwrap_or_default();
                        let inlines = self.numbered_inlines(
                            &chars,
                            body.as_ref(),
                            numbers.map(|(p, t, _)| (p, t, varying.as_slice())),
                        )?;
                        let mut properties = cell_paragraph();
                        if centred_in(chunk, cell.x0, cell.x1) {
                            let mut centred = (*properties).clone();
                            centred.alignment = Some(Alignment::Center);
                            properties = centred.into();
                        }
                        blocks.push(BlockNode::Paragraph(Paragraph {
                            id,
                            properties,
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
                            value: cell_width(cell.x1 - cell.x0),
                            width_type: WidthType::Dxa,
                        }),
                        shading: Shading {
                            fill: cell.shading.map(|[r, g, b]| RgbColor { r, g, b }),
                            ..Shading::default()
                        },
                        vertical_alignment: cell.valign,
                        ..TableCellProperties::default()
                    },
                    blocks,
                });
            }
            rows.push(TableRow {
                id: self.id()?,
                properties: TableRowProperties {
                    header: r < table.header_rows,
                    height: if table.ruled {
                        row_height(row)
                    } else {
                        pitch_height(&table.rows, r)
                    },
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
                    // Inside the model's domain: a whitespace table's
                    // measured columns can be negative or page-wide.
                    width_twips: Some(grid_width(*w)),
                })
                .collect(),
            grid_change: None,
            properties: TableProperties {
                borders,
                // A borderless table's text starts where the PDF's did: the
                // columns were measured from the text, so Word's default
                // 0.08 in either side of each cell would wrap a column whose
                // text filled it.
                cell_margins: if table.ruled {
                    CellMargins::default()
                } else {
                    CellMargins {
                        start_twips: Some(twips(table.pad)),
                        end_twips: Some(0),
                        ..CellMargins::default()
                    }
                },
                ..TableProperties::default()
            },
            rows,
        })
    }
}

/// A ruled row's height as the PDF drew it, as a minimum (`atLeast`, so
/// text the substitute face sets wider can still grow it): the shortest of
/// its cells that start in it and span no other row. A row reflowed at its
/// text's height was a third shorter than the PDF's (LibreOffice and Word
/// pad cells) and the table read as cramped. O(cells).
fn row_height(row: &[super::tables::ICell]) -> RowHeight {
    let height = row
        .iter()
        .filter(|c| c.vmerge.is_none())
        .map(|c| c.y1 - c.y0)
        .filter(|h| *h > 0.0)
        .fold(f64::INFINITY, f64::min);
    if height.is_finite() {
        RowHeight {
            value_twips: Some(u32::try_from(twips(height).clamp(0, 31_680)).unwrap_or(0)),
            rule: Some(HeightRule::AtLeast),
        }
    } else {
        RowHeight::default()
    }
}

/// A borderless row's height as the PDF set it, as a minimum: from its
/// first baseline to the next row's (the last row takes the one before's).
/// Its text's height alone packed the rows of every borderless table
/// together. O(cells of two rows).
fn pitch_height(rows: &[Vec<super::tables::ICell>], r: usize) -> RowHeight {
    let baseline = |r: usize| -> Option<f64> {
        rows.get(r)?
            .iter()
            .filter_map(|c| c.lines.first().map(|l| l.baseline))
            .reduce(f64::min)
    };
    let step = |a: usize, b: usize| -> Option<f64> {
        let (x, y) = (baseline(a)?, baseline(b)?);
        (y > x).then_some(y - x)
    };
    let height = step(r, r + 1).or_else(|| r.checked_sub(1).and_then(|p| step(p, r)));
    // A step of more than three of its lines (each 1.5 of its ems) is a
    // gap the table does not own.
    let em = rows[r]
        .iter()
        .flat_map(|c| c.lines.iter())
        .map(|l| l.size)
        .fold(1.0, f64::max);
    // A tall row (cells at its top and bottom) is as many lines as its
    // text spans, not as its cells hold.
    let spanned = {
        let all = rows[r].iter().flat_map(|c| c.lines.iter());
        let top = all.clone().map(|l| l.top).fold(f64::INFINITY, f64::min);
        let bottom = all.map(|l| l.bottom).fold(f64::NEG_INFINITY, f64::max);
        if bottom > top {
            ((bottom - top) / (1.2 * em)).ceil() as usize
        } else {
            1
        }
    };
    let lines = rows[r]
        .iter()
        .map(|c| c.lines.len().max(1))
        .max()
        .unwrap_or(1)
        .max(spanned);
    match height {
        Some(h) if h <= 3.0 * 1.5 * em * lines as f64 => RowHeight {
            value_twips: Some(u32::try_from(twips(h).clamp(0, 31_680)).unwrap_or(0)),
            rule: Some(HeightRule::AtLeast),
        },
        _ => RowHeight::default(),
    }
}

/// A cell paragraph's properties: no space after it (the row's height, or
/// the cell's padding, is the space; Normal's measured space between body
/// paragraphs inside every cell made each row a line taller). O(1).
/// Whether every line of a cell paragraph is centred between the cell's
/// edges `x0`..`x1` and stands clear of both by at least an em: a merged
/// header centred across the columns it spans, a status centred in its
/// column. Text flush with an edge is left (or end) aligned, whatever its
/// centre. O(lines).
pub(super) fn centred_in(lines: &[&ILine], x0: f64, x1: f64) -> bool {
    let middle = (x0 + x1) / 2.0;
    let tolerance = ((x1 - x0) * 0.04).max(2.0);
    !lines.is_empty()
        && lines.iter().all(|line| {
            let em = line.size.max(1.0);
            ((line.x0 + line.x1) / 2.0 - middle).abs() <= tolerance
                && line.x0 - x0 >= em
                && x1 - line.x1 >= em
        })
}

fn cell_paragraph() -> casual_doc_model::v1::SharedParagraphProperties {
    ParagraphProperties {
        spacing: Some(Spacing {
            after_twips: Some(0),
            ..Spacing::default()
        }),
        ..ParagraphProperties::default()
    }
    .into()
}

/// A table grid column's width in twips, inside the model's domain: a
/// whitespace table's measured column can come out negative (text that
/// overhangs the next column) or page-wide, and one such table made a whole
/// flowing document fail validation (found by the corpus sweep).
pub(super) fn grid_width(points: f64) -> i32 {
    twips(points).clamp(1, 31_680)
}

/// A table cell's preferred width in twips, inside the model's domain.
pub(super) fn cell_width(points: f64) -> i32 {
    twips(points).clamp(0, 31_680)
}

/// An indent in twips, inside the model's domain (22 inches either way).
/// A PDF can place text anywhere on a page larger than that (a poster, a
/// figure page drawn at a large scale), and one such paragraph made the
/// whole flowing document fail validation (`arxiv-2210.11416`, found by the
/// corpus sweep, `docs/182` §18.10).
pub(super) fn indent_twips(points: f64) -> i32 {
    twips(points).clamp(-31_680, 31_680)
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
                start_twips: Some(indent_twips(indent).max(0)),
                end_twips: None,
                first_line_twips: None,
                hanging_twips: Some(indent_twips(hanging).max(0)),
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
fn column(
    flow: &Flow,
    region: Option<usize>,
    page: Option<usize>,
    x: f64,
    page_width: f64,
) -> (f64, f64) {
    if region.is_none()
        && let Some(&(left, right)) = page.and_then(|p| flow.page_body.get(p))
        && left.is_finite()
        && right > left
    {
        return (left, right);
    }
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
    // Where the ink starts: a line written with leading spaces (code, a
    // typed indent) starts at its first visible character.
    let ink = |l: &ILine| {
        l.chars
            .iter()
            .find(|c| !c.text.trim().is_empty())
            .map_or(l.x0, |c| c.x0)
    };
    let first_x0 = lines.first().map_or(0.0, |l| ink(l));
    let rest_x0 = (lines.len() > 1).then(|| {
        lines[1..]
            .iter()
            .map(|l| l.x0)
            .fold(f64::INFINITY, f64::min)
    });
    let right = lines.iter().map(|l| l.x1).fold(f64::NEG_INFINITY, f64::max);
    let page = lines.first().map(|l| l.page);
    let (column_left, column_right) = column(flow, region, page, first_x0, page_width);
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
/// Where `line` had to stop: the left edge of a graphic floated beside the
/// text (`FloatWrap::Beside`) that shares its band and stands to its right,
/// else the column's right edge. Text wrapped around a picture runs to the
/// picture, not the column, so the paragraph it began goes on below the
/// picture instead of ending at its edge. O(floats).
fn wrap_right(line: &ILine, column_right: f64, floats: &[Float]) -> f64 {
    floats
        .iter()
        .filter(|f| f.page == line.page && matches!(f.wrap, FloatWrap::Beside))
        .map(|f| f.graphic.bounds())
        .filter(|b| {
            let shared = line.bottom.min(b.y1) - line.top.max(b.y0);
            shared > 0.5 * (line.bottom - line.top) && b.x0 >= line.x1 - 1.0 && b.x0 < column_right
        })
        .map(|b| b.x0)
        .fold(column_right, f64::min)
}

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
    // The bottom of the last table, picture or drawing placed, and its page.
    let mut object_bottom: Option<(usize, f64)> = None;
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
                        // The rest of the same visual line: a producer that
                        // writes each run (a link, a citation, a styled word)
                        // as its own text object, Chromium's way, leaves a
                        // line in several elements.
                        let same_line = prev
                            .last_line
                            .as_ref()
                            .is_some_and(|last| text::same_line(last, refs[0]));
                        let continued = same_struct
                            || same_line
                            || prev.last_line.as_ref().is_some_and(|last| {
                                let right = wrap_right(last, prev.column_right, &prev.floats);
                                text::continues(last, right, refs[0], left, key)
                            });
                        if continued {
                            let (more, removed) = text::join(&refs);
                            let (glued, removed2) = if same_line {
                                (
                                    text::abut(std::mem::take(&mut prev.chars), more, refs[0]),
                                    0,
                                )
                            } else {
                                glue(std::mem::take(&mut prev.chars), more)
                            };
                            prev.chars = glued;
                            prev.lines += refs.len() - usize::from(same_line);
                            if !same_line && let Some(last) = prev.last_line.as_ref() {
                                let mut more = steps(&[last, refs[0]]);
                                more.extend(steps(&refs));
                                prev.steps.extend(more);
                                prev.pitch = median(prev.steps.clone());
                            }
                            let start = prev.last_line.as_ref().map(|l| l.x0);
                            prev.last_line = refs.last().map(|l| {
                                let mut line = (*l).clone();
                                // One visual line: it starts where the line did.
                                if same_line && refs.len() == 1 {
                                    line.x0 = start.unwrap_or(line.x0).min(line.x0);
                                }
                                line
                            });
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
                        "Title" => Some(0),
                        "H1" | "H" => Some(1),
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
                        first_baseline: refs[0].baseline,
                        shade: refs[0]
                            .shade
                            .filter(|s| refs.iter().all(|l| l.shade == Some(*s))),
                        pitch: median(steps(&refs)),
                        steps: steps(&refs),
                        gap_above: (n == 0)
                            .then_some(object_bottom)
                            .flatten()
                            .filter(|_| {
                                matches!(
                                    items.last(),
                                    Some(Item::Table(..) | Item::Image(..) | Item::Vector(..))
                                )
                            })
                            .filter(|(page, _)| *page == element.page)
                            .map(|(_, bottom)| refs[0].top - bottom)
                            .filter(|gap| (0.0..=72.0).contains(gap)),
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
                object_bottom = Some((element.page, table.bounds.3));
                items.push(Item::Table(table.clone(), element.region));
            }
            EKind::Image(image) => {
                object_bottom = Some((element.page, image.bounds.y1));
                items.push(Item::Image(
                    image.clone(),
                    element.region,
                    element.labels.clone(),
                ));
            }
            EKind::Vector(vector) => {
                object_bottom = Some((element.page, vector.bounds.y1));
                items.push(Item::Vector(
                    vector.clone(),
                    element.region,
                    element.labels.clone(),
                    element.pictures.clone(),
                ));
            }
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
        title_style: None,
        title_key: None,
        normal: None,
        definitions: Definitions::default(),
        links: flow.links.clone(),
        bookmarks: BTreeMap::new(),
        comments: Vec::new(),
        notes: Vec::new(),
        z: 0,
        rhythm: Rhythm::default(),
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
    let (measured, below) = rhythm(&items, emitter.sheet.body.as_ref());
    emitter.rhythm = measured;
    for (item, gap) in items.iter_mut().zip(below) {
        if let Item::Para(p) = item {
            p.gap_below = gap;
        }
    }
    // The Title, when a paragraph is one: the sheet's key, or a tagged
    // `Title`'s own font.
    emitter.title_key = items
        .iter()
        .find_map(|i| match i {
            Item::Para(p) if p.heading == Some(0) => Some(
                emitter
                    .sheet
                    .title
                    .clone()
                    .or_else(|| text::dominant(p.chars.iter(), key)),
            ),
            _ => None,
        })
        .flatten();
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
                    after_twips: Some(twips(
                        emitter
                            .rhythm
                            .body_after
                            .unwrap_or(f64::from(body.half_points) / 2.0 * 0.6),
                    )),
                    line_percent: emitter.rhythm.line_percent,
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
        let rhythm = emitter.rhythm.clone();
        let level_before = |level: usize| {
            u8::try_from(level)
                .ok()
                .and_then(|l| rhythm.heading_before.get(&l))
                .copied()
        };
        let level_after = |level: usize| {
            u8::try_from(level)
                .ok()
                .and_then(|l| rhythm.heading_after.get(&l))
                .copied()
        };
        for (i, heading) in headings.iter().enumerate() {
            let hid = StyleId::new(emitter.id()?);
            let mut s = style(
                StyleKind::Paragraph,
                &format!("heading {}", i + 1),
                ParagraphProperties {
                    outline_level: u8::try_from(i).ok(),
                    keep_next: Some(true),
                    spacing: Some(Spacing {
                        before_twips: Some(twips(
                            level_before(i + 1)
                                .unwrap_or(f64::from(heading.half_points) / 2.0 * 0.8),
                        )),
                        after_twips: Some(twips(
                            level_after(i + 1)
                                .unwrap_or(f64::from(heading.half_points) / 2.0 * 0.3),
                        )),
                        // Normal's measured leading is the body's; a heading
                        // set in a larger size keeps single lines.
                        line_percent: rhythm.line_percent.map(|_| 100),
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
        if let Some(title) = emitter.title_key.clone() {
            let tid = StyleId::new(emitter.id()?);
            let mut s = style(
                StyleKind::Paragraph,
                "Title",
                ParagraphProperties {
                    spacing: Some(Spacing {
                        before_twips: Some(0),
                        after_twips: Some(twips(
                            level_after(0).unwrap_or(f64::from(title.half_points) / 2.0 * 0.3),
                        )),
                        line_percent: rhythm.line_percent.map(|_| 100),
                        ..Spacing::default()
                    }),
                    ..ParagraphProperties::default()
                },
                key_run(&title),
            );
            s.based_on = Some(id);
            s.next = Some(id);
            s.ui_priority = Some(10);
            emitter.styles.push((tid, s));
            emitter.title_style = Some(tid);
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
    // A page turned the other way from the document's (a landscape page
    // among portrait ones) is its own section on a page of its size; any
    // other page, whatever its exact size, is on the document's.
    let turned = |page: usize| -> bool {
        flow.page_sizes
            .get(page)
            .is_some_and(|&(w, h)| (w > h) != (page_w > page_h))
    };
    let size_of_page = |page: usize| -> (f64, f64) {
        if turned(page) {
            flow.page_sizes[page]
        } else {
            (page_w, page_h)
        }
    };
    let (bx0, by0, bx1, by1) = flow.body;
    let header_top = flow
        .furniture
        .parts
        .get(&(false, Variant::Default))
        .and_then(|part| {
            let row = part.rows.first().and_then(|row| row.first()).map(|l| l.top);
            let object = part
                .objects
                .iter()
                .filter(|o| !matches!(o.1, super::PartObject::Background(_)))
                .map(|o| o.0)
                .reduce(f64::min);
            match (row, object) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            }
        });
    let footer_bottom = flow
        .furniture
        .parts
        .get(&(true, Variant::Default))
        .and_then(|part| part.rows.last())
        .and_then(|row| row.last())
        .map(|l| l.bottom);
    // A header's or footer's pictures and tables are page-anchored or sit
    // in the part; the body clears them (the body of a page with a logo in
    // the header had started under the logo).
    let object_edge = |footer: bool| -> Option<f64> {
        let part = flow.furniture.parts.get(&(footer, Variant::Default))?;
        let edges = part.objects.iter().map(|(_, o)| match o {
            super::PartObject::Image(image) => {
                if footer {
                    image.bounds.y0
                } else {
                    image.bounds.y1
                }
            }
            // Behind the text: the body runs over it, but not over its
            // logo band (a letterhead's logo at the top, its address band at
            // the bottom, measured from its ink).
            super::PartObject::Background(image) => {
                let b = image.bounds;
                match image.ink {
                    Some(ink) if footer && ink.bottom < 1.0 => {
                        b.y0 + f64::from(ink.bottom) * b.height()
                    }
                    Some(ink) if !footer && ink.top > 0.0 => b.y0 + f64::from(ink.top) * b.height(),
                    _ if footer => f64::INFINITY,
                    _ => f64::NEG_INFINITY,
                }
            }
            super::PartObject::Table(t, ..) => {
                if footer {
                    t.bounds.1
                } else {
                    t.bounds.3
                }
            }
        });
        if footer {
            edges.reduce(f64::min)
        } else {
            edges.reduce(f64::max)
        }
    };
    let by0 = object_edge(false).map_or(by0, |bottom| by0.max(bottom + 4.0));
    let by1 = object_edge(true).map_or(by1, |top| by1.min(top - 4.0));
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
            match furniture_part(&mut emitter, part, page_w, margins, flow.page_sizes.len())? {
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
    let mut current_size = items
        .iter()
        .find_map(Item::page)
        .map_or((page_w, page_h), size_of_page);
    // How the current section starts: after a change of page size on a new
    // page, else on the same page.
    let mut current_start = SectionType::Continuous;
    // The section break of a change of page size starts the page; the first
    // paragraph after it must not start another.
    let mut drop_break = false;
    let mut column_sections = 0;
    let section = |emitter: &mut Emitter<'_>,
                   shape: (u16, i32),
                   size: (f64, f64),
                   start: SectionType,
                   first: bool|
     -> Result<SectionBoundary, PdfImportError> {
        let id = SectionId::new(emitter.id()?);
        // A turned page keeps the document's margins: Word's own landscape
        // section does, and the PDFs measured do.
        Ok(SectionBoundary {
            id,
            page_size: PageSize {
                width_twips: twips(size.0).clamp(1, 31_680),
                height_twips: twips(size.1).clamp(1, 31_680),
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
            section_type: (!first).then_some(start),
            // Only the document's first page is a title page; a later
            // section's first page carries the running header and footer.
            title_page: if first { title_page } else { None },
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
    // The last block was a scanned page: what follows starts a new page.
    let mut after_scan = false;
    for item in &items {
        let shape = shape_of(item.region());
        let size = item.page().map_or(current_size, size_of_page);
        let resized = size != current_size;
        if (shape.0 != current_shape.0 || resized) && !body.is_empty() {
            let boundary = section(
                &mut emitter,
                current_shape,
                current_size,
                current_start,
                sections.is_empty(),
            )?;
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
            drop_break = resized;
            current_start = if resized {
                SectionType::NextPage
            } else {
                SectionType::Continuous
            };
            current_size = size;
            if shape.0 > 1 {
                column_sections += 1;
            }
        }
        if after_scan && !matches!(item, Item::Image(_, _, lines) if !lines.is_empty()) {
            after_scan = false;
            if !drop_break && !matches!(item, Item::Para(_)) {
                body.push(BlockNode::Paragraph(Paragraph {
                    id: emitter.id()?,
                    properties: ParagraphProperties {
                        page_break_before: Some(true),
                        ..ParagraphProperties::default()
                    }
                    .into(),
                    inlines: Vec::new(),
                }));
            }
            if let Item::Para(p) = item
                && !drop_break
            {
                let mut p = p.clone();
                p.page_break = true;
                drop_break = false;
                body.push(BlockNode::Paragraph(emitter.paragraph(&p)?));
                continue;
            }
        }
        match item {
            Item::Para(p) => {
                let paragraph = if drop_break && p.page_break {
                    let mut p = p.clone();
                    p.page_break = false;
                    emitter.paragraph(&p)?
                } else {
                    emitter.paragraph(p)?
                };
                drop_break = false;
                body.push(BlockNode::Paragraph(paragraph));
            }
            Item::Table(t, _) => {
                let table = emitter.table(t)?;
                body.push(BlockNode::Table(Box::new(table)));
            }
            Item::Vector(vector, region, labels, pictures) => {
                let x0 = labels.iter().map(|l| l.x0).fold(vector.bounds.x0, f64::min);
                let (left, _) = column(&flow, *region, None, x0, page_w);
                let anchor = float_anchor(x0 - left, 0.0, None);
                let inlines = emitter
                    .group(vector, anchor, labels, pictures)?
                    .into_iter()
                    .collect();
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
                let (left, _) = column(&flow, *region, Some(page), x0, page_w);
                let inlines = emitter.floats(floats, page, top, left)?;
                body.push(BlockNode::Paragraph(Paragraph {
                    id: emitter.id()?,
                    properties: ParagraphProperties::default().into(),
                    inlines,
                }));
            }
            Item::Image(image, region, lines) if !lines.is_empty() => {
                let b = image.bounds;
                let page = lines[0].page;
                let (pw, ph) = size_of_page(page);
                let full = b.width() * b.height() >= super::BACKGROUND_SHARE * pw * ph;
                let skews = state.pages.get(page);
                let skew = |line: &ILine| skews.map_or(0.0, |p| p.skew_at(line.x0, line.baseline));
                let (anchor, size) = if full {
                    // A scanned page: the picture where the PDF drew it on a
                    // page of its own, in front of nothing (the page has no
                    // other text), its lines clickable over it.
                    let mut anchor = float_anchor(b.x0, b.y0, Some(FloatWrap::Background));
                    anchor.behind_doc = false;
                    (anchor, (b.width(), b.height()))
                } else {
                    // A picture of text in a text PDF: on its own line, as a
                    // picture is, sized as one.
                    let (left, right) = column(&flow, *region, None, b.x0, page_w);
                    let room = (
                        right - left,
                        by1 - by0 - emitter.rhythm.body_after.unwrap_or(0.0) - 2.0,
                    );
                    (
                        float_anchor((b.x0 - left).max(0.0), 0.0, None),
                        fit(b.width(), b.height(), room),
                    )
                };
                let group = emitter.image_text(image, lines, &skew, anchor, size)?;
                let mut properties = ParagraphProperties::default();
                if full && !body.is_empty() && !drop_break {
                    properties.page_break_before = Some(true);
                }
                drop_break = false;
                body.push(BlockNode::Paragraph(Paragraph {
                    id: emitter.id()?,
                    properties: properties.into(),
                    inlines: vec![group],
                }));
                after_scan = full;
                continue;
            }
            Item::Image(image, region, _) => {
                let media = emitter.media(image)?;
                let b = image.bounds;
                let (left, right) = column(&flow, *region, None, b.x0, page_w);
                let room = (
                    right - left,
                    // The paragraph's own space after it shares the page.
                    by1 - by0 - emitter.rhythm.body_after.unwrap_or(0.0) - 2.0,
                );
                let (width, height) = fit(b.width(), b.height(), room);
                let drawing = Drawing {
                    id: emitter.id()?,
                    media,
                    extent: Some(Extent {
                        width_emu: (width * 12_700.0).round().max(1.0) as i64,
                        height_emu: (height * 12_700.0).round().max(1.0) as i64,
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
    let last = section(
        &mut emitter,
        current_shape,
        current_size,
        current_start,
        sections.is_empty(),
    )?;
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
/// The typical single line of a text face, as a multiple of its size: the
/// ascent, descent and line gap of Arial, Liberation Sans, Times New Roman
/// and Helvetica all come to about 1.15 em, so `auto` spacing of 100% sets
/// lines that far apart. Measured PDF leading is expressed against it.
pub const SINGLE_LINE_EMS: f64 = 1.15;

/// A measured leading within this share of single is single (no direct
/// value): the rounding of a PDF's positions is not a spacing choice.
const SINGLE_TOLERANCE: f64 = 0.05;

/// The document's vertical rhythm as the PDF set it: line spacing for
/// Normal, the space after a body paragraph, before and after each heading
/// level (index 0 is the Title), and between list items. Every value is a
/// median over the pairs that show it, so one paragraph pushed down by a
/// float or a page's leftover space does not set it. Points; `None` keeps
/// the default.
#[derive(Clone, Debug, Default)]
struct Rhythm {
    /// `auto` line spacing for Normal, in percent of single.
    line_percent: Option<u16>,
    body_after: Option<f64>,
    /// Normal's size, in points.
    body_size: f64,
    heading_before: BTreeMap<u8, f64>,
    heading_after: BTreeMap<u8, f64>,
    list_after: Option<f64>,
    /// Per size (half points), the median pitch over size of the
    /// paragraphs set in it that have two lines or more.
    ratio_by_size: BTreeMap<u32, f64>,
}

/// A picture's size scaled down, aspect kept, to fit `room` (the column's
/// width, the body's height). A picture the PDF drew over the margins (a
/// scanned page is one picture the size of the page) overflowed the column
/// on the right and the page at the bottom, and each scanned page reflowed
/// as two: the picture cut off, then a blank page. Word and Google Docs fit
/// a pasted picture to the column the same way. O(1).
pub(super) fn fit(width: f64, height: f64, room: (f64, f64)) -> (f64, f64) {
    let scale = (room.0 / width.max(1e-6))
        .min(room.1 / height.max(1e-6))
        .min(1.0);
    if scale > 0.0 && scale.is_finite() {
        (width * scale, height * scale)
    } else {
        (width, height)
    }
}

/// The median of `values`, or `None` when there are none. O(n log n).
fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(values[values.len() / 2])
}

/// The distances between consecutive baselines of `lines` on one page,
/// ignoring a step larger than three of its ems (a paragraph that carried
/// on below a figure) or one that goes back up (a column change). O(lines).
fn steps(lines: &[&ILine]) -> Vec<f64> {
    lines
        .windows(2)
        .filter(|w| w[0].page == w[1].page)
        .map(|w| w[1].baseline - w[0].baseline)
        .filter(|step| *step > 0.0)
        .filter(|step| *step <= 3.0 * lines[0].size.max(1.0))
        .collect()
}

/// The size most of a paragraph's characters are set in, in points.
/// O(characters).
fn size_of(p: &IPara) -> Option<f64> {
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
    for c in &p.chars {
        if !c.text.trim().is_empty() && !c.sup {
            *counts.entry(c.style.half_points).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|(half, n)| (*n, std::cmp::Reverse(*half)))
        .map(|(half, _)| f64::from(half) / 2.0)
}

/// Measures the rhythm: the body's line pitch over its size gives Normal's
/// line spacing; the space between two paragraphs one above the other in a
/// column is the distance between the first's last baseline and the
/// second's first, less the second's own line (its size times the line
/// ratio it is given: the body's measured one at the body's size, single
/// otherwise). Before the measurement Normal had a fixed 0.6 em after it
/// and single spacing whatever the PDF set, so a document set at 1.4 lines
/// with a blank line between paragraphs reflowed dense and run together.
/// Returns the rhythm and, per item, the space below it when the next item
/// is a paragraph under it in the same column. O(paragraphs log paragraphs).
fn rhythm(items: &[Item], body: Option<&FontKey>) -> (Rhythm, Vec<Option<f64>>) {
    let mut below = vec![None; items.len()];
    let Some(body) = body else {
        return (Rhythm::default(), below);
    };
    let body_size = f64::from(body.half_points) / 2.0;
    let at_body = |p: &IPara| size_of(p).is_some_and(|s| (s - body_size).abs() <= 0.5);
    let plain = |p: &IPara| p.heading.is_none() && p.list.is_none();
    let ratio = median(
        items
            .iter()
            .filter_map(|i| match i {
                Item::Para(p) if plain(p) && at_body(p) => p.pitch,
                _ => None,
            })
            .map(|pitch| pitch / body_size.max(1.0))
            .filter(|r| (0.8..=3.5).contains(r))
            .collect(),
    );
    let mut by_size: BTreeMap<u32, Vec<f64>> = BTreeMap::new();
    for item in items {
        if let Item::Para(p) = item
            && plain(p)
            && let (Some(pitch), Some(size)) = (p.pitch, size_of(p))
        {
            by_size
                .entry((size * 2.0).round() as u32)
                .or_default()
                .push(pitch / size.max(1.0));
        }
    }
    let ratio_by_size: BTreeMap<u32, f64> = by_size
        .into_iter()
        .filter_map(|(half, ratios)| median(ratios).map(|m| (half, m)))
        .collect();
    let line_percent = ratio
        .map(|r| r / SINGLE_LINE_EMS)
        .filter(|f| (f - 1.0).abs() > SINGLE_TOLERANCE)
        .map(|f| (f * 100.0).round().clamp(80.0, 300.0) as u16);
    let body_ratio =
        line_percent.map_or(SINGLE_LINE_EMS, |p| f64::from(p) / 100.0 * SINGLE_LINE_EMS);
    let mut body_gaps = Vec::new();
    let mut list_gaps = Vec::new();
    let mut before: BTreeMap<u8, Vec<f64>> = BTreeMap::new();
    let mut after: BTreeMap<u8, Vec<f64>> = BTreeMap::new();
    for (k, pair) in items.windows(2).enumerate() {
        let (Item::Para(a), Item::Para(b)) = (&pair[0], &pair[1]) else {
            continue;
        };
        let Some(last) = a.last_line.as_ref() else {
            continue;
        };
        if last.page != b.page
            || a.region != b.region
            || (a.column_left - b.column_left).abs() > 1.0
            || b.first_baseline <= last.baseline
        {
            continue;
        }
        let size = size_of(b).unwrap_or(body_size);
        // The line `b` is given: its own measured leading (`own_spacing`)
        // when it is body text, else the body's or single.
        let measured: Option<f64> = b
            .pitch
            .map(|p| p / size.max(1.0))
            .or_else(|| ratio_by_size.get(&((size * 2.0).round() as u32)).copied())
            .filter(|ratio| (0.8..=3.5).contains(ratio));
        let line = match (plain(b), measured) {
            (true, Some(ratio)) => ratio,
            (true, None) if at_body(b) => body_ratio,
            _ => SINGLE_LINE_EMS,
        };
        let gap = b.first_baseline - last.baseline - size * line;
        // A gap of more than six body lines is a page's or a figure's space,
        // not the paragraph's.
        if !(-0.5 * body_size..=6.0 * body_size * body_ratio).contains(&gap) {
            continue;
        }
        let gap = gap.max(0.0);
        // Above a heading the space is the heading style's.
        if b.heading.is_none() {
            below[k] = Some(gap);
        }
        if let Some(level) = b.heading {
            before.entry(level).or_default().push(gap);
        }
        if let Some(level) = a.heading {
            after.entry(level).or_default().push(gap);
        } else if a.list.is_some() && b.list.is_some() && a.list.map(|l| l.0) == b.list.map(|l| l.0)
        {
            list_gaps.push(gap);
        } else if plain(a) && plain(b) && at_body(a) && at_body(b) {
            body_gaps.push(gap);
        }
    }
    let body_after = median(body_gaps);
    let rhythm = Rhythm {
        line_percent,
        body_after,
        body_size,
        // The space above a heading is what is left after the paragraph
        // before it has put its own space after (Word adds the two).
        heading_before: before
            .into_iter()
            .filter_map(|(level, gaps)| {
                median(gaps).map(|g| (level, (g - body_after.unwrap_or(0.0)).max(0.0)))
            })
            .collect(),
        heading_after: after
            .into_iter()
            .filter_map(|(level, gaps)| median(gaps).map(|g| (level, g)))
            .collect(),
        list_after: median(list_gaps),
        ratio_by_size,
    };
    (rhythm, below)
}

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
    part: &super::Part,
    page_width: f64,
    margins: PageMargins,
    total_pages: usize,
) -> Result<Option<(HeaderFooterId, HeaderFooter)>, PdfImportError> {
    let (rows, numbers) = (&part.rows, &part.numbers);
    if rows.is_empty() && part.objects.is_empty() {
        return Ok(None);
    }
    // Blocks in the order they sit down the band: text rows, tables, and
    // pictures (carried by the block they precede).
    let mut placed: Vec<(f64, BlockNode)> = Vec::new();
    let mut pictures: Vec<(f64, InlineNode)> = Vec::new();
    for (top, object) in &part.objects {
        match object {
            super::PartObject::Table(table, page, varying) => {
                let built =
                    emitter.numbered_table(table, Some((page + 1, total_pages, varying)))?;
                placed.push((*top, BlockNode::Table(Box::new(built))));
            }
            super::PartObject::Image(image) | super::PartObject::Background(image) => {
                let behind = matches!(object, super::PartObject::Background(_));
                let media = emitter.media(image)?;
                let b = image.bounds;
                emitter.z += 1;
                pictures.push((
                    *top,
                    InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
                        id: emitter.id()?,
                        media,
                        extent: Extent {
                            width_emu: crate::fixed::emu(b.width()).max(1),
                            height_emu: crate::fixed::emu(b.height()).max(1),
                        },
                        anchor: DrawingAnchor {
                            horizontal: AnchorHorizontal {
                                relative_from: HorizontalAnchor::Page,
                                position: HorizontalPosition::Offset(crate::fixed::emu(b.x0)),
                            },
                            vertical: AnchorVertical {
                                relative_from: VerticalAnchor::Page,
                                position: VerticalPosition::Offset(crate::fixed::emu(b.y0)),
                            },
                            wrap: WrapMode::None,
                            wrap_text: None,
                            wrap_distances: WrapDistances::default(),
                            wrap_polygon: None,
                            behind_doc: behind,
                            allow_overlap: true,
                        },
                        descr: None,
                        relative_height: Some(emitter.z),
                        crop: None,
                        opacity: None,
                        hyperlink: None,
                        border: None,
                        flip_h: false,
                        flip_v: false,
                        rotation: None,
                    })),
                ));
            }
        }
    }
    let left = f64::from(margins.start_twips) / 20.0;
    let right = page_width - f64::from(margins.end_twips) / 20.0;
    let width = (right - left).max(1.0);
    let body = emitter.sheet.body.clone();
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
        let top = row.first().map_or(0.0, |l| l.top);
        placed.push((
            top,
            BlockNode::Paragraph(Paragraph {
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
            }),
        ));
    }
    placed.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut blocks: Vec<BlockNode> = placed.into_iter().map(|p| p.1).collect();
    // A part must end in a paragraph (a table cannot close a story), and
    // the pictures, page-anchored, ride on its first paragraph.
    if !matches!(blocks.last(), Some(BlockNode::Paragraph(_))) {
        blocks.push(BlockNode::Paragraph(Paragraph {
            id: emitter.id()?,
            properties: ParagraphProperties::default().into(),
            inlines: Vec::new(),
        }));
    }
    if !pictures.is_empty()
        && let Some(BlockNode::Paragraph(first)) = blocks
            .iter_mut()
            .find(|b| matches!(b, BlockNode::Paragraph(_)))
    {
        let mut inlines: Vec<InlineNode> = pictures.into_iter().map(|p| p.1).collect();
        inlines.append(&mut first.inlines);
        first.inlines = inlines;
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
