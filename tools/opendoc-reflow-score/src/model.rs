//! What a document IS, structurally, for comparing a reflowed PDF against
//! the DOCX it was printed from: paragraphs with their heading level, list
//! level and dominant run formatting; tables; pictures; header and footer;
//! notes; sections. O(document).

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
}

/// One table.
#[derive(Clone, Debug, Default)]
pub struct Tab {
    /// Rows of cell texts.
    pub cells: Vec<Vec<String>>,
    /// Cells with a grid span or a vertical merge.
    pub merges: usize,
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
            .or_else(|| chain(defs, para.style_ref, |s| s.run.as_ref().and_then(get)))
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
    text_boxes: usize,
    groups: usize,
    page_fields: usize,
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
            InlineNode::Drawing(_) | InlineNode::AnchoredDrawing(_) => out.pictures += 1,
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
                sum.text_boxes += walk.text_boxes;
                sum.groups += walk.groups;
                all.push_str(&walk.text);
                all.push('\n');
                let text = collapse(&walk.text);
                if !in_table && !text.is_empty() {
                    let (bold, italic, size) = majority(&walk.formats);
                    sum.paras.push(Para {
                        text,
                        heading: heading_of(defs, &p.properties),
                        list: list_of(defs, &p.properties),
                        bold,
                        italic,
                        size,
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
                let mut tab = Tab::default();
                for row in &t.rows {
                    let mut cells = Vec::new();
                    for cell in &row.cells {
                        if cell.properties.grid_span.is_some_and(|s| s > 1)
                            || cell.properties.vertical_merge.is_some()
                        {
                            tab.merges += 1;
                        }
                        let mut inner = Summary::default();
                        let mut text = String::new();
                        blocks(defs, &cell.blocks, true, &mut inner, &mut text);
                        sum.pictures += inner.pictures;
                        all.push_str(&text);
                        cells.push(collapse(&text));
                    }
                    tab.cells.push(cells);
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
