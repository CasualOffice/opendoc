//! "Convert to flowing document": the PDF source state to a **new**
//! `v1::Document` of real paragraphs, headings, lists, tables, header/footer
//! parts, footnotes and column sections (`docs/182` §4.2, §5.3).
//!
//! [`build`] is a pure function of the import's [`PdfSourceState`] (the I4
//! sidecar the fixed-layout job kept) and its resources; it parses nothing
//! again and changes nothing in the fixed-layout document, which stays open
//! (reflow is not an undoable transform of the frames: no edit history
//! survives a change of document shape, which is Word's PDF Reflow model
//! too).
//!
//! The passes, each its own module and each reported (PDFR-I4):
//!
//! | Pass | Module | Finding |
//! | --- | --- | --- |
//! | page furniture: repeated top/bottom blocks, `/Artifact` content | here | `header_footer_inferred`, `artifact_omitted` |
//! | ruled grids, whitespace columns | `tables` | `table_inferred`, `table_topology_degraded` |
//! | superscripts matched to bottom blocks | `notes` | `footnote_inferred` |
//! | reading order: structure tree, else XY-Cut++ | here, `order` | `reading_order_inferred` |
//! | lines to paragraphs, de-hyphenation | `text` | `paragraph_boundary_inferred`, `dehyphenation_inferred` |
//! | font clusters to Normal / Heading n | `text` | `style_inferred`, `heading_inferred` |
//! | markers with a hanging indent to numbering | `text`, `emit` | `list_inferred` |
//! | column regions to `SectionColumns` | `order`, `emit` | `columns_inferred` |
//! | links, notes as comments, destinations as bookmarks | `emit` | `hyperlink_inferred`, `comment_inferred`, `bookmark_inferred`, `link_unresolved` |
//! | pictures beside text and vector art under it float on their paragraph; other vector art is a group on its own line | here, `emit` | `float_inferred`, `vector_placed`; masks over a picture `vector_omitted` |
//!
//! Complexity: O(document log document); every grouping and ordering pass is
//! guarded by an n-versus-2n work test (SKILL §8), and the counters are on
//! [`Reflowed::work`].

pub mod decorate;
mod emit;
pub mod notes;
pub mod order;
pub mod record;
pub mod tables;
pub mod text;
pub mod watermark;

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::v1::Document;
use casual_doc_pdf_read::{AnnotationEvidence, AnnotationKind, Bounds, LinkTarget, StructNode};
// Own line: rules made vector art.
use casual_doc_pdf_read::{PathCommand, Point};

pub use record::{Fill, PdfSourceState, Rule, SourceBlock, SourceImage, SourcePage, SourceVector};

use crate::PdfImportError;
use crate::group::{self, Line};
use crate::report::{self, Findings, PdfImportReport, PdfModelOutcome as Outcome};
use order::Rect;
use tables::ITable;
use text::FontKey;

/// Furniture sits in the top or bottom band of this share of the page.
pub const FURNITURE_BAND: f64 = 0.1;
/// Text repeated on at least this share of the pages (and on two) is
/// furniture.
pub const FURNITURE_REPEAT: f64 = 0.5;
/// A character at most this share of its line's size, raised, is superscript.
pub const SUPERSCRIPT_SIZE: f64 = 0.8;

/// One character, with what reflow attached to it.
#[derive(Clone, Debug)]
pub struct PChar {
    /// The text.
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
    pub style: group::Style,
    /// Superscript (smaller and raised against its line).
    pub sup: bool,
    /// The link (index into the document's links) covering it.
    pub link: Option<usize>,
    /// The footnote it references.
    pub note: Option<usize>,
    /// Comments anchored on it.
    pub comments: Vec<usize>,
    /// Underlined (a rule under it, `decorate`).
    pub underline: bool,
    /// Struck through (a rule through it).
    pub strike: bool,
    /// Shaded (a fill tight behind its run: a highlight).
    pub shade: Option<[u8; 3]>,
    /// The letter spacing of its run, in points: the even space the PDF
    /// put between its glyphs beyond their widths (`Tc`, a subtitle's
    /// expanded tracking). Zero when the run is set solid.
    pub tracking: f64,
}

/// One line.
#[derive(Clone, Debug)]
pub struct ILine {
    /// Page.
    pub page: usize,
    /// Characters, spaces included.
    pub chars: Vec<PChar>,
    /// Left.
    pub x0: f64,
    /// Right.
    pub x1: f64,
    /// Top.
    pub top: f64,
    /// Bottom.
    pub bottom: f64,
    /// The baseline of its largest characters.
    pub baseline: f64,
    /// Its largest em size.
    pub size: f64,
    /// Marked-content id.
    pub mcid: Option<i32>,
    /// A rule under it that is its paragraph's bottom border: thickness in
    /// points and colour (`decorate`).
    pub border_below: Option<(f64, Option<[u8; 3]>)>,
    /// A fill behind the whole line with room to spare: its paragraph's
    /// shading (`decorate`).
    pub shade: Option<[u8; 3]>,
}

/// Even space between letters wider than tracking noise, in ems: below it
/// a run is set solid (kerning and rounding leave a few hundredths).
const TRACKING_EMS: f64 = 0.04;

/// Sets each run's letter spacing ([`PChar::tracking`]): over a run of one
/// style, the gaps between adjacent single-character, non-space glyphs
/// (the PDF's position less the glyph's width) when there are at least
/// three and they agree (each within a third of their median) on a
/// median of [`TRACKING_EMS`] to half an em. Kerning is uneven and a word
/// space is wider, so neither reads as tracking. O(characters).
fn track(chars: &mut [PChar]) {
    let mut start = 0;
    while start < chars.len() {
        let mut end = start + 1;
        while end < chars.len() && chars[end].style == chars[start].style {
            end += 1;
        }
        let run = &chars[start..end];
        let letter = |c: &PChar| c.text.chars().count() == 1 && !c.text.trim().is_empty();
        let mut gaps: Vec<f64> = run
            .windows(2)
            .filter(|w| letter(&w[0]) && letter(&w[1]))
            .map(|w| w[1].x0 - w[0].x1)
            .collect();
        let size = run.iter().map(|c| c.size).fold(0.0, f64::max).max(1.0);
        if gaps.len() >= 3 {
            gaps.sort_by(f64::total_cmp);
            let median = gaps[gaps.len() / 2];
            let agree = gaps
                .iter()
                .all(|g| (g - median).abs() <= median.abs() / 3.0);
            if agree && (TRACKING_EMS * size..=0.5 * size).contains(&median) {
                for c in &mut chars[start..end] {
                    c.tracking = median;
                }
            }
        }
        start = end;
    }
}

impl ILine {
    fn new(line: &Line, page: usize, mcid: Option<i32>) -> Self {
        let size = line.size;
        let baseline = line
            .chars
            .iter()
            .filter(|c| (c.size - size).abs() < 0.01)
            .map(|c| c.baseline)
            .next()
            .unwrap_or(line.baseline);
        // A glyph with no Unicode (empty text) has no character to edit.
        let mut chars: Vec<PChar> = line
            .chars
            .iter()
            .filter(|c| !c.text.is_empty())
            .map(|c| PChar {
                text: c.text.clone(),
                x0: c.x0,
                x1: c.x1,
                baseline: c.baseline,
                size: c.size,
                style: c.style,
                sup: c.size <= SUPERSCRIPT_SIZE * size
                    && c.baseline < baseline - 0.15 * size
                    && !c.text.trim().is_empty(),
                link: None,
                note: None,
                comments: Vec::new(),
                underline: false,
                strike: false,
                shade: None,
                tracking: 0.0,
            })
            .collect();

        track(&mut chars);
        Self {
            page,
            chars,
            x0: line.x0,
            x1: line.x1,
            top: line.top,
            bottom: line.bottom,
            baseline,
            size,
            mcid,
            border_below: None,
            shade: None,
        }
    }

    /// The same line holding other characters (bounds recomputed).
    #[must_use]
    pub fn with_chars(&self, chars: Vec<PChar>) -> Self {
        let x0 = chars.iter().map(|c| c.x0).fold(f64::INFINITY, f64::min);
        let x1 = chars.iter().map(|c| c.x1).fold(f64::NEG_INFINITY, f64::max);
        Self {
            chars,
            x0,
            x1,
            ..self.clone()
        }
    }

    pub(crate) fn text(&self) -> String {
        self.chars.iter().map(|c| c.text.as_str()).collect()
    }
}

/// The em size most of these lines' characters carry.
#[must_use]
pub fn dominant_size(lines: &[ILine]) -> f64 {
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
    for line in lines {
        for c in &line.chars {
            if !c.text.trim().is_empty() && !c.sup {
                *counts.entry((c.size * 2.0).round() as u32).or_default() += 1;
            }
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map_or(0.0, |(half, _)| f64::from(half) / 2.0)
}

/// What an element holds.
#[derive(Clone, Debug)]
pub enum EKind {
    /// Lines of text.
    Text(Vec<ILine>),
    /// A table.
    Table(ITable),
    /// A picture on its own line.
    Image(SourceImage),
    /// Vector art on its own line.
    Vector(SourceVector),
}

/// A graphic that floats.
#[derive(Clone, Debug)]
pub enum Graphic {
    /// A picture.
    Image(SourceImage),
    /// Vector art, drawn as a shape group.
    Vector(SourceVector),
}

impl Graphic {
    /// Where it sits on its page.
    #[must_use]
    pub fn bounds(&self) -> Bounds {
        match self {
            Self::Image(image) => image.bounds,
            Self::Vector(vector) => vector.bounds,
        }
    }
}

/// How a floating graphic wraps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloatWrap {
    /// Text runs down its sides (`wp:wrapSquare`): a graphic beside text.
    Beside,
    /// It paints behind the text (`wp:wrapNone behindDoc`): art under text.
    Behind,
    /// The page's background: a picture covering much of the page with text
    /// over it, behind the text and anchored to the page where the PDF drew
    /// it (`wp:wrapNone behindDoc`, page-relative), so it pushes nothing.
    Background,
}

/// The share of a page's area a picture with text over it must cover to be
/// the page's background rather than a picture in the text.
pub const BACKGROUND_SHARE: f64 = 0.4;

/// A graphic floated on the text element it belongs with.
#[derive(Clone, Debug)]
pub struct Float {
    /// Page.
    pub page: usize,
    /// The graphic.
    pub graphic: Graphic,
    /// Its wrap.
    pub wrap: FloatWrap,
}

/// A graphic at most this share of the page's text width may float beside
/// text; a wider one has no room for a text channel and stays on its line.
pub const FLOAT_MAX_WIDTH: f64 = 0.6;
/// Text beside a graphic shares at least this share of the shorter one's
/// height with it.
pub const FLOAT_BAND_SHARE: f64 = 0.5;
/// Text beside a graphic is at most this far (points) from its edge; text
/// further away is another column's.
pub const FLOAT_GAP: f64 = 36.0;
/// Vector art under at least this share of a text element's area is behind
/// that text (shading, a panel).
pub const BEHIND_TEXT_SHARE: f64 = 0.5;

/// Where a graphic goes: `Some((text element, wrap))` when it floats on a
/// text element, `None` when it keeps its own line in reading order.
///
/// - Vector art under at least [`BEHIND_TEXT_SHARE`] of a text element is
///   behind that text, anchored on the topmost such element.
/// - A graphic at most [`FLOAT_MAX_WIDTH`] of the text width, with text
///   beside it (sharing [`FLOAT_BAND_SHARE`] of the shorter height, within
///   [`FLOAT_GAP`] of its side), floats beside the topmost such text, unless
///   text above or below it is no wider than 1.25 times the graphic: then it
///   fills its own column and keeps its line (Word's inline picture).
///
/// O(text elements on the page).
#[must_use]
pub fn placement(
    graphic: &Rect,
    vector: bool,
    texts: &[(usize, Rect)],
) -> Option<(usize, FloatWrap)> {
    let (g, gw, gh) = (graphic, graphic.width(), graphic.height());
    if vector {
        let under = texts
            .iter()
            .filter(|(_, t)| {
                let w = (t.x1.min(g.x1) - t.x0.max(g.x0)).max(0.0);
                let h = (t.y1.min(g.y1) - t.y0.max(g.y0)).max(0.0);
                let area = (t.width() * t.height()).max(f64::MIN_POSITIVE);
                w * h >= BEHIND_TEXT_SHARE * area
            })
            .min_by(|a, b| a.1.y0.total_cmp(&b.1.y0).then(a.0.cmp(&b.0)));
        if let Some(&(i, _)) = under {
            return Some((i, FloatWrap::Behind));
        }
    }
    let left = texts
        .iter()
        .map(|(_, t)| t.x0)
        .fold(f64::INFINITY, f64::min);
    let right = texts
        .iter()
        .map(|(_, t)| t.x1)
        .fold(f64::NEG_INFINITY, f64::max);
    if texts.is_empty() || gw > FLOAT_MAX_WIDTH * (right - left) {
        return None;
    }
    let mut beside: Option<(usize, f64)> = None;
    for &(i, t) in texts {
        let overlap_x = t.x1.min(g.x1) - t.x0.max(g.x0);
        if overlap_x > 2.0 {
            // Above or below it: a column no wider than the graphic is the
            // graphic's own.
            if t.width() <= 1.25 * gw {
                return None;
            }
            continue;
        }
        let gap = (g.x0 - t.x1).max(t.x0 - g.x1);
        let shared = t.y1.min(g.y1) - t.y0.max(g.y0);
        if gap <= FLOAT_GAP
            && shared > 0.0
            && shared >= FLOAT_BAND_SHARE * gh.min(t.height())
            && beside.is_none_or(|(_, top)| t.y0 < top)
        {
            beside = Some((i, t.y0));
        }
    }
    beside.map(|(i, _)| (i, FloatWrap::Beside))
}

/// One element of a page, before and after ordering.
#[derive(Clone, Debug)]
pub struct Element {
    /// Page.
    pub page: usize,
    /// Box.
    pub rect: Rect,
    /// Contents.
    pub kind: EKind,
    /// Index of its first structure leaf, when tagged.
    pub leaf: Option<usize>,
    /// The structure element owning that leaf.
    pub struct_element: Option<u32>,
    /// The leaf's role (`H1`, `P`, `LI`, ...).
    pub role: Option<String>,
    /// Column region (document-wide index into `Flow::regions`).
    pub region: Option<usize>,
    /// The structure elements from the root down to the leaf's owner.
    pub lineage: Vec<StructNode>,
    /// Graphics floated on it (text elements only).
    pub floats: Vec<Float>,
    /// Vector art only: the text drawn inside it (a chart's axis labels and
    /// legend, a diagram's captions), drawn with it (`labels`).
    pub labels: Vec<ILine>,
    /// Vector art only: the pictures that are pieces of the same figure (a
    /// screenshot with its frame and shadow tiles), drawn with it, under
    /// its shapes (`cluster_pictures`).
    pub pictures: Vec<SourceImage>,
}

/// The ordinal of the nearest `role` element on a lineage.
fn nearest(lineage: &[StructNode], roles: &[&str]) -> Option<u32> {
    lineage
        .iter()
        .rev()
        .find(|n| roles.contains(&n.role.as_str()))
        .map(|n| n.ordinal)
}

/// One column band of a row: its column, its lines, and its horizontal extent.
type Band = (Option<u32>, Vec<ILine>, (f64, f64));
/// One row of bands, keyed by its row index.
type Row = (Option<u32>, Vec<Band>);

/// A tagged table (`Table`/`TR`/`TH`/`TD`, ISO 32000-1 §14.8.4.3.4): the
/// consecutive elements under one `Table` element, rows by `TR`, cells by
/// `TD`/`TH`, each in structure order. Column edges are each column's
/// leftmost cell edge. O(elements in the table log).
fn tagged_table(cells: Vec<Element>) -> Element {
    let mut rows: Vec<Row> = Vec::new();
    let page = cells[0].page;
    let (mut x0, mut y0, mut x1, mut y1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    let leaf = cells.iter().filter_map(|c| c.leaf).min();
    let region = cells[0].region;
    let mut floats = Vec::new();
    for mut cell in cells {
        floats.append(&mut cell.floats);
        x0 = x0.min(cell.rect.x0);
        y0 = y0.min(cell.rect.y0);
        x1 = x1.max(cell.rect.x1);
        y1 = y1.max(cell.rect.y1);
        let row = nearest(&cell.lineage, &["TR"]);
        let td = nearest(&cell.lineage, &["TD", "TH"]);
        let EKind::Text(lines) = cell.kind else {
            continue;
        };
        if rows.last().is_none_or(|r| r.0 != row) {
            rows.push((row, Vec::new()));
        }
        let cells = &mut rows.last_mut().expect("pushed").1;
        match cells.last_mut() {
            Some((id, existing, bounds)) if *id == td && td.is_some() => {
                bounds.0 = bounds.0.min(cell.rect.x0);
                bounds.1 = bounds.1.max(cell.rect.x1);
                existing.extend(lines);
            }
            _ => cells.push((td, lines, (cell.rect.x0, cell.rect.x1))),
        }
    }
    let columns = rows.iter().map(|r| r.1.len()).max().unwrap_or(1);
    let mut edges = vec![f64::INFINITY; columns];
    for (_, cells) in &rows {
        for (k, (_, _, (left, _))) in cells.iter().enumerate() {
            edges[k] = edges[k].min(*left);
        }
    }
    let mut widths: Vec<f64> = (0..columns)
        .map(|k| {
            let next = edges.get(k + 1).copied().unwrap_or(x1);
            (next - edges[k]).max(12.0)
        })
        .collect();
    if widths.is_empty() {
        widths.push((x1 - x0).max(12.0));
    }
    let mut table = ITable {
        rows: rows
            .into_iter()
            .map(|(_, cells)| {
                let n = cells.len();
                cells
                    .into_iter()
                    .enumerate()
                    .map(|(k, (_, lines, (left, right)))| tables::ICell {
                        lines,
                        // A short row's last cell spans the remaining columns.
                        span: if k + 1 == n {
                            u32::try_from(columns - k).unwrap_or(1)
                        } else {
                            1
                        },
                        vmerge: None,
                        x0: left,
                        x1: right,
                        ..tables::ICell::default()
                    })
                    .collect()
            })
            .collect(),
        columns: widths,
        ruled: false,
        bounds: (x0, y0, x1, y1),
        ..ITable::default()
    };
    // Its cells' alignment and the grid from it, as for a borderless table.
    let aligned = tables::align_cells(&mut table, &[]);
    tables::align_grid(&mut table, &aligned);
    Element {
        page,
        rect: Rect { x0, y0, x1, y1 },
        kind: EKind::Table(table),
        leaf,
        struct_element: None,
        role: Some("Table".to_owned()),
        region,
        lineage: Vec::new(),
        floats,
        labels: Vec::new(),
        pictures: Vec::new(),
    }
}

/// The share of a page a background covers to be the whole page's.
pub const FULL_PAGE: f64 = 0.85;

/// Takes out of the elements' floats every page background that repeats in
/// the same place and size (2 pt; any that covers 85% of its page is one
/// class) on at least two pages and half of them, and returns one of each,
/// the second page's: a letterhead printed as a picture the size of the
/// page. Each page's copy, floated on whichever paragraph the reflow put
/// first on that page of the PDF, landed on arbitrary pages of the flowing
/// document (twice on some, never on others). O(floats log floats).
fn repeated_backgrounds(
    elements: &mut [Element],
    sizes: &[(f64, f64)],
    work: &mut u64,
) -> Vec<SourceImage> {
    let pages = sizes.len();
    let bucket = |v: f64| (v / 2.0).round() as i64;
    // A background covering nearly the whole page is the page's, wherever a
    // scanner or a producer put it a few points off: those are one class.
    let key = |b: &Bounds, page: usize| {
        let full = sizes
            .get(page)
            .is_some_and(|&(w, h)| b.width() * b.height() >= FULL_PAGE * w * h);
        if full {
            (i64::MIN, 0, 0, 0)
        } else {
            (
                bucket(b.x0),
                bucket(b.y0),
                bucket(b.width()),
                bucket(b.height()),
            )
        }
    };
    let mut seen: BTreeMap<(i64, i64, i64, i64), BTreeSet<usize>> = BTreeMap::new();
    for e in elements.iter() {
        for f in &e.floats {
            *work += 1;
            if f.wrap == FloatWrap::Background {
                seen.entry(key(&f.graphic.bounds(), f.page))
                    .or_default()
                    .insert(f.page);
            }
        }
    }
    let repeated: BTreeSet<(i64, i64, i64, i64)> = seen
        .into_iter()
        .filter(|(_, on)| on.len() >= 2 && on.len() * 2 >= pages)
        .map(|(k, _)| k)
        .collect();
    if repeated.is_empty() {
        return Vec::new();
    }
    let mut out: BTreeMap<(i64, i64, i64, i64), SourceImage> = BTreeMap::new();
    for e in elements.iter_mut() {
        e.floats.retain(|f| {
            let k = key(&f.graphic.bounds(), f.page);
            if f.wrap != FloatWrap::Background || !repeated.contains(&k) {
                return true;
            }
            if let Graphic::Image(image) = &f.graphic {
                // The second page's copy: a first page is the likeliest to
                // differ.
                if f.page > 0 || !out.contains_key(&k) {
                    out.entry(k).or_insert_with(|| image.clone());
                }
            }
            false
        });
    }
    out.into_values().collect()
}

/// The digit runs of a cell's text, in order. O(characters).
pub(crate) fn digit_runs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut run = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() {
            run.push(c);
        } else if !run.is_empty() {
            out.push(std::mem::take(&mut run));
        }
    }
    if !run.is_empty() {
        out.push(run);
    }
    out
}

/// A cell's text. O(characters).
pub(crate) fn cell_text(table: &ITable, r: usize, c: usize) -> String {
    table
        .rows
        .get(r)
        .and_then(|row| row.get(c))
        .map(|cell| {
            cell.lines
                .iter()
                .map(ILine::text)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

/// The digit runs of `table` that differ in some other instance of it:
/// a running title block's page number changes from page to page, its form
/// number and revision do not. O(cells x instances).
fn varying_runs(table: &ITable, instances: &[&ITable]) -> BTreeSet<(usize, usize, usize)> {
    let mut out = BTreeSet::new();
    for (r, row) in table.rows.iter().enumerate() {
        for c in 0..row.len() {
            let mine = digit_runs(&cell_text(table, r, c));
            for other in instances {
                let theirs = digit_runs(&cell_text(other, r, c));
                for (k, value) in mine.iter().enumerate() {
                    if theirs.get(k).is_some_and(|t| t != value) {
                        out.insert((r, c, k));
                    }
                }
            }
        }
    }
    out
}

/// The share of a page, from its top or its bottom, a repeated table or
/// picture must lie in to be the header's or the footer's.
pub const OBJECT_BAND: f64 = 0.3;

/// A repeated object's identity: footer or header, table or picture, its
/// place (2 pt buckets, measured from its own edge of the page) and its
/// grid's shape (a table) or its picture's type.
type ObjectKey = (bool, bool, i64, i64, i64, i64, String);

/// Takes out of `elements` every table and picture that repeats, in the
/// same place with the same text, at the top or bottom of at least two
/// pages and half of them (a form's title block, a letterhead's logo); one
/// instance of each is returned with whether it is a footer, whether it is
/// the first page's own variant (a table in a repeated table's place on the
/// first page only, with other text), and its top.
/// Repeated text was already furniture (`furniture`); repeated tables and
/// pictures were body content on every page. O(elements log elements).
fn repeated_objects(
    elements: &mut Vec<Element>,
    pages: &[SourcePage],
    work: &mut u64,
) -> Vec<(bool, bool, f64, PartObject)> {
    let bucket = |v: f64| (v / 2.0).round() as i64;
    let mut seen: BTreeMap<ObjectKey, (BTreeSet<usize>, Vec<usize>)> = BTreeMap::new();
    for (i, e) in elements.iter().enumerate() {
        *work += 1;
        // A picture with line boxes over it is the text of its page.
        if !e.labels.is_empty() && matches!(e.kind, EKind::Image(_)) {
            continue;
        }
        let Some(page) = pages.get(e.page) else {
            continue;
        };
        let h = page.height;
        let footer = if e.rect.y1 <= OBJECT_BAND * h {
            false
        } else if e.rect.y0 >= (1.0 - OBJECT_BAND) * h {
            true
        } else {
            continue;
        };
        // A table is known by its place and its grid: its text can differ
        // from page to page (a page number, a title page's own wording).
        let (table, text) = match &e.kind {
            EKind::Table(t) => (
                true,
                format!(
                    "{}x{}",
                    t.rows.len(),
                    t.rows.iter().map(Vec::len).max().unwrap_or(0)
                ),
            ),
            // The same logo is often a fresh image object on every page:
            // its place and size identify it.
            EKind::Image(image) => (false, image.media.media_type.clone()),
            _ => continue,
        };
        let y = if footer { h - e.rect.y1 } else { e.rect.y0 };
        let key = (
            footer,
            table,
            bucket(e.rect.x0),
            bucket(y),
            bucket(e.rect.x1 - e.rect.x0),
            bucket(e.rect.y1 - e.rect.y0),
            text,
        );
        let entry = seen.entry(key).or_default();
        entry.0.insert(e.page);
        entry.1.push(i);
    }
    let total = pages.len();
    let mut take: BTreeSet<usize> = BTreeSet::new();
    let mut out = Vec::new();
    // Accepted places (the key without its text), for the first page's own
    // variant of a repeated table (a form's first page says "Page 1" where
    // its digits are the same, but its title block's text can differ).
    let mut places: BTreeSet<(bool, bool, i64, i64, i64, i64)> = BTreeSet::new();
    let mut pending: Vec<(ObjectKey, Vec<usize>)> = Vec::new();
    for (key, (on, at)) in seen {
        if on.len() < 2 || on.len() * 2 < total {
            if on.len() == 1 && on.contains(&0) {
                pending.push((key, at));
            }
            continue;
        }
        let (footer, table, x, y, w, h, _) = key;
        places.insert((footer, table, x, y, w, h));
        // The instance from the first page after the first: a first page
        // is the likeliest to differ.
        let first = at
            .iter()
            .copied()
            .find(|&i| elements[i].page > 0)
            .unwrap_or(at[0]);
        let e = &elements[first];
        let object = match &e.kind {
            EKind::Table(t) => {
                let tables: Vec<&ITable> = at
                    .iter()
                    .filter_map(|&i| match &elements[i].kind {
                        EKind::Table(t) => Some(t),
                        _ => None,
                    })
                    .collect();
                PartObject::Table(t.clone(), e.page, varying_runs(t, &tables))
            }
            EKind::Image(image) => PartObject::Image(image.clone()),
            _ => continue,
        };
        out.push((footer, false, e.rect.y0, object));
        take.extend(at);
    }
    for ((footer, table, x, y, w, h, _), at) in pending {
        if !table || !places.contains(&(footer, table, x, y, w, h)) {
            continue;
        }
        let e = &elements[at[0]];
        if let EKind::Table(t) = &e.kind {
            out.push((
                footer,
                true,
                e.rect.y0,
                PartObject::Table(t.clone(), e.page, BTreeSet::new()),
            ));
            take.extend(at);
        }
    }
    if !take.is_empty() {
        let mut i = 0;
        elements.retain(|_| {
            let keep = !take.contains(&i);
            i += 1;
            keep
        });
    }
    out
}

/// A label line: at most six words and forty characters (an axis value,
/// a legend entry, a box's caption), never prose.
fn label_line(line: &ILine) -> bool {
    let text: String = line.chars.iter().map(|c| c.text.as_str()).collect();
    text.split_whitespace().count() <= 6 && text.trim().chars().count() <= 40
}

/// The smallest drawing whose text is its own: a chart or a diagram, at
/// least an inch wide, half an inch tall and of three paths.
const LABELLED_ART: (f64, f64, usize) = (72.0, 36.0, 3);

/// The largest share of the page a figure covers: anything larger is a
/// background.
const FIGURE_AREA: f64 = 0.5;

/// How near two runs of vector art are to be one figure, in points.
const FIGURE_GAP: f64 = 6.0;

/// Runs of vector art that overlap or nearly touch, merged into one, paths
/// in paint order: a chart's frame, gridlines, bars and legend keys are
/// painted between its text, so each was a drawing of its own, the chart
/// reflowed as several pieces. O(runs^2) on a page, which holds few.
/// Each run carries whether it is only rules (`free_rules`); a merged
/// figure is only rules when all its pieces are.
fn merge_figures(
    vectors: Vec<(SourceVector, bool)>,
    area: f64,
    work: &mut u64,
) -> Vec<(SourceVector, bool)> {
    // A page-sized fill (a page's or a frame's background) is behind
    // everything; it is no figure's piece.
    let background = |b: &Bounds| b.width() * b.height() > FIGURE_AREA * area;
    let near = |a: &Bounds, b: &Bounds| {
        !background(a)
            && !background(b)
            && a.x0 <= b.x1 + FIGURE_GAP
            && b.x0 <= a.x1 + FIGURE_GAP
            && a.y0 <= b.y1 + FIGURE_GAP
            && b.y0 <= a.y1 + FIGURE_GAP
    };
    let mut out: Vec<(SourceVector, bool)> = Vec::new();
    for vector in vectors {
        out.push(vector);
        // Fold every earlier figure the last one meets into it, until none.
        loop {
            let last = out.len() - 1;
            *work += last as u64;
            let Some(k) = (0..last).find(|&k| near(&out[k].0.bounds, &out[last].0.bounds)) else {
                break;
            };
            let (other, rules) = out.remove(k);
            let last = out.len() - 1;
            out[last].1 &= rules;
            let merged = &mut out[last].0;
            merged.bounds = Bounds {
                x0: merged.bounds.x0.min(other.bounds.x0),
                y0: merged.bounds.y0.min(other.bounds.y0),
                x1: merged.bounds.x1.max(other.bounds.x1),
                y1: merged.bounds.y1.max(other.bounds.y1),
            };
            let mut paths = other.paths;
            paths.append(&mut merged.paths);
            merged.paths = paths;
        }
    }
    out
}

/// Whether vector art draws nothing on a white page: every path a white
/// fill with no stroke (LibreOffice paints the white knock-out around a
/// picture text wraps about as four white rectangles; Qt paints its page
/// white). Such art is not placed, and it is no figure for a picture to
/// join: the knock-out had made a wrapped picture an own-line figure. O(paths).
fn invisible(vector: &SourceVector) -> bool {
    vector
        .paths
        .iter()
        .all(|p| p.stroke.is_none() && matches!(p.fill, Some(Some([255, 255, 255, _]))))
}

/// `vector` without the paths that lie mostly off the page (a clip in the
/// PDF hid them: a mockup's panel drawn past the page edge and cut there),
/// its bounds recomputed; `None` when nothing is left. Drawn, such a path
/// was a black block beside the page in a group as wide as two pages.
/// O(paths).
fn on_page(vector: &SourceVector, width: f64, height: f64) -> Option<SourceVector> {
    let inside = |b: &Bounds| {
        let w = (b.x1.min(width) - b.x0.max(0.0)).max(0.0);
        let h = (b.y1.min(height) - b.y0.max(0.0)).max(0.0);
        let area = b.width().max(0.01) * b.height().max(0.01);
        // A rule (no area) counts by its length on the page.
        if b.width() < 0.5 || b.height() < 0.5 {
            return w.max(h) >= 0.5 * b.width().max(b.height());
        }
        w * h >= 0.5 * area
    };
    let paths: Vec<casual_doc_pdf_read::PathEvidence> = vector
        .paths
        .iter()
        .filter(|p| inside(&p.bounds))
        .cloned()
        .collect();
    if paths.len() == vector.paths.len() {
        return Some(vector.clone());
    }
    let first = paths.first()?.bounds;
    let bounds = paths.iter().fold(first, |b, p| Bounds {
        x0: b.x0.min(p.bounds.x0),
        y0: b.y0.min(p.bounds.y0),
        x1: b.x1.max(p.bounds.x1),
        y1: b.y1.max(p.bounds.y1),
    });
    Some(SourceVector { bounds, paths })
}

/// `vector` with its page-sized fills (half the page or more) taken out: a
/// white page, or a white body box (LibreOffice), painted in the same run as
/// the shapes on it (Qt, Skia) made the whole run as large as the page, so
/// nothing on the page could be told apart from it. A white one draws
/// nothing on a white page and is dropped; any other is a run of its own.
/// O(paths).
fn split_page_fill(vector: SourceVector, width: f64, height: f64) -> Vec<SourceVector> {
    let page_sized = |p: &casual_doc_pdf_read::PathEvidence| {
        p.bounds.width() * p.bounds.height() >= FIGURE_AREA * width * height
    };
    if !vector.paths.iter().any(page_sized) {
        return vec![vector];
    }
    let (fills, rest): (Vec<_>, Vec<_>) = vector.paths.into_iter().partition(page_sized);
    let mut out = Vec::new();
    let coloured: Vec<_> = fills
        .into_iter()
        .filter(|p| p.stroke.is_some() || !matches!(p.fill, Some(Some([255, 255, 255, _]))))
        .collect();
    for paths in [coloured, rest] {
        let Some(first) = paths.first().map(|p| p.bounds) else {
            continue;
        };
        let bounds = paths.iter().fold(first, |b, p| Bounds {
            x0: b.x0.min(p.bounds.x0),
            y0: b.y0.min(p.bounds.y0),
            x1: b.x1.max(p.bounds.x1),
            y1: b.y1.max(p.bounds.y1),
        });
        out.push(SourceVector { bounds, paths });
    }
    out
}

/// The shortest rule kept on its own, as a share of the page's width: a
/// separator line across the text. Shorter lone rules are a form's fields
/// or a footnote separator, and stay out.
const SEPARATOR_WIDTH: f64 = 0.5;

/// The page's rules no table, decoration or furniture band took, as one-path
/// vector art each (a filled bar in the rule's colour): a chart's axes and
/// gridlines, a diagram's connectors, a separator. They were dropped, so a
/// chart reflowed without its frame or gridlines. Rules within 8 pt of a
/// table (a borderless table's rules over and under its header) and in the
/// top or bottom furniture band (a header's or a footer's line) stay out.
/// O(rules x tables).
fn free_rules(
    page: &SourcePage,
    grids: &BTreeSet<usize>,
    decorations: &BTreeSet<usize>,
    tables: &[(f64, f64, f64, f64)],
) -> Vec<SourceVector> {
    let band = FURNITURE_BAND * page.height;
    page.rules
        .iter()
        .enumerate()
        .filter(|(i, _)| !grids.contains(i) && !decorations.contains(i))
        .filter(|(_, r)| r.y0 > band && r.y1 < page.height - band)
        .filter(|(_, r)| {
            !tables.iter().any(|&(x0, y0, x1, y1)| {
                r.x0 >= x0 - 8.0 && r.x1 <= x1 + 8.0 && r.y0 >= y0 - 8.0 && r.y1 <= y1 + 8.0
            })
        })
        .map(|(_, r)| {
            // At least a quarter point thick, so a hairline still draws.
            let (y0, y1) = if r.y1 - r.y0 < 0.25 {
                let c = (r.y0 + r.y1) / 2.0;
                (c - 0.125, c + 0.125)
            } else {
                (r.y0, r.y1)
            };
            let (x0, x1) = if r.x1 - r.x0 < 0.25 {
                let c = (r.x0 + r.x1) / 2.0;
                (c - 0.125, c + 0.125)
            } else {
                (r.x0, r.x1)
            };
            let bounds = Bounds { x0, y0, x1, y1 };
            SourceVector {
                bounds,
                paths: vec![casual_doc_pdf_read::PathEvidence {
                    commands: vec![
                        PathCommand::MoveTo(Point { x: x0, y: y0 }),
                        PathCommand::LineTo(Point { x: x1, y: y0 }),
                        PathCommand::LineTo(Point { x: x1, y: y1 }),
                        PathCommand::LineTo(Point { x: x0, y: y1 }),
                        PathCommand::Close,
                    ],
                    fill: Some(Some(r.color.unwrap_or([0, 0, 0, 255]))),
                    even_odd: false,
                    stroke: None,
                    bounds,
                    clipped: false,
                }],
            }
        })
        .collect()
}

/// A figure, whether it is only rules, and the pictures that are its pieces.
type PicturedFigure = (SourceVector, bool, Vec<SourceImage>);

/// Pictures that overlap a figure join it, and pictures within
/// `FIGURE_GAP` of each other are one; pictures that touch each other with no
/// figure make one of their own (paths empty). A screenshot drawn as nine
/// tiles of shadow around its content and a frame of rules was eleven
/// drawings, each placed on its own line; it is one figure. A picture
/// covering half the page or more (a background) joins nothing. Returns the
/// figures with their pictures, and the pictures left alone.
/// O((figures + pictures)^2) on a page, which holds few.
fn cluster_pictures(
    figures: Vec<(SourceVector, bool)>,
    images: &[SourceImage],
    area: f64,
    work: &mut u64,
) -> (Vec<PicturedFigure>, Vec<SourceImage>) {
    let near = |a: &Bounds, b: &Bounds| {
        a.x0 <= b.x1 + FIGURE_GAP
            && b.x0 <= a.x1 + FIGURE_GAP
            && a.y0 <= b.y1 + FIGURE_GAP
            && b.y0 <= a.y1 + FIGURE_GAP
    };
    // Only a page's own background is no figure's piece: a mockup's card
    // with its screenshot can cover most of a page.
    let big = |b: &Bounds| b.width() * b.height() >= FULL_PAGE * area;
    // Union-find over figures (0..f) and pictures (f..f+p).
    let f = figures.len();
    let total = f + images.len();
    let mut parent: Vec<usize> = (0..total).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let bounds = |i: usize| -> Bounds {
        if i < f {
            figures[i].0.bounds
        } else {
            images[i - f].bounds
        }
    };
    for i in f..total {
        if big(&bounds(i)) {
            continue;
        }
        #[allow(clippy::needless_range_loop)] // `j` spans figures and pictures.
        for j in 0..total {
            *work += 1;
            // A page-sized figure (a page's background) or picture is no
            // figure's piece, and a lone rule is not a frame.
            if j == i || big(&bounds(j)) || figures.get(j).is_some_and(|fig| fig.1) {
                continue;
            }
            // Picture to picture: touching (a screenshot's shadow tiles).
            // Picture to figure: overlapping it (a screenshot inside its
            // frame); a picture merely beside a drawing floats on its own.
            let (bi, bj) = (bounds(i), bounds(j));
            let linked = if j >= f {
                near(&bi, &bj)
            } else {
                bi.x0 < bj.x1 && bj.x0 < bi.x1 && bi.y0 < bj.y1 && bj.y0 < bi.y1
            };
            if linked {
                let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                if a != b {
                    parent[a.max(b)] = a.min(b);
                }
            }
        }
    }
    // Each root's members.
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..total {
        let r = root(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    let mut figures: Vec<Option<(SourceVector, bool)>> = figures.into_iter().map(Some).collect();
    let mut out = Vec::new();
    let mut loose = Vec::new();
    for members in groups.into_values() {
        let pics: Vec<SourceImage> = members
            .iter()
            .filter(|&&i| i >= f)
            .map(|&i| images[i - f].clone())
            .collect();
        let vecs: Vec<usize> = members.iter().copied().filter(|&i| i < f).collect();
        if vecs.is_empty() && pics.len() < 2 {
            loose.extend(pics);
            continue;
        }
        // One figure from its vector runs (paths in paint order) and its
        // pictures' extent.
        let mut paths = Vec::new();
        let mut only_rules = !vecs.is_empty();
        let mut b: Option<Bounds> = None;
        let grow = |b: &mut Option<Bounds>, c: Bounds| {
            *b = Some(match *b {
                Some(o) => Bounds {
                    x0: o.x0.min(c.x0),
                    y0: o.y0.min(c.y0),
                    x1: o.x1.max(c.x1),
                    y1: o.y1.max(c.y1),
                },
                None => c,
            });
        };
        for i in vecs {
            if let Some((v, rules)) = figures[i].take() {
                only_rules &= rules;
                grow(&mut b, v.bounds);
                paths.extend(v.paths);
            }
        }
        for p in &pics {
            grow(&mut b, p.bounds);
        }
        let Some(bounds) = b else {
            continue;
        };
        out.push((
            SourceVector { bounds, paths },
            only_rules && pics.is_empty(),
            pics,
        ));
    }
    (out, loose)
}

/// The text drawn inside `figure`, when it is a figure: every text element
/// inside its bounds (two of the text's ems of slack), not yet taken, whose lines are all
/// labels; they are marked taken. A chart's axis values and legend,
/// reflowed as paragraphs, came out as a column of one-word lines beside a
/// chart with none. Prose over a drawing stays prose (the drawing floats
/// behind it). O(elements).
fn labels(
    figure: &SourceVector,
    area: f64,
    body_size: f64,
    elements: &[Element],
    taken: &mut [bool],
    work: &mut u64,
) -> Vec<ILine> {
    let b = figure.bounds;
    if b.width() < LABELLED_ART.0
        || b.height() < LABELLED_ART.1
        || b.width() * b.height() > FIGURE_AREA * area
        || figure.paths.len() < LABELLED_ART.2
    {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, e) in elements.iter().enumerate() {
        *work += 1;
        // A tagged table's cells are the table's, however short (the
        // structure tree builds it after this; a table's header fill and
        // rules had made it a figure and its cells its labels).
        if taken[i] || !e.floats.is_empty() || e.lineage.iter().any(|n| n.role == "Table") {
            continue;
        }
        let EKind::Text(lines) = &e.kind else {
            continue;
        };
        // An axis value sits just outside the frame it labels (right-aligned
        // against it, so a long value reaches farther): inside the drawing
        // along one axis and within two of its ems of it along the other
        // counts. Labels are set no larger than the body, so a heading
        // just above a chart stays a heading.
        let em = lines.iter().map(|l| l.size).fold(0.5, f64::max);
        let slack = 2.0 * em;
        let r = e.rect;
        let within_x = r.x0 >= b.x0 - slack && r.x1 <= b.x1 + slack;
        let within_y = r.y0 >= b.y0 - slack && r.y1 <= b.y1 + slack;
        let near_x = r.x1 >= b.x0 - slack && r.x0 <= b.x1 + slack;
        let near_y = r.y1 >= b.y0 - slack && r.y0 <= b.y1 + slack;
        // A caption beside the drawing ("Figure 1. ...", "Source: ...")
        // is the figure's paragraph, not one of its labels: short and set
        // small, it sits within two ems as an axis title does.
        let inside = r.x0 >= b.x0 && r.x1 <= b.x1 && r.y0 >= b.y0 && r.y1 <= b.y1;
        let captioned = !inside && lines.first().is_some_and(|line| caption_text(&line.text()));
        if ((within_x && near_y) || (within_y && near_x))
            && em <= body_size + 0.5
            && !captioned
            && lines.iter().all(label_line)
        {
            taken[i] = true;
            out.extend(lines.iter().cloned());
        }
    }
    out
}

/// Whether `text` opens as a caption does: "Figure 1", "Fig. 2a",
/// "Table IV", "Chart 3", "Exhibit 2" (the word, then a number or a roman
/// numeral), or a "Source:" or "Note:" line. O(text).
fn caption_text(text: &str) -> bool {
    let lower = text.trim_start().to_lowercase();
    if ["source:", "note:", "notes:"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return true;
    }
    let numbered = |rest: &str| {
        let number = rest.trim_start_matches([' ', '\u{a0}']);
        let spaced = number.len() < rest.len();
        let roman = number
            .split(|c: char| !c.is_ascii_alphabetic())
            .next()
            .is_some_and(|w| !w.is_empty() && w.chars().all(|c| "ivxlc".contains(c)));
        (spaced || rest.is_empty()) && (number.starts_with(|c: char| c.is_ascii_digit()) || roman)
    };
    [
        "figure", "fig.", "fig", "table", "chart", "graph", "exhibit", "diagram", "plate",
    ]
    .iter()
    .any(|word| lower.strip_prefix(word).is_some_and(numbered))
}

/// A column region and the page it is on.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    /// Page.
    pub page: usize,
    /// The region.
    pub columns: order::ColumnRegion,
}

/// Which pages a header or footer part serves (`w:headerReference@w:type`).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Variant {
    /// Every page (odd pages, when even pages have their own).
    Default,
    /// Even pages (`evenAndOddHeaders`).
    Even,
    /// The first page (`titlePg`).
    First,
}

/// One header or footer part: lines grouped by baseline.
#[derive(Clone, Debug, Default)]
pub struct Part {
    /// Rows of lines sharing a baseline.
    pub rows: Vec<Vec<ILine>>,
    /// Character positions holding the page number: `(row, line, start, end)`.
    pub numbers: Vec<(usize, usize, usize, usize)>,
    /// Tables and pictures repeated in the band (a letterhead's logo, a
    /// form's title block), with the top they sit at (`repeated_objects`).
    pub objects: Vec<(f64, PartObject)>,
}

/// A table or picture that repeats at the top or bottom of the pages.
#[derive(Clone, Debug)]
pub enum PartObject {
    /// A table, the page (zero-based) the kept instance is from, and the
    /// digit runs that change from page to page, `(row, cell, run)`: those
    /// are its page number.
    Table(ITable, usize, BTreeSet<(usize, usize, usize)>),
    /// A picture, drawn where the PDF placed it.
    Image(SourceImage),
    /// A page background repeated on the pages (a letterhead printed as one
    /// picture the size of the page), behind the text on every page.
    Background(SourceImage),
}

/// The header and footer parts, keyed by (is footer, variant).
#[derive(Clone, Debug, Default)]
pub struct Furniture {
    /// The parts.
    pub parts: BTreeMap<(bool, Variant), Part>,
    /// The first page has its own parts.
    pub title_page: bool,
    /// Even pages have their own parts.
    pub even_odd: bool,
}

/// How a repeated band key repeats.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Repeat {
    All,
    OddPages,
    EvenPages,
}

/// A link annotation placed on characters.
#[derive(Clone, Debug)]
pub struct ILink {
    /// Where it goes.
    pub target: LinkTarget,
}

/// A note annotation, to become a comment.
#[derive(Clone, Debug)]
pub struct IComment {
    /// Author.
    pub author: Option<String>,
    /// ISO 8601 date.
    pub date: Option<String>,
    /// Text.
    pub text: String,
    /// The note's object id.
    pub object: Option<(i32, i32)>,
    /// The note it replies to (`/IRT`).
    pub reply_to: Option<(i32, i32)>,
}

/// Work counters per pass (counts, never times: SKILL §8).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReflowWork {
    /// Furniture detection.
    pub furniture: u64,
    /// Table detection (grids, character placement, whitespace candidates).
    pub tables: u64,
    /// Footnote matching.
    pub notes: u64,
    /// Reading order.
    pub order: u64,
    /// Paragraph segmentation and joins.
    pub paragraphs: u64,
    /// Annotation placement.
    pub annotations: u64,
    /// Picture and vector-art placement (text elements weighed per graphic).
    pub graphics: u64,
}

/// A finished reflow.
#[derive(Debug)]
pub struct Reflowed {
    /// The new, validated document.
    pub document: Document,
    /// Its parts (pictures and font programs it references).
    pub resources: BTreeMap<String, Vec<u8>>,
    /// Every inference and loss.
    pub report: PdfImportReport,
    /// Work counters.
    pub work: ReflowWork,
}

/// The document-wide intermediate form the emitter reads.
#[derive(Debug, Default)]
pub(crate) struct Flow {
    /// Elements in reading order across pages.
    pub elements: Vec<Element>,
    /// Column regions.
    pub regions: Vec<Region>,
    /// Footnotes by index.
    pub notes: Vec<notes::INote>,
    /// Links by index.
    pub links: Vec<ILink>,
    /// Comments by index.
    pub comments: Vec<IComment>,
    /// Header and footer.
    pub furniture: Furniture,
    /// The most common page size.
    pub page_size: (f64, f64),
    /// Body text bounds: left, top, right, bottom (the margins).
    pub body: (f64, f64, f64, f64),
    /// Per page, where its body content ends (the lowest text or table
    /// edge), for page-break inference.
    pub page_bottoms: Vec<f64>,
    /// Per page, its text's left and right edges (the column a paragraph on
    /// that page wraps in when the page has no column region). A document
    /// whose pages differ in size (a landscape section) has a different
    /// measure per page, and one document-wide edge made every portrait
    /// line look short (found by `tools/opendoc-reflow-score`).
    pub page_body: Vec<(f64, f64)>,
    /// Per page, its size in points (a landscape page in a portrait
    /// document starts a section of its own).
    pub page_sizes: Vec<(f64, f64)>,
    /// The document's watermark (`watermark`).
    pub watermark: Option<watermark::Found>,
}

/// The font key of a character, through the families the fixed build chose.
pub(crate) fn key_of(state: &PdfSourceState) -> impl Fn(&PChar) -> FontKey + '_ {
    move |c: &PChar| {
        let (family, bold, italic) =
            crate::fixed::run_font(c.style.font, &state.fonts, &state.font_family);
        FontKey {
            family,
            half_points: c.style.half_points,
            bold,
            italic: italic || c.style.oblique,
        }
    }
}

/// Text with every digit run as one `#` (page 9 and page 10 are the same
/// furniture), lowercased, whitespace collapsed.
fn normalized(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii_digit() {
            if !out.ends_with('#') {
                out.push('#');
            }
        } else {
            out.push(c.to_ascii_lowercase());
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Marks page furniture. Returns, per page, the block indices that are
/// furniture or omitted artifacts, and the furniture's content.
/// O(blocks log blocks).
fn furniture(
    state: &PdfSourceState,
    findings: &mut Findings,
    work: &mut u64,
    line_boxes: bool,
) -> (Vec<BTreeSet<usize>>, Furniture) {
    let pages = state.pages.len();
    let mut out = vec![BTreeSet::new(); pages];
    // (band, normalized text, y bucket) -> pages it appears on, with the
    // first occurrence.
    let mut seen: BTreeMap<(bool, String, i64), Vec<(usize, usize)>> = BTreeMap::new();
    let mut artifacts_omitted = 0;
    for page in &state.pages {
        // Text inside a ruled grid is a table's cell: a repeated table is
        // the header's or footer's whole (`repeated_objects`), so its first
        // row must not be taken out on its own as a band of text.
        let mut grid_work = 0;
        let cells: Vec<(f64, f64, f64, f64)> = tables::grids(&page.rules, &mut grid_work)
            .iter()
            .filter_map(|(xs, ys, _)| Some((*xs.first()?, *ys.first()?, *xs.last()?, *ys.last()?)))
            .collect();
        *work += grid_work;
        for (b, block) in page.blocks.iter().enumerate() {
            *work += 1;
            let bb = &block.block;
            let (cx, cy) = ((bb.x0 + bb.x1) / 2.0, (bb.top + bb.bottom) / 2.0);
            if cells
                .iter()
                .any(|&(x0, y0, x1, y1)| cx > x0 && cx < x1 && cy > y0 && cy < y1)
            {
                continue;
            }
            // Text read from a scan stays over the scan, in its line boxes: a
            // running head printed on every scanned page is in the picture.
            if line_boxes && optical_block(bb) {
                continue;
            }
            let top_band = bb.bottom <= FURNITURE_BAND * page.height * 1.5
                && bb.top <= FURNITURE_BAND * page.height;
            let bottom_band = bb.top >= (1.0 - FURNITURE_BAND) * page.height;
            if !(top_band || bottom_band) {
                if block.artifact {
                    out[page.index].insert(b);
                    artifacts_omitted += 1;
                }
                continue;
            }
            let text: String = bb
                .lines
                .iter()
                .map(|l| l.chars.iter().map(|c| c.text.as_str()).collect::<String>())
                .collect::<Vec<_>>()
                .join(" ");
            // Where it sits, measured from the edge of its band: a footer on
            // a landscape page is as far from that page's bottom as on a
            // portrait one, not at the same distance from the top.
            let from_edge = if top_band {
                bb.top
            } else {
                page.height - bb.bottom
            };
            let key = (
                top_band,
                normalized(&text),
                (from_edge / 6.0).round() as i64,
            );
            seen.entry(key).or_default().push((page.index, b));
        }
    }
    findings.record(
        report::ARTIFACT_OMITTED,
        Outcome::Omitted,
        artifacts_omitted,
        None,
    );
    let need = ((pages as f64) * FURNITURE_REPEAT).ceil().max(2.0) as usize;
    // Pages 1, 3, 5... (indices 0, 2, 4...) are odd; a key on only one
    // parity, on at least half of those pages, is an even/odd variant.
    let need_parity = |count: usize| ((count as f64) * FURNITURE_REPEAT).ceil().max(2.0) as usize;
    let (odd_pages, even_pages) = (pages.div_ceil(2), pages / 2);
    let mut content = Furniture::default();
    let mut inferred = 0;
    // Per band: (first baseline, left edge, lines, carries the page number).
    type Row = (f64, f64, Vec<ILine>, bool);
    let mut keyed: Vec<(bool, Repeat, BTreeSet<usize>, Row)> = Vec::new();
    for ((top_band, _, _), occurrences) in &seen {
        let artifact = occurrences
            .iter()
            .any(|(p, b)| state.pages[*p].blocks[*b].artifact);
        let distinct: BTreeSet<usize> = occurrences.iter().map(|(p, _)| *p).collect();
        let repeat = if pages >= 4
            && distinct.iter().all(|p| p % 2 == 0)
            && distinct.len() >= need_parity(odd_pages)
        {
            Repeat::OddPages
        } else if pages >= 4
            && distinct.iter().all(|p| p % 2 == 1)
            && distinct.len() >= need_parity(even_pages)
        {
            Repeat::EvenPages
        } else if artifact || distinct.len() >= need {
            Repeat::All
        } else {
            continue;
        };
        for (p, b) in occurrences {
            out[*p].insert(*b);
        }
        inferred += 1;
        let (p, b) = occurrences[0];
        let block = &state.pages[p].blocks[b];
        let lines: Vec<ILine> = block
            .block
            .lines
            .iter()
            .map(|l| ILine::new(l, p, block.mcid))
            .collect();
        // The page number: a digit run whose value minus the page index is
        // the same constant (one) on every occurrence.
        let numbered = distinct.len() >= 2
            && occurrences.iter().all(|(op, ob)| {
                let text: String = state.pages[*op].blocks[*ob]
                    .block
                    .lines
                    .iter()
                    .flat_map(|l| l.chars.iter().map(|c| c.text.clone()))
                    .collect();
                text.split(|c: char| !c.is_ascii_digit())
                    .filter(|t| !t.is_empty())
                    .any(|t| t.parse::<usize>().ok() == Some(op + 1))
            });
        let first = lines.first().map_or(0.0, |l| l.baseline);
        let x = block.block.x0;
        keyed.push((!*top_band, repeat, distinct, (first, x, lines, numbered)));
    }
    content.even_odd = keyed.iter().any(|k| k.1 == Repeat::EvenPages);
    // The first page is different when a band's every-page content skips it
    // (a title page) on a document long enough to tell.
    content.title_page = pages >= 3
        && [false, true].iter().any(|footer| {
            let band: Vec<_> = keyed
                .iter()
                .filter(|k| k.0 == *footer && k.1 != Repeat::EvenPages)
                .collect();
            !band.is_empty() && band.iter().all(|k| !k.2.contains(&0))
        });
    let mut rows: BTreeMap<(bool, Variant), Vec<Row>> = BTreeMap::new();
    for (footer, repeat, distinct, row) in keyed {
        let mut variants = Vec::new();
        match repeat {
            Repeat::All => {
                variants.push(Variant::Default);
                if content.even_odd {
                    variants.push(Variant::Even);
                }
            }
            Repeat::OddPages => variants.push(Variant::Default),
            Repeat::EvenPages => variants.push(Variant::Even),
        }
        if content.title_page && distinct.contains(&0) {
            variants.push(Variant::First);
        }
        for variant in variants {
            rows.entry((footer, variant)).or_default().push(row.clone());
        }
    }
    if content.title_page {
        for footer in [false, true] {
            if rows.contains_key(&(footer, Variant::Default)) {
                rows.entry((footer, Variant::First)).or_default();
            }
        }
    }
    findings.record(
        report::HEADER_FOOTER_INFERRED,
        Outcome::Mapped,
        inferred,
        None,
    );
    for (key, list) in &mut rows {
        list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut grouped: Vec<Vec<ILine>> = Vec::new();
        let mut numbers: Vec<(usize, usize, usize, usize)> = Vec::new();
        let mut last_baseline = f64::NEG_INFINITY;
        for (baseline, _, lines, numbered) in list.drain(..) {
            for line in lines {
                if (line.baseline - last_baseline).abs() > 2.0 || grouped.is_empty() {
                    grouped.push(Vec::new());
                }
                last_baseline = line.baseline;
                let row = grouped.len() - 1;
                let index = grouped[row].len();
                if numbered {
                    // The page number on the first occurrence (page `p`) is the
                    // digit run equal to p + 1; the emitter replaces it with a
                    // PAGE field.
                    let page = line.page + 1;
                    let mut i = 0;
                    while i < line.chars.len() {
                        if line.chars[i].text.chars().all(|c| c.is_ascii_digit())
                            && !line.chars[i].text.is_empty()
                        {
                            let start = i;
                            let mut digits = String::new();
                            while i < line.chars.len()
                                && line.chars[i].text.chars().all(|c| c.is_ascii_digit())
                                && !line.chars[i].text.is_empty()
                            {
                                digits.push_str(&line.chars[i].text);
                                i += 1;
                            }
                            if digits.parse::<usize>().ok() == Some(page) {
                                numbers.push((row, index, start, i));
                                break;
                            }
                        } else {
                            i += 1;
                        }
                    }
                }
                let _ = baseline;
                grouped[row].push(line);
            }
        }
        content.parts.insert(
            *key,
            Part {
                rows: grouped,
                numbers,
                objects: Vec::new(),
            },
        );
    }
    (out, content)
}

/// Tags characters covered by a link rectangle (centre inside). O(chars x
/// links on the page); links are few per page.
fn tag_links(lines: &mut [ILine], links: &[(usize, Bounds)]) {
    if links.is_empty() {
        return;
    }
    for line in lines {
        for c in &mut line.chars {
            let (cx, cy) = ((c.x0 + c.x1) / 2.0, c.baseline - c.size * 0.3);
            if let Some((index, _)) = links
                .iter()
                .find(|(_, r)| cx >= r.x0 && cx <= r.x1 && cy >= r.y0 - 1.0 && cy <= r.y1 + 1.0)
            {
                c.link = Some(*index);
            }
        }
    }
}

/// How far (in ems) a marker may sit from the text it labels.
pub const MARKER_REACH: f64 = 4.0;

/// A list marker set apart from its text by more than pdfminer's character
/// margin (an indent tab, the usual case) is grouped as a line of its own,
/// often stacked with the other markers into a block. Each such marker-only
/// line is put back at the start of the line it labels: the nearest line to
/// its right on the same baseline within [`MARKER_REACH`] ems. Returns how
/// many were attached. O(l log l) in lines.
fn attach_markers(elements: &mut [Vec<ILine>], work: &mut u64) -> u32 {
    let mut all: Vec<(f64, usize, usize)> = Vec::new();
    for (e, lines) in elements.iter().enumerate() {
        for (l, line) in lines.iter().enumerate() {
            all.push((line.baseline, e, l));
        }
    }
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    *work += all.len() as u64;
    let mut moves: Vec<((usize, usize), (usize, usize))> = Vec::new();
    let mut targets = BTreeSet::new();
    for &(baseline, e, l) in &all {
        let marker = &elements[e][l];
        if !text::marker_token(&marker.text()) {
            continue;
        }
        let from = all.partition_point(|a| a.0 < baseline - 1.0);
        let reach = MARKER_REACH * marker.size.max(1.0);
        let best = all[from..]
            .iter()
            .inspect(|_| *work += 1)
            .take_while(|a| a.0 <= baseline + 1.0)
            .filter(|a| {
                a.0 >= baseline - 1.0 && (a.1, a.2) != (e, l) && !targets.contains(&(a.1, a.2))
            })
            .map(|a| (elements[a.1][a.2].x0 - marker.x1, a.1, a.2))
            .filter(|(gap, ..)| *gap >= 0.0 && *gap <= reach)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, te, tl)) = best {
            targets.insert((te, tl));
            moves.push(((e, l), (te, tl)));
        }
    }
    let attached = moves.len() as u32;
    let mut removed: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for ((e, l), (te, tl)) in moves {
        let marker = elements[e][l].clone();
        let target = &mut elements[te][tl];
        let mut chars = marker.chars.clone();
        if let Some(last) = marker.chars.last() {
            let mut space = last.clone();
            space.text = " ".to_owned();
            space.x0 = last.x1;
            space.x1 = target.x0;
            chars.push(space);
        }
        chars.append(&mut target.chars);
        target.chars = chars;
        target.x0 = marker.x0;
        target.top = target.top.min(marker.top);
        target.bottom = target.bottom.max(marker.bottom);
        removed.entry(e).or_default().push(l);
    }
    for (e, mut ls) in removed {
        ls.sort_unstable();
        for l in ls.into_iter().rev() {
            elements[e].remove(l);
        }
    }
    attached
}

/// What becomes of text OCR read out of a picture (`docs/182` §20b, the
/// owner's decision of 2026-11-01).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScanText {
    /// The picture stays where it was, and each line read from it is an
    /// editable line box over its ink, its text laid out but not painted
    /// (`RunProperties::no_text_fill`): the page shows the picture's own
    /// pixels, a misread shows only once that line is edited, and editing a
    /// line erases the ink under it (`casual_doc_render::image_text`). A
    /// scanned page is its picture on a page of its own, its lines over it.
    /// Reported `pdf.import.image_text_boxes`.
    #[default]
    LineBoxes,
    /// The text reflows as the document's own paragraphs and a scanned page's
    /// picture is left to the exact layout (`pdf.import.scan_replaced_by_text`):
    /// the behaviour before 2026-11-01, kept as an opt-in.
    Reflow,
}

/// How [`build_with`] reconstructs. `Default` is what every host opens.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReflowOptions {
    /// What becomes of text read from pictures.
    pub scan_text: ScanText,
}

/// Reflows the import with the default [`ReflowOptions`]. Pure: reads
/// `state` and `resources`, returns a new document. O(document log document).
///
/// # Errors
///
/// [`PdfImportError::Model`] if the built document fails validation (an
/// importer defect, never a property of the input).
pub fn build(
    state: &PdfSourceState,
    resources: &BTreeMap<String, Vec<u8>>,
    namespace: u64,
) -> Result<Reflowed, PdfImportError> {
    build_with(state, resources, namespace, ReflowOptions::default())
}

/// [`build`] with explicit options. O(document log document).
///
/// # Errors
///
/// As [`build`].
pub fn build_with(
    state: &PdfSourceState,
    resources: &BTreeMap<String, Vec<u8>>,
    namespace: u64,
    options: ReflowOptions,
) -> Result<Reflowed, PdfImportError> {
    let line_boxes = options.scan_text == ScanText::LineBoxes;
    let mut findings = Findings::default();
    let mut work = ReflowWork::default();
    // The watermark leaves the pages first: its stamps are no text and its
    // picture no background.
    let (state, found) = watermark::extract(state, &mut findings);
    let state: &PdfSourceState = &state;
    let key = key_of(state);
    let (furniture_blocks, mut furniture) =
        furniture(state, &mut findings, &mut work.furniture, line_boxes);

    // Structure leaves by (page, mcid), when tagged.
    let mut leaves: BTreeMap<(usize, i32), usize> = BTreeMap::new();
    if let Some(tree) = &state.structure {
        for (i, leaf) in tree.leaves.iter().enumerate() {
            leaves.entry((leaf.page, leaf.mcid)).or_insert(i);
        }
        if tree.truncated {
            findings.record(report::TAG_STRUCTURE_IGNORED, Outcome::Degraded, 1, None);
        }
    }

    // The body size, for footnote detection, from every body character.
    let mut body_sizes: BTreeMap<u32, usize> = BTreeMap::new();
    for page in &state.pages {
        for (b, block) in page.blocks.iter().enumerate() {
            if furniture_blocks[page.index].contains(&b) {
                continue;
            }
            for line in &block.block.lines {
                for c in &line.chars {
                    if !c.text.trim().is_empty() {
                        *body_sizes.entry(c.style.half_points).or_default() += 1;
                    }
                }
            }
        }
    }
    let body_size = body_sizes
        .iter()
        .max_by_key(|(_, n)| **n)
        .map_or(10.0, |(h, _)| f64::from(*h) / 2.0);

    let mut flow = Flow::default();
    let mut sizes: BTreeMap<(i64, i64), usize> = BTreeMap::new();
    let (mut bx0, mut by0, mut bx1, mut by1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    let mut tagged_pages = 0;
    let mut xy_pages = 0;
    // The most common page size, for the document's margins.
    let common_size = {
        let mut counts: BTreeMap<(i64, i64), usize> = BTreeMap::new();
        for page in &state.pages {
            *counts
                .entry((page.width.round() as i64, page.height.round() as i64))
                .or_default() += 1;
        }
        counts
            .into_iter()
            .max_by_key(|(_, n)| *n)
            .map_or((612.0, 792.0), |((w, h), _)| (w as f64, h as f64))
    };
    let mut masks = 0;
    let mut scans_replaced = 0;
    let mut image_lines = 0;
    let mut vectors_placed = 0;
    let mut floats = 0;
    let mut rotated = 0;
    for page in &state.pages {
        *sizes
            .entry((page.width.round() as i64, page.height.round() as i64))
            .or_default() += 1;
        masks += page.masks;
        rotated += page.rotated_runs;
        // Links and notes on this page.
        let mut page_links: Vec<(usize, Bounds)> = Vec::new();
        let mut page_comments: Vec<(usize, Bounds)> = Vec::new();
        for annotation in &page.annotations {
            work.annotations += 1;
            classify(
                annotation,
                &mut flow,
                &mut page_links,
                &mut page_comments,
                &mut findings,
                page.index,
            );
        }
        let mut texts: Vec<(Vec<ILine>, Option<i32>, bool)> = Vec::new();
        for (b, block) in page.blocks.iter().enumerate() {
            if furniture_blocks[page.index].contains(&b) {
                continue;
            }
            let mut lines: Vec<ILine> = block
                .block
                .lines
                .iter()
                .map(|l| ILine::new(l, page.index, block.mcid))
                .collect();
            tag_links(&mut lines, &page_links);
            texts.push((lines, block.mcid, block.artifact));
        }
        // Ruled tables take their lines out of the blocks.
        let mut lines_only: Vec<Vec<ILine>> =
            texts.iter_mut().map(|t| std::mem::take(&mut t.0)).collect();
        attach_markers(&mut lines_only, &mut work.paragraphs);
        let mut page_tables: Vec<ITable> = Vec::new();
        let grids = tables::grids(&page.rules, &mut work.tables);
        for mut table in tables::ruled_tables(&grids, &mut lines_only, &mut work.tables) {
            if table.rows.iter().flatten().any(|c| !c.lines.is_empty()) {
                // Filled rectangles under its cells are their shading.
                tables::shade(&mut table, &page.fills, &mut work.tables);
                page_tables.push(table);
            }
        }

        // Footnotes leave the body; a note the previous page left open
        // continues in this page's note area.
        if flow
            .notes
            .last()
            .is_some_and(|n| n.page + 1 == page.index && notes::open(n))
        {
            let more =
                notes::continuation(&mut lines_only, page.height, body_size, &mut work.notes);
            if !more.is_empty() {
                findings.record(
                    report::FOOTNOTE_INFERRED,
                    Outcome::Mapped,
                    1,
                    Some(page.index),
                );
                if let Some(note) = flow.notes.last_mut() {
                    note.lines.extend(more);
                }
            }
        }
        let first_note = flow.notes.len();
        let notes = notes::footnotes(
            &mut lines_only,
            page.index,
            page.height,
            body_size,
            first_note,
            &mut work.notes,
        );
        findings.record(
            report::FOOTNOTE_INFERRED,
            Outcome::Mapped,
            notes.len() as u32,
            Some(page.index),
        );
        flow.notes.extend(notes);
        // Whitespace tables among what is left.
        let bounds_of = |lines: &[ILine]| -> (f64, f64) {
            (
                lines.iter().map(|l| l.x0).fold(f64::INFINITY, f64::min),
                lines.iter().map(|l| l.x1).fold(f64::NEG_INFINITY, f64::max),
            )
        };
        // A header band's cell spans the columns under it, not its words.
        let mut spans_over: BTreeMap<usize, (f64, f64)> = BTreeMap::new();
        let candidates = {
            let views: Vec<(&[ILine], (f64, f64))> = lines_only
                .iter()
                .map(|l| (l.as_slice(), bounds_of(l)))
                .collect();
            let mut found = tables::whitespace_candidates(&views, &mut work.tables);
            // A two-row candidate under a shaded header band spanning its
            // columns is a table: the band is its merged header row.
            for candidate in &mut found {
                if candidate.rows == 2
                    && let Some((header, extent)) = tables::shaded_header(
                        &candidate.elements,
                        &views,
                        &page.fills,
                        &mut work.tables,
                    )
                {
                    candidate.elements.insert(0, header);
                    spans_over.insert(header, extent);
                    candidate.rows = 3;
                    candidate.confidence *= 1.5;
                }
            }
            found
        };
        for candidate in candidates {
            // Cells a tagged `Table` owns are its; the tree decides them.
            if candidate.elements.iter().any(|&e| {
                texts[e]
                    .1
                    .and_then(|m| leaves.get(&(page.index, m)))
                    .and_then(|&i| state.structure.as_ref().map(|t| &t.leaves[i]))
                    .is_some_and(|leaf| nearest(&leaf.lineage, &["Table"]).is_some())
            }) {
                continue;
            }
            if candidate.confidence >= tables::TABLE_CONFIDENCE && candidate.rows >= 3 {
                // Lines that are no row of it go back as text of their own.
                for (e, out) in &candidate.strays {
                    let mut kept = Vec::new();
                    let mut lead = Vec::new();
                    let mut trail = Vec::new();
                    for (l, line) in std::mem::take(&mut lines_only[*e]).into_iter().enumerate() {
                        if !out.contains(&l) {
                            kept.push(line);
                        } else if kept.is_empty() {
                            lead.push(line);
                        } else {
                            trail.push(line);
                        }
                    }
                    lines_only[*e] = kept;
                    for piece in [lead, trail] {
                        if !piece.is_empty() {
                            texts.push((Vec::new(), texts[*e].1, texts[*e].2));
                            lines_only.push(piece);
                        }
                    }
                }
                let columns: Vec<(Vec<ILine>, (f64, f64))> = candidate
                    .elements
                    .iter()
                    .map(|&e| {
                        let lines = std::mem::take(&mut lines_only[e]);
                        let b = spans_over
                            .get(&e)
                            .copied()
                            .unwrap_or_else(|| bounds_of(&lines));
                        (lines, b)
                    })
                    .collect();
                page_tables.push(tables::whitespace_table(&columns, &candidate.tall_rows));
            } else if candidate.confidence >= tables::TABLE_CANDIDATE {
                findings.record(
                    report::TABLE_TOPOLOGY_DEGRADED,
                    Outcome::Degraded,
                    1,
                    Some(page.index),
                );
            }
        }
        findings.record(
            report::TABLE_INFERRED,
            Outcome::Mapped,
            page_tables.len() as u32,
            Some(page.index),
        );
        // Filled rectangles under a whitespace table's cells are its shading
        // too (a borderless table with a shaded header row).
        for table in page_tables.iter_mut().filter(|t| !t.ruled) {
            tables::fit_to_fills(table, &page.fills, &mut work.tables);
            tables::shade(table, &page.fills, &mut work.tables);
        }
        // Each cell's alignment, and a borderless table's grid from it.
        for table in &mut page_tables {
            let columns = tables::align_cells(table, &page.fills);
            tables::align_grid(table, &columns);
        }
        // Underlines, strikethroughs, highlights, paragraph shading and
        // borders the PDF drew as paths go back on the text.
        // The rules of the grids that became tables (a grid with no text in
        // it, a chart's frame and gridlines, is no table: its rules are art).
        let ruled: Vec<(f64, f64, f64, f64)> = page_tables
            .iter()
            .filter(|t| t.ruled)
            .map(|t| t.bounds)
            .collect();
        let taken_rules: BTreeSet<usize> = page
            .rules
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                grids
                    .iter()
                    .flat_map(|g| g.2.iter())
                    .any(|g| g.x0 == r.x0 && g.x1 == r.x1 && g.y0 == r.y0 && g.y1 == r.y1)
                    && ruled.iter().any(|&(x0, y0, x1, y1)| {
                        r.x0 >= x0 - 1.0 && r.x1 <= x1 + 1.0 && r.y0 >= y0 - 1.0 && r.y1 <= y1 + 1.0
                    })
            })
            .map(|(i, _)| i)
            .collect();
        let table_bounds: Vec<(f64, f64, f64, f64)> =
            page_tables.iter().map(|t| t.bounds).collect();
        let decorated = decorate::decorate(
            &mut lines_only,
            &page.rules,
            &taken_rules,
            &page.fills,
            &table_bounds,
            page.width * page.height,
            &mut work.paragraphs,
        );
        findings.record(
            report::DECORATION_INFERRED,
            Outcome::Mapped,
            (decorated.rules.len() + decorated.fills.len()) as u32,
            Some(page.index),
        );
        let decoration_art: BTreeSet<usize> = page
            .vectors
            .iter()
            .enumerate()
            .filter(|(_, v)| {
                v.paths.iter().all(|path| {
                    path.stroke.is_none()
                        && record::rectangles(path).is_some_and(|rects| {
                            rects.iter().all(|b| {
                                decorated.fills.iter().any(|&f| {
                                    let d = page.fills[f].bounds;
                                    (d.x0 - b.x0).abs() < 0.01
                                        && (d.y0 - b.y0).abs() < 0.01
                                        && (d.x1 - b.x1).abs() < 0.01
                                        && (d.y1 - b.y1).abs() < 0.01
                                })
                            })
                        })
                })
            })
            .map(|(i, _)| i)
            .collect();
        // Vector art that is only flat rectangles inside a table is that
        // table's shading (now on its cells), not a drawing of its own.
        let shading_art: BTreeSet<usize> = page
            .vectors
            .iter()
            .enumerate()
            .filter(|(_, v)| tables::is_shading(v, &page_tables))
            .map(|(i, _)| i)
            .collect();

        // A reply anchors where its root does (replies often carry no
        // rectangle of their own).
        let roots: Vec<((i32, i32), Bounds)> = page_comments
            .iter()
            .filter_map(|(c, r)| flow.comments[*c].object.map(|o| (o, *r)))
            .collect();
        for (comment, rect) in &mut page_comments {
            if let Some(parent) = flow.comments[*comment].reply_to
                && let Some((_, root_rect)) = roots.iter().find(|(o, _)| *o == parent)
            {
                *rect = *root_rect;
            }
        }
        // Comments anchor on the line nearest each note.
        for (comment, rect) in &page_comments {
            let centre = (rect.y0 + rect.y1) / 2.0;
            let best = lines_only
                .iter_mut()
                .flat_map(|l| l.iter_mut())
                .min_by(|a, b| {
                    let da = ((a.top + a.bottom) / 2.0 - centre).abs()
                        + (a.x0 - rect.x0).max(0.0) * 0.01;
                    let db = ((b.top + b.bottom) / 2.0 - centre).abs()
                        + (b.x0 - rect.x0).max(0.0) * 0.01;
                    da.total_cmp(&db)
                });
            if let Some(line) = best {
                for c in &mut line.chars {
                    c.comments.push(*comment);
                }
                findings.record(
                    report::COMMENT_INFERRED,
                    Outcome::Mapped,
                    1,
                    Some(page.index),
                );
            } else {
                flow.comments[*comment].text.clear();
                findings.record(
                    report::ANNOTATION_OMITTED,
                    Outcome::Omitted,
                    1,
                    Some(page.index),
                );
            }
        }

        // The page's elements.
        let mut elements: Vec<Element> = Vec::new();
        for (lines, (_, mcid, _)) in lines_only.into_iter().zip(&texts) {
            if lines.is_empty() {
                continue;
            }
            let rect = Rect {
                x0: lines.iter().map(|l| l.x0).fold(f64::INFINITY, f64::min),
                y0: lines.iter().map(|l| l.top).fold(f64::INFINITY, f64::min),
                x1: lines.iter().map(|l| l.x1).fold(f64::NEG_INFINITY, f64::max),
                y1: lines
                    .iter()
                    .map(|l| l.bottom)
                    .fold(f64::NEG_INFINITY, f64::max),
            };
            let leaf = mcid.and_then(|m| leaves.get(&(page.index, m)).copied());
            let node = leaf.and_then(|i| state.structure.as_ref().map(|t| &t.leaves[i]));
            elements.push(Element {
                page: page.index,
                rect,
                kind: EKind::Text(lines),
                leaf,
                struct_element: node.map(|n| n.element()),
                role: node.map(|n| n.role().to_owned()),
                region: None,
                lineage: node.map(|n| n.lineage.clone()).unwrap_or_default(),
                floats: Vec::new(),
                labels: Vec::new(),
                pictures: Vec::new(),
            });
        }
        for table in page_tables {
            let (x0, y0, x1, y1) = table.bounds;
            let leaf = table
                .rows
                .iter()
                .flatten()
                .flat_map(|c| c.lines.iter())
                .filter_map(|l| l.mcid.and_then(|m| leaves.get(&(page.index, m)).copied()))
                .min();
            elements.push(Element {
                page: page.index,
                rect: Rect { x0, y0, x1, y1 },
                kind: EKind::Table(table),
                leaf,
                struct_element: None,
                role: None,
                region: None,
                lineage: Vec::new(),
                floats: Vec::new(),
                labels: Vec::new(),
                pictures: Vec::new(),
            });
        }
        // Pictures and vector art: floated on the text they sit beside or
        // under, else on their own line in reading order.
        // A figure's pieces (a chart's frame, bars and legend keys, painted
        // between its labels) are one drawing, and the text inside it is its
        // labels, drawn with it on its own line.
        let mut pieces: Vec<(SourceVector, bool)> = page
            .vectors
            .iter()
            .enumerate()
            .filter(|(i, _)| !shading_art.contains(i) && !decoration_art.contains(i))
            .filter(|(_, v)| !invisible(v))
            .filter_map(|(_, v)| on_page(v, page.width, page.height))
            .flat_map(|v| split_page_fill(v, page.width, page.height))
            .map(|v| (v, false))
            .collect();
        pieces.extend(
            free_rules(page, &taken_rules, &decorated.rules, &table_bounds)
                .into_iter()
                .map(|v| (v, true)),
        );
        let figures = merge_figures(pieces, page.width * page.height, &mut work.graphics);
        // Pictures that touch a figure or each other are its pieces.
        let (figures, loose) = cluster_pictures(
            figures,
            &page.images,
            page.width * page.height,
            &mut work.graphics,
        );
        let mut labelled: Vec<(SourceVector, Vec<ILine>, Vec<SourceImage>)> = Vec::new();
        let mut taken = vec![false; elements.len()];
        let mut unlabelled: Vec<SourceVector> = Vec::new();
        for (figure, only_rules, pictures) in figures {
            // Rules on their own: a long one is a separator line in the
            // text (kept as a drawing); short ones (a form's fields, a
            // footnote's separator) are left out, as before.
            if only_rules {
                if figure.paths.len() == 1 && figure.bounds.width() >= SEPARATOR_WIDTH * page.width
                {
                    unlabelled.push(figure);
                }
                continue;
            }
            let lines = labels(
                &figure,
                page.width * page.height,
                body_size,
                &elements,
                &mut taken,
                &mut work.graphics,
            );
            // A figure with pictures in it is drawn whole on its own line.
            if lines.is_empty() && pictures.is_empty() {
                unlabelled.push(figure);
            } else {
                labelled.push((figure, lines, pictures));
            }
        }
        // A picture with text OCR read from it keeps that text over it, one
        // line box per line (`ScanText::LineBoxes`): before the floats are
        // placed, so no float hangs on a line that leaves the flow.
        let mut loose = loose;
        let mut image_texts: Vec<(SourceImage, Vec<ILine>)> = Vec::new();
        if line_boxes {
            let mut rest = Vec::with_capacity(loose.len());
            for image in loose {
                work.graphics += elements.len() as u64;
                let lines = claim_optical_lines(&image, &elements, &mut taken);
                if lines.is_empty() {
                    rest.push(image);
                } else {
                    image_texts.push((image, lines));
                }
            }
            loose = rest;
        }
        let text_rects: Vec<(usize, Rect)> = elements
            .iter()
            .enumerate()
            .filter(|(i, e)| matches!(e.kind, EKind::Text(_)) && !taken[*i])
            .map(|(i, e)| (i, e.rect))
            .collect();
        for (image, lines) in image_texts {
            image_lines += lines.len() as u32;
            let b = image.bounds;
            elements.push(Element {
                page: page.index,
                rect: Rect {
                    x0: b.x0,
                    y0: b.y0,
                    x1: b.x1,
                    y1: b.y1,
                },
                kind: EKind::Image(image),
                leaf: None,
                struct_element: None,
                role: None,
                region: None,
                lineage: Vec::new(),
                floats: Vec::new(),
                labels: lines,
                pictures: Vec::new(),
            });
        }
        for (figure, lines, pictures) in labelled {
            let b = figure.bounds;
            vectors_placed += 1;
            elements.push(Element {
                page: page.index,
                rect: Rect {
                    x0: b.x0,
                    y0: b.y0,
                    x1: b.x1,
                    y1: b.y1,
                },
                kind: EKind::Vector(figure),
                leaf: None,
                struct_element: None,
                role: None,
                region: None,
                lineage: Vec::new(),
                floats: Vec::new(),
                labels: lines,
                pictures,
            });
        }
        let graphics = loose
            .into_iter()
            .map(Graphic::Image)
            .chain(unlabelled.into_iter().map(Graphic::Vector));
        for graphic in graphics {
            let b = graphic.bounds();
            let rect = Rect {
                x0: b.x0,
                y0: b.y0,
                x1: b.x1,
                y1: b.y1,
            };
            let vector = matches!(graphic, Graphic::Vector(_));
            vectors_placed += u32::from(vector);
            work.graphics += text_rects.len() as u64;
            // A picture covering much of the page with text over it is the
            // page's background: before this it was an inline picture a page
            // tall, and the text it was behind came after it.
            if !vector && b.width() * b.height() >= BACKGROUND_SHARE * page.width * page.height {
                let over = text_rects
                    .iter()
                    .find(|(_, t)| {
                        t.x0 < rect.x1 && t.x1 > rect.x0 && t.y0 < rect.y1 && t.y1 > rect.y0
                    })
                    .map(|(i, _)| *i);
                // A scan whose text is the recogniser's: the flowing document
                // is that text, and the scan behind it (its words at their
                // old places, now under different lines) is left to the exact
                // layout.
                let optical = text_rects
                    .iter()
                    .filter(|(_, t)| {
                        t.x0 < rect.x1 && t.x1 > rect.x0 && t.y0 < rect.y1 && t.y1 > rect.y0
                    })
                    .all(|(i, _)| match &elements[*i].kind {
                        EKind::Text(lines) => lines
                            .iter()
                            .flat_map(|l| l.chars.iter())
                            .all(|c| c.style.font == crate::ocr::OCR_FONT),
                        _ => false,
                    });
                if over.is_some() && optical && !line_boxes {
                    scans_replaced += 1;
                    continue;
                }
                if let Some(anchor) = over {
                    floats += 1;
                    elements[anchor].floats.push(Float {
                        page: page.index,
                        graphic,
                        wrap: FloatWrap::Background,
                    });
                    continue;
                }
            }
            if let Some((anchor, wrap)) = placement(&rect, vector, &text_rects) {
                floats += 1;
                elements[anchor].floats.push(Float {
                    page: page.index,
                    graphic,
                    wrap,
                });
                continue;
            }
            elements.push(Element {
                page: page.index,
                rect,
                kind: match graphic {
                    Graphic::Image(image) => EKind::Image(image),
                    Graphic::Vector(vector) => EKind::Vector(vector),
                },
                leaf: None,
                struct_element: None,
                role: None,
                region: None,
                lineage: Vec::new(),
                floats: Vec::new(),
                labels: Vec::new(),
                pictures: Vec::new(),
            });
        }
        // The labels left their text elements for their drawings.
        let mut index = 0;
        elements.retain(|_| {
            let keep = taken.get(index).is_none_or(|t| !t);
            index += 1;
            keep
        });
        let mut page_bottom = f64::NEG_INFINITY;
        let (mut px0, mut px1) = (f64::INFINITY, f64::NEG_INFINITY);
        for e in &elements {
            // Where the page's content stops: a picture or a figure at the
            // bottom of the page is content (a chart closing a page had read
            // as a page that ended early, so the next page started with a
            // break and the reflow left a page nearly empty).
            if !matches!(e.kind, EKind::Text(_) | EKind::Table(_)) {
                page_bottom = page_bottom.max(e.rect.y1);
            }
            if matches!(e.kind, EKind::Text(_) | EKind::Table(_)) {
                page_bottom = page_bottom.max(e.rect.y1);
                px0 = px0.min(e.rect.x0);
                px1 = px1.max(e.rect.x1);
                // The document's margins are its common page size's.
                if (page.width.round(), page.height.round()) == common_size {
                    bx0 = bx0.min(e.rect.x0);
                    by0 = by0.min(e.rect.y0);
                    bx1 = bx1.max(e.rect.x1);
                    by1 = by1.max(e.rect.y1);
                }
            }
        }
        flow.page_body.push((px0, px1));
        flow.page_sizes.push((page.width, page.height));

        flow.page_bottoms.push(page_bottom);
        // Reading order: XY-Cut++ always (it also finds the column regions);
        // the structure tree overrides the order when it covers the page.
        let rects: Vec<Rect> = elements.iter().map(|e| e.rect).collect();
        let ordered = order::xy_cut_pp(&rects, &mut work.order);
        let base = flow.regions.len();
        flow.regions.extend(ordered.regions.iter().map(|c| Region {
            page: page.index,
            columns: *c,
        }));
        for (i, e) in elements.iter_mut().enumerate() {
            e.region = ordered.region[i].map(|r| base + r);
        }
        let texts_total = elements
            .iter()
            .filter(|e| matches!(e.kind, EKind::Text(_)))
            .count();
        let tagged = elements.iter().filter(|e| e.leaf.is_some()).count();
        let use_tags = texts_total > 0 && tagged * 5 >= texts_total * 4;
        let sequence: Vec<usize> = if use_tags {
            tagged_pages += 1;
            // Untagged elements keep their XY position relative to the
            // tagged element before them.
            let mut keyed: Vec<(usize, usize, usize)> = Vec::with_capacity(elements.len());
            let mut previous = 0;
            for (rank, &i) in ordered.order.iter().enumerate() {
                let key = match elements[i].leaf {
                    Some(leaf) => {
                        previous = leaf;
                        (leaf, 0)
                    }
                    None => (previous, 1 + rank),
                };
                keyed.push((key.0, key.1, i));
            }
            keyed.sort_unstable();
            keyed.into_iter().map(|k| k.2).collect()
        } else {
            if elements.len() > 1 {
                xy_pages += 1;
            }
            ordered.order.clone()
        };
        let mut slots: Vec<Option<Element>> = elements.into_iter().map(Some).collect();
        let mut tagged_tables = 0;
        let mut pending: Vec<Element> = Vec::new();
        let mut pending_table: Option<u32> = None;
        for i in sequence {
            let Some(e) = slots[i].take() else {
                continue;
            };
            // Elements under one tagged `Table` become one table.
            let table = if use_tags && matches!(e.kind, EKind::Text(_)) {
                nearest(&e.lineage, &["Table"])
            } else {
                None
            };
            if pending_table.is_some() && table != pending_table {
                flow.elements
                    .push(tagged_table(std::mem::take(&mut pending)));
                tagged_tables += 1;
            }
            pending_table = table;
            match table {
                Some(_) => pending.push(e),
                None => flow.elements.push(e),
            }
        }
        if !pending.is_empty() {
            flow.elements.push(tagged_table(pending));
            tagged_tables += 1;
        }
        findings.record(
            report::TABLE_INFERRED,
            Outcome::Mapped,
            tagged_tables,
            Some(page.index),
        );
    }
    // A table the page break cut in two is one table (its repeated header
    // row, when the next page repeats it, becomes a header row).
    let continued = tables::continue_across_pages(&mut flow.elements, &mut work.tables);
    findings.record(report::TABLE_CONTINUED, Outcome::Mapped, continued, None);
    // Tables and pictures repeated at the top or bottom of the pages are
    // the header's and the footer's, not body content on every page; a
    // background repeated on the pages is the header's too, behind the text
    // of every page.
    let mut objects = repeated_objects(&mut flow.elements, &state.pages, &mut work.furniture);
    objects.extend(
        repeated_backgrounds(
            &mut flow.elements,
            &state
                .pages
                .iter()
                .map(|p| (p.width, p.height))
                .collect::<Vec<_>>(),
            &mut work.furniture,
        )
        .into_iter()
        .map(|image| (false, false, image.bounds.y0, PartObject::Background(image))),
    );
    if !objects.is_empty() {
        findings.record(
            report::HEADER_FOOTER_INFERRED,
            Outcome::Mapped,
            objects.len() as u32,
            None,
        );
        let mut first_variant = Vec::new();
        for (footer, first, top, object) in objects {
            if first {
                first_variant.push((footer, top, object));
                continue;
            }
            // A background is on every page: in the first-page and even-page
            // header parts too, when the text made them.
            if matches!(object, PartObject::Background(_)) {
                for variant in [Variant::First, Variant::Even] {
                    if let Some(part) = furniture.parts.get_mut(&(false, variant)) {
                        part.objects.push((top, object.clone()));
                    }
                }
            }
            furniture
                .parts
                .entry((footer, Variant::Default))
                .or_default()
                .objects
                .push((top, object));
        }
        // The first page's own table: a title page whose parts are the
        // default ones with that table in place of the repeated one.
        if !first_variant.is_empty() && !furniture.title_page {
            for footer in [false, true] {
                let mut part = furniture
                    .parts
                    .get(&(footer, Variant::Default))
                    .cloned()
                    .unwrap_or_default();
                let own: Vec<(f64, PartObject)> = first_variant
                    .iter()
                    .filter(|o| o.0 == footer)
                    .map(|o| (o.1, o.2.clone()))
                    .collect();
                if !own.is_empty() {
                    part.objects
                        .retain(|o| !matches!(o.1, PartObject::Table(..)));
                    part.objects.extend(own);
                }
                if !part.rows.is_empty() || !part.objects.is_empty() {
                    furniture.parts.insert((footer, Variant::First), part);
                }
            }
            furniture.title_page = true;
        }
        // The body's extent and each page's content end without them.
        let (mut x0, mut y0, mut x1, mut y1) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        let mut bottoms = vec![f64::NEG_INFINITY; state.pages.len()];
        for e in &flow.elements {
            if let Some(b) = bottoms.get_mut(e.page) {
                *b = b.max(e.rect.y1);
            }
            let page = &state.pages[e.page];
            if matches!(e.kind, EKind::Text(_) | EKind::Table(_))
                && (page.width.round(), page.height.round()) == common_size
            {
                x0 = x0.min(e.rect.x0);
                y0 = y0.min(e.rect.y0);
                x1 = x1.max(e.rect.x1);
                y1 = y1.max(e.rect.y1);
            }
        }
        if x0.is_finite() {
            (bx0, by0, bx1, by1) = (x0, y0, x1, y1);
        }
        flow.page_bottoms = bottoms;
    }
    findings.record(
        report::READING_ORDER_INFERRED,
        Outcome::Mapped,
        xy_pages,
        None,
    );
    if state.structure.is_some() && tagged_pages == 0 {
        findings.record(report::TAG_STRUCTURE_IGNORED, Outcome::Degraded, 1, None);
    }
    findings.record(report::VECTOR_OMITTED, Outcome::Omitted, masks, None);
    findings.record(
        report::SCAN_REPLACED_BY_TEXT,
        Outcome::Degraded,
        scans_replaced,
        None,
    );
    findings.record(report::IMAGE_TEXT_BOXES, Outcome::Mapped, image_lines, None);
    findings.record(report::VECTOR_PLACED, Outcome::Mapped, vectors_placed, None);
    findings.record(report::FLOAT_INFERRED, Outcome::Mapped, floats, None);
    findings.record(report::TEXT_ROTATED, Outcome::Degraded, rotated, None);
    flow.page_size = sizes
        .iter()
        .max_by_key(|(_, n)| **n)
        .map_or((612.0, 792.0), |((w, h), _)| (*w as f64, *h as f64));
    flow.body = if bx0.is_finite() {
        (bx0, by0, bx1, by1)
    } else {
        (72.0, 72.0, flow.page_size.0 - 72.0, flow.page_size.1 - 72.0)
    };
    flow.furniture = furniture;
    flow.watermark = found;
    emit::emit(
        state,
        resources,
        flow,
        &key,
        &mut findings,
        &mut work,
        namespace,
    )
}

/// [`build`] in the id namespace [`crate::PdfImport::reflow`] uses for an
/// import whose own namespace is `import_namespace`: what a host that kept
/// only the source state and the resources (`casual-doc-io`'s source
/// envelope) calls to get the same document. O(document log document).
///
/// # Errors
///
/// As [`build`].
pub fn build_for_import(
    state: &PdfSourceState,
    resources: &BTreeMap<String, Vec<u8>>,
    import_namespace: u64,
) -> Result<Reflowed, PdfImportError> {
    build(state, resources, import_namespace ^ crate::REFLOW_NAMESPACE)
}

/// Whether every character of `block` is OCR's (`crate::ocr::OCR_FONT`).
/// O(characters).
fn optical_block(block: &group::Block) -> bool {
    let mut chars = block.lines.iter().flat_map(|l| l.chars.iter()).peekable();
    chars.peek().is_some() && chars.all(|c| c.style.font == crate::ocr::OCR_FONT)
}

/// Takes out of the flow the text elements OCR read from `image`: each one
/// whose every character is the recogniser's and whose centre lies on the
/// picture. Their lines are returned in order, for the picture's line boxes.
/// O(elements).
fn claim_optical_lines(
    image: &SourceImage,
    elements: &[Element],
    taken: &mut [bool],
) -> Vec<ILine> {
    let b = image.bounds;
    let mut lines = Vec::new();
    for (i, e) in elements.iter().enumerate() {
        if taken.get(i).copied().unwrap_or(true) {
            continue;
        }
        let EKind::Text(text) = &e.kind else {
            continue;
        };
        let (cx, cy) = ((e.rect.x0 + e.rect.x1) / 2.0, (e.rect.y0 + e.rect.y1) / 2.0);
        if !(cx > b.x0 && cx < b.x1 && cy > b.y0 && cy < b.y1) {
            continue;
        }
        let mut chars = text.iter().flat_map(|l| l.chars.iter()).peekable();
        if chars.peek().is_none() || !chars.all(|c| c.style.font == crate::ocr::OCR_FONT) {
            continue;
        }
        lines.extend(text.iter().cloned());
        taken[i] = true;
    }
    lines
}

/// Sorts one annotation into links, comments or the omitted count.
fn classify(
    annotation: &AnnotationEvidence,
    flow: &mut Flow,
    links: &mut Vec<(usize, Bounds)>,
    comments: &mut Vec<(usize, Bounds)>,
    findings: &mut Findings,
    page: usize,
) {
    match &annotation.kind {
        AnnotationKind::Link(LinkTarget::Uri(_) | LinkTarget::Page { .. }) => {
            links.push((flow.links.len(), annotation.rect));
            flow.links.push(ILink {
                target: annotation_target(annotation),
            });
        }
        AnnotationKind::Link(_) => {
            findings.record(report::LINK_UNRESOLVED, Outcome::Omitted, 1, Some(page));
        }
        AnnotationKind::Note { .. } => {
            let text = annotation.contents.clone().unwrap_or_default();
            if text.trim().is_empty() {
                findings.record(report::ANNOTATION_OMITTED, Outcome::Omitted, 1, Some(page));
                return;
            }
            comments.push((flow.comments.len(), annotation.rect));
            flow.comments.push(IComment {
                author: annotation.author.clone(),
                date: annotation.date.clone(),
                text,
                object: annotation.object,
                reply_to: annotation.in_reply_to,
            });
        }
        // A popup is its parent's window; widgets and attachments are
        // counted with forms and files at admission.
        AnnotationKind::Popup | AnnotationKind::Widget | AnnotationKind::FileAttachment => {}
        AnnotationKind::Other => {
            findings.record(report::ANNOTATION_OMITTED, Outcome::Omitted, 1, Some(page));
        }
    }
}

fn annotation_target(annotation: &AnnotationEvidence) -> LinkTarget {
    match &annotation.kind {
        AnnotationKind::Link(target) => target.clone(),
        _ => LinkTarget::Unresolved,
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_layout;
