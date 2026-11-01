//! What a document IS, structurally, for comparing a reflowed PDF against
//! the DOCX it was printed from: paragraphs with their heading level, list
//! level and dominant run formatting; tables; pictures; header and footer;
//! notes; sections. O(document).

use casual_doc_model::v1::{Alignment, LineRule, Spacing};
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, FieldKind, InlineNode, ParagraphProperties, RunProperties,
    StyleId,
};

/// One paragraph.
#[derive(Clone, Debug, Default)]
pub struct Para {
    /// Its text, whitespace collapsed.
    pub text: String,
    /// Heading level (0 for Title), from the style chain or outline level.
    pub heading: Option<u8>,
    /// List level, when numbered or bulleted.
    pub list: Option<u8>,
    /// Majority bold, italic, and size in half points over its characters.
    pub bold: bool,
    /// Majority italic.
    pub italic: bool,
    /// Majority size (half points).
    pub size: u32,
    /// Alignment through the style chain: 0 start, 1 centre, 2 end, 3 both.
    pub align: u8,
    /// Line pitch in points, through the style chain (`auto` lines as
    /// `percent` of 1.15 em of its size; `exact` and `atLeast` their height).
    pub pitch: f64,
    /// Space before, in points, through the style chain.
    pub before: f64,
    /// Space after, in points, through the style chain.
    pub after: f64,
    /// Start indent and first-line indent (negative: hanging), in points,
    /// through the style chain.
    pub indent: (f64, f64),
}

/// One table.
#[derive(Clone, Debug, Default)]
pub struct Tab {
    /// Rows of cell texts.
    pub cells: Vec<Vec<String>>,
    /// Cells with a grid span or a vertical merge.
    pub merges: usize,
    /// The grid's column widths, in points.
    pub widths: Vec<f64>,
    /// Each cell's first paragraph's alignment (0 start, 1 centre, 2 end,
    /// 3 both), through its style chain.
    pub aligns: Vec<Vec<u8>>,
}

/// The whole document.
#[derive(Clone, Debug, Default)]
pub struct Summary {
    /// Body paragraphs outside tables, in order (empty ones dropped).
    pub paras: Vec<Para>,
    /// Tables, in order (nested tables flattened into their parent's count).
    pub tables: Vec<Tab>,
    /// Inline and anchored pictures.
    pub pictures: usize,
    /// Each picture's extent, in points, in document order.
    pub picture_sizes: Vec<(f64, f64)>,
    /// The first section's watermark: its text (or `"<picture>"`) and
    /// whether it is diagonal.
    pub watermark: Option<(String, bool)>,
    /// Text boxes (frames) in the body: zero in a flowing document.
    pub text_boxes: usize,
    /// Shape groups.
    pub groups: usize,
    /// Header text (all parts), whitespace collapsed.
    pub header: String,
    /// Footer text.
    pub footer: String,
    /// PAGE fields in headers and footers.
    pub page_fields: usize,
    /// Footnotes and endnotes.
    pub notes: usize,
    /// Sections and the largest column count.
    pub sections: usize,
    /// Largest column count of any section.
    pub columns: u16,
    /// First section's page size (twips).
    pub page: (i32, i32),
    /// First section's margins: top, bottom, start, end (twips).
    pub margins: (i32, i32, i32, i32),
    /// U+FFFD, private-use and ligature characters in all text.
    pub bad_chars: (usize, usize, usize),
    /// Every non-ASCII character, with counts.
    pub non_ascii: std::collections::BTreeMap<char, usize>,
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The style chain's first value of `get`, from `id` up its `based_on`.
fn chain<T>(
    defs: &Definitions,
    mut id: Option<StyleId>,
    get: impl Fn(&casual_doc_model::v1::Style) -> Option<T>,
) -> Option<T> {
    let mut guard = 0;
    while let Some(current) = id {
        guard += 1;
        if guard > 32 {
            break;
        }
        let style = defs.styles.get(&current)?;
        if let Some(value) = get(style) {
            return Some(value);
        }
        id = style.based_on;
    }
    None
}

/// The paragraph's style: its own, else the document's default paragraph
/// style (Normal), which applies to every paragraph that names none. O(styles).
fn own_style(defs: &Definitions, p: &ParagraphProperties) -> Option<StyleId> {
    p.style_ref.or_else(|| {
        defs.styles
            .iter()
            .find(|(_, s)| s.is_default && s.kind == casual_doc_model::v1::StyleKind::Paragraph)
            .map(|(id, _)| *id)
    })
}

fn heading_of(defs: &Definitions, p: &ParagraphProperties) -> Option<u8> {
    if let Some(level) = p.outline_level.filter(|l| *l < 9) {
        return Some(level + 1);
    }
    chain(defs, p.style_ref, |s| {
        let name = s.name.as_deref()?.to_ascii_lowercase();
        if name == "title" {
            return Some(0);
        }
        if let Some(n) = name.strip_prefix("heading ") {
            return n.trim().parse::<u8>().ok();
        }
        s.paragraph
            .as_ref()
            .and_then(|pp| pp.outline_level)
            .filter(|l| *l < 9)
            .map(|l| l + 1)
    })
}

fn list_of(defs: &Definitions, p: &ParagraphProperties) -> Option<u8> {
    if p.numbering_none {
        return None;
    }
    if let Some(n) = &p.numbering {
        return Some(n.level);
    }
    chain(defs, p.style_ref, |s| {
        s.paragraph
            .as_ref()
            .and_then(|pp| pp.numbering.as_ref())
            .map(|n| n.level)
    })
}

/// A paragraph's alignment through its style chain: 0 start, 1 centre,
/// 2 end, 3 both.
fn align_of(defs: &Definitions, p: &ParagraphProperties) -> u8 {
    let align = p.alignment.or_else(|| {
        chain(defs, own_style(defs, p), |s| {
            s.paragraph.as_ref().and_then(|pp| pp.alignment)
        })
    });
    match align {
        Some(Alignment::Center) => 1,
        Some(Alignment::End) => 2,
        Some(Alignment::Justify) => 3,
        _ => 0,
    }
}

/// A paragraph's start and first-line indents (negative: hanging), in
/// points, through its style chain. O(style chain).
fn indent_of(defs: &Definitions, p: &ParagraphProperties) -> (f64, f64) {
    let get = |f: &dyn Fn(&casual_doc_model::v1::Indentation) -> Option<i32>| -> Option<i32> {
        p.indentation.as_ref().and_then(f).or_else(|| {
            chain(defs, own_style(defs, p), |s| {
                s.paragraph
                    .as_ref()
                    .and_then(|pp| pp.indentation.as_ref())
                    .and_then(f)
            })
        })
    };
    let start = f64::from(get(&|i| i.start_twips).unwrap_or(0)) / 20.0;
    let first = f64::from(get(&|i| i.first_line_twips).unwrap_or(0)) / 20.0
        - f64::from(get(&|i| i.hanging_twips).unwrap_or(0)) / 20.0;
    (start, first)
}

/// The alignment, line pitch, and space before and after of a paragraph of
/// majority size `size` (half points), through its style chain and the
/// document defaults. O(style chain).
fn layout_of(defs: &Definitions, p: &ParagraphProperties, size: u32) -> (u8, f64, f64, f64) {
    let spacing = |get: &dyn Fn(&Spacing) -> Option<i32>| -> Option<i32> {
        p.spacing
            .as_ref()
            .and_then(get)
            .or_else(|| {
                chain(defs, own_style(defs, p), |s| {
                    s.paragraph
                        .as_ref()
                        .and_then(|pp| pp.spacing.as_ref())
                        .and_then(get)
                })
            })
            .or_else(|| {
                defs.document_defaults
                    .as_ref()
                    .and_then(|d| d.paragraph.as_ref())
                    .and_then(|pp| pp.spacing.as_ref())
                    .and_then(get)
            })
    };
    let align = align_of(defs, p);
    let em = f64::from(size) / 2.0;
    let rule = spacing(&|s| {
        s.line_rule.map(|r| match r {
            LineRule::Exact => 2,
            LineRule::AtLeast => 1,
            LineRule::Auto => 0,
        })
    });
    let twips = spacing(&|s| s.line_twips);
    let pitch = match (rule, twips) {
        (Some(2), Some(t)) => f64::from(t) / 20.0,
        (Some(1), Some(t)) => (f64::from(t) / 20.0).max(1.15 * em),
        _ => {
            f64::from(spacing(&|s| s.line_percent.map(i32::from)).unwrap_or(100)) / 100.0
                * 1.15
                * em
        }
    };
    let before = f64::from(spacing(&|s| s.before_twips).unwrap_or(0)) / 20.0;
    let after = f64::from(spacing(&|s| s.after_twips).unwrap_or(0)) / 20.0;
    (align, pitch, before, after)
}

/// Bold, italic and size of a run, through its run style, the paragraph
/// style chain and the document defaults.
fn run_format(
    defs: &Definitions,
    para: &ParagraphProperties,
    run: &RunProperties,
) -> (bool, bool, u32) {
    let from_chain = |get: &dyn Fn(&RunProperties) -> Option<u32>| -> Option<u32> {
        get(run)
            .or_else(|| chain(defs, run.style_ref, |s| s.run.as_ref().and_then(get)))
            .or_else(|| {
                chain(defs, own_style(defs, para), |s| {
                    s.run.as_ref().and_then(get)
                })
            })
            .or_else(|| {
                defs.document_defaults
                    .as_ref()
                    .and_then(|d| d.run.as_ref())
                    .and_then(get)
            })
    };
    let bold = from_chain(&|r| r.bold.map(u32::from)).unwrap_or(0) == 1;
    let italic = from_chain(&|r| r.italic.map(u32::from)).unwrap_or(0) == 1;
    let size = from_chain(&|r| r.size_half_points).unwrap_or(20);
    (bold, italic, size)
}

#[derive(Default)]
struct Walk {
    text: String,
    formats: Vec<(bool, bool, u32, usize)>,
    pictures: usize,
    picture_sizes: Vec<(f64, f64)>,
    text_boxes: usize,
    groups: usize,
    page_fields: usize,
}

/// EMU to points.
fn emu_points(emu: i64) -> f64 {
    emu as f64 / 12_700.0
}

fn inlines(defs: &Definitions, para: &ParagraphProperties, list: &[InlineNode], out: &mut Walk) {
    for inline in list {
        match inline {
            InlineNode::Run(run) => {
                let (b, i, s) = run_format(defs, para, &run.properties);
                let n = run.text.chars().filter(|c| !c.is_whitespace()).count();
                out.formats.push((b, i, s, n));
                out.text.push_str(&run.text);
            }
            InlineNode::Tab(_) => out.text.push(' '),
            InlineNode::Break(_) => out.text.push(' '),
            InlineNode::Drawing(d) => {
                out.pictures += 1;
                if let Some(e) = d.extent {
                    out.picture_sizes
                        .push((emu_points(e.width_emu), emu_points(e.height_emu)));
                }
            }
            InlineNode::AnchoredDrawing(d) => {
                out.pictures += 1;
                out.picture_sizes.push((
                    emu_points(d.extent.width_emu),
                    emu_points(d.extent.height_emu),
                ));
            }
            InlineNode::TextBox(_) => out.text_boxes += 1,
            InlineNode::Group(_) => out.groups += 1,
            InlineNode::Hyperlink(h) => inlines(defs, para, &h.inlines, out),
            InlineNode::Field(f) => {
                if matches!(f.kind, FieldKind::Page) {
                    out.page_fields += 1;
                }
                inlines(defs, para, &f.inlines, out);
            }
            InlineNode::Sdt(s) => inlines(defs, para, &s.inlines, out),
            _ => {}
        }
    }
}

fn majority(formats: &[(bool, bool, u32, usize)]) -> (bool, bool, u32) {
    let total: usize = formats.iter().map(|f| f.3).sum();
    let bold: usize = formats.iter().filter(|f| f.0).map(|f| f.3).sum();
    let italic: usize = formats.iter().filter(|f| f.1).map(|f| f.3).sum();
    let mut sizes: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for f in formats {
        *sizes.entry(f.2).or_default() += f.3;
    }
    let size = sizes
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map_or(20, |(s, _)| s);
    (bold * 2 > total, italic * 2 > total, size)
}

fn blocks(
    defs: &Definitions,
    list: &[BlockNode],
    in_table: bool,
    sum: &mut Summary,
    all: &mut String,
) {
    for block in list {
        match block {
            BlockNode::Paragraph(p) => {
                let mut walk = Walk::default();
                inlines(defs, &p.properties, &p.inlines, &mut walk);
                sum.pictures += walk.pictures;
                sum.picture_sizes.extend(walk.picture_sizes);
                sum.text_boxes += walk.text_boxes;
                sum.groups += walk.groups;
                all.push_str(&walk.text);
                all.push('\n');
                let text = collapse(&walk.text);
                if !in_table && !text.is_empty() {
                    let (bold, italic, size) = majority(&walk.formats);
                    let (align, pitch, before, after) = layout_of(defs, &p.properties, size);
                    sum.paras.push(Para {
                        text,
                        heading: heading_of(defs, &p.properties),
                        list: list_of(defs, &p.properties),
                        bold,
                        italic,
                        size,
                        align,
                        pitch,
                        before,
                        after,
                        indent: indent_of(defs, &p.properties),
                    });
                }
            }
            // A one-cell table is a shaded box around paragraphs: in any
            // flowing reading of its PDF (Word's, Google Docs', ours) it is
            // those paragraphs, so that is what the ground truth holds.
            BlockNode::Table(t) if !in_table && t.rows.len() == 1 && t.rows[0].cells.len() == 1 => {
                blocks(defs, &t.rows[0].cells[0].blocks, false, sum, all);
            }
            BlockNode::Table(t) => {
                let mut tab = Tab {
                    widths: t
                        .grid
                        .iter()
                        .map(|g| f64::from(g.width_twips.unwrap_or(0)) / 20.0)
                        .collect(),
                    ..Tab::default()
                };
                for row in &t.rows {
                    let mut cells = Vec::new();
                    let mut aligns = Vec::new();
                    for cell in &row.cells {
                        aligns.push(
                            cell.blocks
                                .iter()
                                .find_map(|b| match b {
                                    BlockNode::Paragraph(p) => Some(align_of(defs, &p.properties)),
                                    _ => None,
                                })
                                .unwrap_or(0),
                        );
                        if cell.properties.grid_span.is_some_and(|s| s > 1)
                            || cell.properties.vertical_merge.is_some()
                        {
                            tab.merges += 1;
                        }
                        let mut inner = Summary::default();
                        let mut text = String::new();
                        blocks(defs, &cell.blocks, true, &mut inner, &mut text);
                        sum.pictures += inner.pictures;
                        sum.picture_sizes.extend(inner.picture_sizes);
                        all.push_str(&text);
                        cells.push(collapse(&text));
                    }
                    tab.cells.push(cells);
                    tab.aligns.push(aligns);
                }
                if !in_table {
                    sum.tables.push(tab);
                }
            }
            BlockNode::Sdt(s) => blocks(defs, &s.blocks, in_table, sum, all),
            _ => {}
        }
    }
}

fn part_text(defs: &Definitions, list: &[BlockNode], fields: &mut usize) -> String {
    let mut out = String::new();
    for block in list {
        if let BlockNode::Paragraph(p) = block {
            let mut walk = Walk::default();
            inlines(defs, &p.properties, &p.inlines, &mut walk);
            *fields += walk.page_fields;
            out.push_str(&walk.text);
            out.push(' ');
        }
    }
    collapse(&out)
}

/// The summary of `document`. O(document).
#[must_use]
pub fn summarize(document: &Document) -> Summary {
    let defs = document.definitions();
    let mut sum = Summary::default();
    let mut all = String::new();
    blocks(defs, document.body(), false, &mut sum, &mut all);
    let mut fields = 0;
    let mut header = Vec::new();
    for part in defs.headers.iter().map(|(_, v)| v) {
        header.push(part_text(defs, &part.blocks, &mut fields));
    }
    let mut footer = Vec::new();
    for part in defs.footers.iter().map(|(_, v)| v) {
        footer.push(part_text(defs, &part.blocks, &mut fields));
    }
    header.sort();
    header.dedup();
    footer.sort();
    footer.dedup();
    sum.header = collapse(&header.join(" | "));
    sum.footer = collapse(&footer.join(" | "));
    sum.page_fields = fields;
    sum.notes = defs.footnotes.len() + defs.endnotes.len();
    for note in defs
        .footnotes
        .iter()
        .chain(defs.endnotes.iter())
        .map(|(_, v)| v)
    {
        let mut inner = Summary::default();
        blocks(defs, &note.blocks, true, &mut inner, &mut all);
    }
    sum.sections = defs.sections.len();
    sum.columns = defs
        .sections
        .iter()
        .map(|s| s.columns.count)
        .max()
        .unwrap_or(1);
    sum.watermark = defs
        .sections
        .iter()
        .find_map(|s| s.watermark.as_ref())
        .map(|w| {
            let text = match &w.content {
                casual_doc_model::v1::WatermarkContent::Text(t) => t.text.clone(),
                casual_doc_model::v1::WatermarkContent::Picture(_) => "<picture>".to_owned(),
            };
            (
                text,
                w.layout == casual_doc_model::v1::WatermarkLayout::Diagonal,
            )
        });
    if let Some(first) = defs.sections.first() {
        sum.page = (first.page_size.width_twips, first.page_size.height_twips);
        let m = &first.page_margins;
        sum.margins = (m.top_twips, m.bottom_twips, m.start_twips, m.end_twips);
    }
    for c in all.chars() {
        match c {
            '\u{FFFD}' => sum.bad_chars.0 += 1,
            '\u{E000}'..='\u{F8FF}' => sum.bad_chars.1 += 1,
            '\u{FB00}'..='\u{FB06}' => sum.bad_chars.2 += 1,
            _ => {}
        }
        if !c.is_ascii() && !c.is_whitespace() {
            *sum.non_ascii.entry(c).or_default() += 1;
        }
    }
    sum
}

/// Non-whitespace characters per size (half points) over every run of the
/// document: body, tables, headers, footers and notes. Compared with the
/// PDF's own glyph sizes (`main.rs` `pdf_sizes`) it says whether the reflow
/// kept every size exactly, with no ground truth. O(document).
#[must_use]
pub fn size_histogram(document: &Document) -> std::collections::BTreeMap<u32, usize> {
    fn walk_inlines(
        defs: &Definitions,
        para: &ParagraphProperties,
        list: &[InlineNode],
        out: &mut std::collections::BTreeMap<u32, usize>,
    ) {
        for inline in list {
            match inline {
                InlineNode::Run(run) => {
                    let (_, _, size) = run_format(defs, para, &run.properties);
                    let n = run.text.chars().filter(|c| !c.is_whitespace()).count();
                    if n > 0 {
                        *out.entry(size).or_default() += n;
                    }
                }
                InlineNode::Hyperlink(h) => walk_inlines(defs, para, &h.inlines, out),
                InlineNode::Field(f) => walk_inlines(defs, para, &f.inlines, out),
                InlineNode::Sdt(s) => walk_inlines(defs, para, &s.inlines, out),
                _ => {}
            }
        }
    }
    fn walk(
        defs: &Definitions,
        list: &[BlockNode],
        out: &mut std::collections::BTreeMap<u32, usize>,
    ) {
        for block in list {
            match block {
                BlockNode::Paragraph(p) => walk_inlines(defs, &p.properties, &p.inlines, out),
                BlockNode::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            walk(defs, &cell.blocks, out);
                        }
                    }
                }
                BlockNode::Sdt(s) => walk(defs, &s.blocks, out),
                _ => {}
            }
        }
    }
    let defs = document.definitions();
    let mut out = std::collections::BTreeMap::new();
    walk(defs, document.body(), &mut out);
    for part in defs
        .headers
        .iter()
        .chain(defs.footers.iter())
        .map(|(_, v)| v)
    {
        walk(defs, &part.blocks, &mut out);
    }
    for note in defs
        .footnotes
        .iter()
        .chain(defs.endnotes.iter())
        .map(|(_, v)| v)
    {
        walk(defs, &note.blocks, &mut out);
    }
    out
}

/// How consistent the body's spacing and indents are: over the plain body
/// paragraphs (no heading, no list), the share whose (before, after, pitch)
/// is the most common one, the number of distinct triples, the share whose
/// start indent is the most common one, and the number of distinct start
/// indents (each rounded to half a point). A reflow that measures each
/// paragraph on its own scatters these; the PDF's own style sheet did not.
#[must_use]
pub fn consistency(summary: &Summary) -> (f64, usize, f64, usize) {
    let plain: Vec<&Para> = summary
        .paras
        .iter()
        .filter(|p| p.heading.is_none() && p.list.is_none())
        .collect();
    let half = |v: f64| (v * 2.0).round() as i64;
    let mut triples: std::collections::BTreeMap<(i64, i64, i64), usize> =
        std::collections::BTreeMap::new();
    let mut starts: std::collections::BTreeMap<i64, usize> = std::collections::BTreeMap::new();
    for p in &plain {
        *triples
            .entry((half(p.before), half(p.after), half(p.pitch)))
            .or_default() += 1;
        *starts.entry(half(p.indent.0)).or_default() += 1;
    }
    let modal = |largest: usize| {
        if plain.is_empty() {
            1.0
        } else {
            largest as f64 / plain.len() as f64
        }
    };
    (
        modal(triples.values().copied().max().unwrap_or(0)),
        triples.len(),
        modal(starts.values().copied().max().unwrap_or(0)),
        starts.len(),
    )
}
