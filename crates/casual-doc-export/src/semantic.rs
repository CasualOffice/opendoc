//! Semantic DOCX writer: serialize a v1 `Document` model back to a valid,
//! editable WordprocessingML package (the dual of the Retention byte-copy
//! writer). Written today: the core body (`w:document`/`w:body`, paragraphs,
//! runs, the mapped run/paragraph properties, tabs/breaks), tables (grid +
//! table/row/cell properties, merges, nested tables), and the self-contained
//! inline constructs (hyperlinks with `document.xml.rels` generation, fields,
//! bookmarks, tracked-change revisions, inline content controls). The
//! media-backed constructs (drawings, text boxes) and the definition parts
//! (styles/numbering/notes/headers/comments, and note/comment references) land
//! in later slices; a model that uses them is written with what is supported and
//! the rest is skipped (never a malformed part).
//!
//! Output is byte-deterministic for a given model: parts in fixed order, a fixed
//! ZIP timestamp, ids/relationships re-minted in document order.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};

use casual_doc_model::v1::{
    Alignment, BlockNode, BorderEdge, BreakKind, CellVerticalAlignment, Color, Definitions,
    Document, HeightRule, HyperlinkTarget, InlineNode, ParagraphProperties, RevisionKind, RgbColor,
    RunProperties, SdtControlKind, SdtProperties, Table, TableBorders, TableCell,
    TableCellProperties, TableLayout, TableProperties, TableRow, TableRowProperties, TextDirection,
    VerticalMerge,
};
use quick_xml::Writer;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::ExportError;

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const CT_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const DOC_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const DOC_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const HYPERLINK_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";

/// A `document.xml.rels` entry: (relationship id, external target URL).
type RelEntry = (String, String);

/// Accumulates the `word/_rels/document.xml.rels` entries the body needs while
/// it is written. Today that is one external-target relationship per distinct
/// hyperlink URL; media/part relationships join in later slices. `rId`s are
/// assigned in document order so the package is byte-deterministic, and a URL
/// seen twice reuses its `rId` (harmless to the round-trip, which stores the URL
/// not the id).
struct RelBuilder {
    next: u32,
    by_url: BTreeMap<String, String>,
    entries: Vec<RelEntry>,
}

impl RelBuilder {
    fn new() -> Self {
        Self {
            next: 0,
            by_url: BTreeMap::new(),
            entries: Vec::new(),
        }
    }

    /// Returns the `rId` for an external hyperlink URL, minting a new one (and
    /// recording the relationship) the first time each URL is seen.
    fn hyperlink(&mut self, url: &str) -> String {
        if let Some(id) = self.by_url.get(url) {
            return id.clone();
        }
        self.next += 1;
        let id = format!("rId{}", self.next);
        self.by_url.insert(url.to_string(), id.clone());
        self.entries.push((id.clone(), url.to_string()));
        id
    }
}

/// Threaded write context: the bookmark-name source (bookmark markers carry only
/// a `BookmarkId`; the name lives in `Definitions`) and the relationship
/// accumulator (external hyperlinks).
struct Ctx<'a> {
    defs: &'a Definitions,
    rels: RelBuilder,
}

/// Serializes a v1 `Document` to a DOCX package. `media` supplies binary image
/// bytes by part name (the model carries `MediaReference` metadata, not bytes);
/// pass an empty map when the model uses no drawings.
pub fn write_document(
    document: &Document,
    _media: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, ExportError> {
    let (document_xml, rels) = document_xml(document)?;

    // Fixed part set; each is emitted in a deterministic order so the package
    // bytes are reproducible. The document relationships part carries any
    // external hyperlink targets collected while writing the body (empty
    // otherwise — byte-identical to the no-relationship case).
    let parts: [(&str, Vec<u8>); 4] = [
        ("[Content_Types].xml", content_types_xml()?),
        ("_rels/.rels", root_rels_xml()?),
        ("word/document.xml", document_xml),
        ("word/_rels/document.xml.rels", document_rels_xml(&rels)?),
    ];

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::default());
    for (name, bytes) in parts {
        writer
            .start_file(name, options)
            .map_err(|_| ExportError::Package)?;
        writer.write_all(&bytes).map_err(|_| ExportError::Package)?;
    }
    Ok(writer
        .finish()
        .map_err(|_| ExportError::Package)?
        .into_inner())
}

fn new_writer() -> Writer<Cursor<Vec<u8>>> {
    Writer::new(Cursor::new(Vec::new()))
}

fn finish(writer: Writer<Cursor<Vec<u8>>>) -> Vec<u8> {
    writer.into_inner().into_inner()
}

fn start<'a>(name: &'a str) -> BytesStart<'a> {
    BytesStart::new(name)
}

/// Emits `[Content_Types].xml` with the standard defaults plus the main-document
/// override.
fn content_types_xml() -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut types = start("Types");
    types.push_attribute(("xmlns", CT_NS));
    w.write_event(Event::Start(types)).map_err(pkg)?;
    for (ext, ct) in [
        (
            "rels",
            "application/vnd.openxmlformats-package.relationships+xml",
        ),
        ("xml", "application/xml"),
    ] {
        let mut d = start("Default");
        d.push_attribute(("Extension", ext));
        d.push_attribute(("ContentType", ct));
        w.write_event(Event::Empty(d)).map_err(pkg)?;
    }
    let mut over = start("Override");
    over.push_attribute(("PartName", "/word/document.xml"));
    over.push_attribute(("ContentType", DOC_CT));
    w.write_event(Event::Empty(over)).map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("Types")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// Emits `_rels/.rels` pointing at the main document.
fn root_rels_xml() -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut rels = start("Relationships");
    rels.push_attribute(("xmlns", REL_NS));
    w.write_event(Event::Start(rels)).map_err(pkg)?;
    let mut rel = start("Relationship");
    rel.push_attribute(("Id", "rId1"));
    rel.push_attribute(("Type", DOC_REL_TYPE));
    rel.push_attribute(("Target", "word/document.xml"));
    w.write_event(Event::Empty(rel)).map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("Relationships")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// Emits `word/_rels/document.xml.rels`. With no entries it is the empty
/// `<Relationships/>` element (byte-identical to the earlier no-relationship
/// slices); with entries it carries one external hyperlink relationship each.
fn document_rels_xml(entries: &[RelEntry]) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut rels = start("Relationships");
    rels.push_attribute(("xmlns", REL_NS));
    if entries.is_empty() {
        w.write_event(Event::Empty(rels)).map_err(pkg)?;
        return Ok(finish(w));
    }
    w.write_event(Event::Start(rels)).map_err(pkg)?;
    for (id, url) in entries {
        let mut rel = start("Relationship");
        rel.push_attribute(("Id", id.as_str()));
        rel.push_attribute(("Type", HYPERLINK_REL_TYPE));
        rel.push_attribute(("Target", url.as_str()));
        rel.push_attribute(("TargetMode", "External"));
        w.write_event(Event::Empty(rel)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("Relationships")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// Emits `word/document.xml` from the model body, returning the bytes plus the
/// external hyperlink relationships collected while writing (for
/// `document.xml.rels`).
fn document_xml(document: &Document) -> Result<(Vec<u8>, Vec<RelEntry>), ExportError> {
    let mut w = new_writer();
    let mut doc = start("w:document");
    doc.push_attribute(("xmlns:w", W_NS));
    // `xmlns:r` is required so a hyperlink's `r:id` is well-formed; harmless when
    // no relationship-referencing construct is present.
    doc.push_attribute(("xmlns:r", R_NS));
    w.write_event(Event::Start(doc)).map_err(pkg)?;
    w.write_event(Event::Start(start("w:body"))).map_err(pkg)?;

    let mut ctx = Ctx {
        defs: document.definitions(),
        rels: RelBuilder::new(),
    };
    for block in document.body() {
        write_block(&mut w, block, &mut ctx)?;
    }

    w.write_event(Event::End(BytesEnd::new("w:body")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:document")))
        .map_err(pkg)?;
    Ok((finish(w), ctx.rels.entries))
}

/// Emits a body/cell block. Content-control block wrappers (`BlockNode::Sdt`)
/// are a later slice — their inner blocks are emitted directly (no text loss).
fn write_block(
    w: &mut Writer<Cursor<Vec<u8>>>,
    block: &BlockNode,
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    match block {
        BlockNode::Paragraph(paragraph) => {
            write_paragraph(w, &paragraph.properties, &paragraph.inlines, ctx)
        }
        BlockNode::Table(table) => write_table(w, table, ctx),
        BlockNode::Sdt(sdt) => {
            for inner in &sdt.blocks {
                write_block(w, inner, ctx)?;
            }
            Ok(())
        }
    }
}

fn write_table(
    w: &mut Writer<Cursor<Vec<u8>>>,
    table: &Table,
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:tbl"))).map_err(pkg)?;
    write_table_properties(w, &table.properties)?;
    if !table.grid.is_empty() {
        w.write_event(Event::Start(start("w:tblGrid")))
            .map_err(pkg)?;
        for column in &table.grid {
            let mut col = start("w:gridCol");
            if let Some(width) = column.width_twips {
                col.push_attribute(("w:w", width.to_string().as_str()));
            }
            w.write_event(Event::Empty(col)).map_err(pkg)?;
        }
        w.write_event(Event::End(BytesEnd::new("w:tblGrid")))
            .map_err(pkg)?;
    }
    for row in &table.rows {
        write_row(w, row, ctx)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:tbl")))
        .map_err(pkg)?;
    Ok(())
}

fn write_table_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &TableProperties,
) -> Result<(), ExportError> {
    if *properties == TableProperties::default() {
        return Ok(());
    }
    w.write_event(Event::Start(start("w:tblPr"))).map_err(pkg)?;
    if let Some(alignment) = properties.alignment {
        let mut jc = start("w:jc");
        jc.push_attribute(("w:val", alignment_token(alignment)));
        w.write_event(Event::Empty(jc)).map_err(pkg)?;
    }
    if let Some(width) = properties.width_twips {
        let mut el = start("w:tblW");
        el.push_attribute(("w:type", "dxa"));
        el.push_attribute(("w:w", width.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(layout) = properties.layout {
        let mut el = start("w:tblLayout");
        el.push_attribute((
            "w:type",
            match layout {
                TableLayout::Fixed => "fixed",
                TableLayout::Autofit => "autofit",
            },
        ));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if !properties.look.is_empty() {
        let look = &properties.look;
        let mut el = start("w:tblLook");
        for (on, name) in [
            (look.first_row, "w:firstRow"),
            (look.last_row, "w:lastRow"),
            (look.first_column, "w:firstColumn"),
            (look.last_column, "w:lastColumn"),
            (look.no_h_band, "w:noHBand"),
            (look.no_v_band, "w:noVBand"),
        ] {
            if on {
                el.push_attribute((name, "1"));
            }
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    write_borders(w, "w:tblBorders", &properties.borders)?;
    write_shading(w, &properties.shading)?;
    write_margins(w, "w:tblCellMar", &properties.cell_margins)?;
    w.write_event(Event::End(BytesEnd::new("w:tblPr")))
        .map_err(pkg)?;
    Ok(())
}

fn write_row(
    w: &mut Writer<Cursor<Vec<u8>>>,
    row: &TableRow,
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:tr"))).map_err(pkg)?;
    write_row_properties(w, &row.properties)?;
    for cell in &row.cells {
        write_cell(w, cell, ctx)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:tr")))
        .map_err(pkg)?;
    Ok(())
}

fn write_row_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &TableRowProperties,
) -> Result<(), ExportError> {
    if *properties == TableRowProperties::default() {
        return Ok(());
    }
    w.write_event(Event::Start(start("w:trPr"))).map_err(pkg)?;
    if !properties.height.is_empty() {
        let mut el = start("w:trHeight");
        if let Some(value) = properties.height.value_twips {
            el.push_attribute(("w:val", value.to_string().as_str()));
        }
        if let Some(rule) = properties.height.rule {
            el.push_attribute((
                "w:hRule",
                match rule {
                    HeightRule::Auto => "auto",
                    HeightRule::AtLeast => "atLeast",
                    HeightRule::Exact => "exact",
                },
            ));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if properties.cant_split {
        w.write_event(Event::Empty(start("w:cantSplit")))
            .map_err(pkg)?;
    }
    if properties.header {
        w.write_event(Event::Empty(start("w:tblHeader")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:trPr")))
        .map_err(pkg)?;
    Ok(())
}

fn write_cell(
    w: &mut Writer<Cursor<Vec<u8>>>,
    cell: &TableCell,
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:tc"))).map_err(pkg)?;
    write_cell_properties(w, &cell.properties)?;
    for block in &cell.blocks {
        write_block(w, block, ctx)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:tc")))
        .map_err(pkg)?;
    Ok(())
}

fn write_cell_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &TableCellProperties,
) -> Result<(), ExportError> {
    if *properties == TableCellProperties::default() {
        return Ok(());
    }
    w.write_event(Event::Start(start("w:tcPr"))).map_err(pkg)?;
    if let Some(width) = properties.width_twips {
        let mut el = start("w:tcW");
        el.push_attribute(("w:type", "dxa"));
        el.push_attribute(("w:w", width.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(span) = properties.grid_span {
        let mut el = start("w:gridSpan");
        el.push_attribute(("w:val", span.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(merge) = properties.vertical_merge {
        let mut el = start("w:vMerge");
        // `Continue` is the bare element; `Restart` carries `w:val="restart"`.
        if merge == VerticalMerge::Restart {
            el.push_attribute(("w:val", "restart"));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    write_borders(w, "w:tcBorders", &properties.borders)?;
    write_shading(w, &properties.shading)?;
    write_margins(w, "w:tcMar", &properties.margins)?;
    if let Some(alignment) = properties.vertical_alignment {
        let mut el = start("w:vAlign");
        el.push_attribute((
            "w:val",
            match alignment {
                CellVerticalAlignment::Top => "top",
                CellVerticalAlignment::Center => "center",
                CellVerticalAlignment::Bottom => "bottom",
            },
        ));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if properties.no_wrap {
        w.write_event(Event::Empty(start("w:noWrap")))
            .map_err(pkg)?;
    }
    if let Some(direction) = properties.text_direction {
        let mut el = start("w:textDirection");
        el.push_attribute((
            "w:val",
            match direction {
                TextDirection::LrTb => "lrTb",
                TextDirection::TbRl => "tbRl",
                TextDirection::BtLr => "btLr",
            },
        ));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:tcPr")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits a border set (`w:tblBorders`/`w:tcBorders`) if any edge is present.
fn write_borders(
    w: &mut Writer<Cursor<Vec<u8>>>,
    container: &str,
    borders: &TableBorders,
) -> Result<(), ExportError> {
    if borders.is_empty() {
        return Ok(());
    }
    w.write_event(Event::Start(start(container))).map_err(pkg)?;
    for (edge, name) in [
        (&borders.top, "w:top"),
        (&borders.start, "w:start"),
        (&borders.bottom, "w:bottom"),
        (&borders.end, "w:end"),
        (&borders.inside_h, "w:insideH"),
        (&borders.inside_v, "w:insideV"),
    ] {
        if let Some(edge) = edge {
            write_border_edge(w, name, edge)?;
        }
    }
    w.write_event(Event::End(BytesEnd::new(container)))
        .map_err(pkg)?;
    Ok(())
}

fn write_border_edge(
    w: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
    edge: &BorderEdge,
) -> Result<(), ExportError> {
    let mut el = start(name);
    el.push_attribute(("w:val", edge.style.as_str()));
    if let Some(size) = edge.size_eighth_points {
        el.push_attribute(("w:sz", size.to_string().as_str()));
    }
    if let Some(color) = &edge.color {
        el.push_attribute(("w:color", rgb_hex(color).as_str()));
    }
    if let Some(space) = edge.space_points {
        el.push_attribute(("w:space", space.to_string().as_str()));
    }
    w.write_event(Event::Empty(el)).map_err(pkg)?;
    Ok(())
}

/// Emits cell margins (`w:tblCellMar`/`w:tcMar`) if any edge is present.
fn write_margins(
    w: &mut Writer<Cursor<Vec<u8>>>,
    container: &str,
    margins: &casual_doc_model::v1::CellMargins,
) -> Result<(), ExportError> {
    if margins.is_empty() {
        return Ok(());
    }
    w.write_event(Event::Start(start(container))).map_err(pkg)?;
    for (value, name) in [
        (margins.top_twips, "w:top"),
        (margins.start_twips, "w:start"),
        (margins.bottom_twips, "w:bottom"),
        (margins.end_twips, "w:end"),
    ] {
        if let Some(value) = value {
            let mut el = start(name);
            el.push_attribute(("w:type", "dxa"));
            el.push_attribute(("w:w", value.to_string().as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
    }
    w.write_event(Event::End(BytesEnd::new(container)))
        .map_err(pkg)?;
    Ok(())
}

fn write_shading(
    w: &mut Writer<Cursor<Vec<u8>>>,
    shading: &casual_doc_model::v1::Shading,
) -> Result<(), ExportError> {
    if let Some(fill) = &shading.fill {
        let mut el = start("w:shd");
        el.push_attribute(("w:val", "clear"));
        el.push_attribute(("w:color", "auto"));
        el.push_attribute(("w:fill", rgb_hex(fill).as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    Ok(())
}

fn rgb_hex(color: &RgbColor) -> String {
    format!("{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

fn write_paragraph(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &ParagraphProperties,
    inlines: &[InlineNode],
    ctx: &mut Ctx,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:p"))).map_err(pkg)?;
    write_paragraph_properties(w, properties)?;
    for inline in inlines {
        write_inline(w, inline, ctx, false)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:p")))
        .map_err(pkg)?;
    Ok(())
}

fn write_paragraph_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &ParagraphProperties,
) -> Result<(), ExportError> {
    if *properties == ParagraphProperties::default() {
        return Ok(());
    }
    w.write_event(Event::Start(start("w:pPr"))).map_err(pkg)?;
    if let Some(alignment) = properties.alignment {
        let mut jc = start("w:jc");
        jc.push_attribute(("w:val", alignment_token(alignment)));
        w.write_event(Event::Empty(jc)).map_err(pkg)?;
    }
    if let Some(indent) = &properties.indentation {
        let mut el = start("w:ind");
        if let Some(v) = indent.start_twips {
            el.push_attribute(("w:start", v.to_string().as_str()));
        }
        if let Some(v) = indent.end_twips {
            el.push_attribute(("w:end", v.to_string().as_str()));
        }
        if let Some(v) = indent.first_line_twips {
            el.push_attribute(("w:firstLine", v.to_string().as_str()));
        }
        if let Some(v) = indent.hanging_twips {
            el.push_attribute(("w:hanging", v.to_string().as_str()));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    for (flag, name) in [
        (properties.keep_next, "w:keepNext"),
        (properties.keep_lines, "w:keepLines"),
        (properties.page_break_before, "w:pageBreakBefore"),
        (properties.contextual_spacing, "w:contextualSpacing"),
        (properties.suppress_line_numbers, "w:suppressLineNumbers"),
    ] {
        if flag {
            w.write_event(Event::Empty(start(name))).map_err(pkg)?;
        }
    }
    if let Some(level) = properties.outline_level {
        let mut el = start("w:outlineLvl");
        el.push_attribute(("w:val", level.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:pPr")))
        .map_err(pkg)?;
    Ok(())
}

/// Emits one inline node. `in_deletion` is true when the node sits inside a
/// tracked-change deletion range, so a run's text is written as `w:delText`
/// (which the importer reads back into `run.text`, exactly like `w:t`).
fn write_inline(
    w: &mut Writer<Cursor<Vec<u8>>>,
    inline: &InlineNode,
    ctx: &mut Ctx,
    in_deletion: bool,
) -> Result<(), ExportError> {
    match inline {
        InlineNode::Run(run) => {
            w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
            write_run_properties(w, &run.properties)?;
            let tag = if in_deletion { "w:delText" } else { "w:t" };
            let mut t = start(tag);
            t.push_attribute(("xml:space", "preserve"));
            w.write_event(Event::Start(t)).map_err(pkg)?;
            w.write_event(Event::Text(BytesText::new(&run.text)))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new(tag))).map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:r")))
                .map_err(pkg)?;
        }
        InlineNode::Tab(_) => {
            w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
            w.write_event(Event::Empty(start("w:tab"))).map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:r")))
                .map_err(pkg)?;
        }
        InlineNode::Break(node) => {
            w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
            let mut br = start("w:br");
            br.push_attribute(("w:type", break_token(node.kind)));
            w.write_event(Event::Empty(br)).map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:r")))
                .map_err(pkg)?;
        }
        // A hyperlink is a direct inline child of `w:p` (never inside a `w:r`):
        // an external target resolves through an `r:id` relationship, an internal
        // one through a `w:anchor` bookmark name.
        InlineNode::Hyperlink(link) => {
            let mut el = start("w:hyperlink");
            match &link.target {
                HyperlinkTarget::External(ext) => {
                    let rid = ctx.rels.hyperlink(&ext.url);
                    el.push_attribute(("r:id", rid.as_str()));
                    if let Some(tip) = &link.tooltip {
                        el.push_attribute(("w:tooltip", tip.as_str()));
                    }
                    w.write_event(Event::Start(el)).map_err(pkg)?;
                }
                HyperlinkTarget::Internal(int) => {
                    el.push_attribute(("w:anchor", int.anchor.as_str()));
                    if let Some(tip) = &link.tooltip {
                        el.push_attribute(("w:tooltip", tip.as_str()));
                    }
                    w.write_event(Event::Start(el)).map_err(pkg)?;
                }
            }
            for child in &link.inlines {
                write_inline(w, child, ctx, in_deletion)?;
            }
            w.write_event(Event::End(BytesEnd::new("w:hyperlink")))
                .map_err(pkg)?;
        }
        // A simple field: the instruction inline, the cached result as children.
        InlineNode::Field(field) => {
            let mut el = start("w:fldSimple");
            el.push_attribute(("w:instr", field.instruction.as_str()));
            w.write_event(Event::Start(el)).map_err(pkg)?;
            for child in &field.inlines {
                write_inline(w, child, ctx, in_deletion)?;
            }
            w.write_event(Event::End(BytesEnd::new("w:fldSimple")))
                .map_err(pkg)?;
        }
        // A tracked-change range. Its own runs are deleted text when this is a
        // deletion (or when already inside one); insertions keep the flag.
        InlineNode::Revision(revision) => {
            let (name, deleted) = match revision.kind {
                RevisionKind::Insertion => ("w:ins", in_deletion),
                RevisionKind::Deletion => ("w:del", true),
            };
            let mut el = start(name);
            if let Some(author) = &revision.author {
                el.push_attribute(("w:author", author.as_str()));
            }
            if let Some(date) = &revision.date {
                el.push_attribute(("w:date", date.as_str()));
            }
            if let Some(id) = &revision.revision_id {
                el.push_attribute(("w:id", id.as_str()));
            }
            w.write_event(Event::Start(el)).map_err(pkg)?;
            for child in &revision.inlines {
                write_inline(w, child, ctx, deleted)?;
            }
            w.write_event(Event::End(BytesEnd::new(name))).map_err(pkg)?;
        }
        // A bookmark range marker (zero-width). The pairing key `w:id` derives
        // from the shared `BookmarkId`; the name lives in `Definitions`.
        InlineNode::BookmarkStart(marker) => {
            if let Some(bookmark) = ctx.defs.bookmarks.get(&marker.bookmark) {
                let id = marker.bookmark.node_id().as_u128().to_string();
                let mut el = start("w:bookmarkStart");
                el.push_attribute(("w:id", id.as_str()));
                el.push_attribute(("w:name", bookmark.name.as_str()));
                w.write_event(Event::Empty(el)).map_err(pkg)?;
            }
        }
        InlineNode::BookmarkEnd(marker) => {
            let id = marker.bookmark.node_id().as_u128().to_string();
            let mut el = start("w:bookmarkEnd");
            el.push_attribute(("w:id", id.as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
        // An inline content control: typed `w:sdtPr` (omitted when default) plus
        // `w:sdtContent` wrapping the children.
        InlineNode::Sdt(sdt) => {
            w.write_event(Event::Start(start("w:sdt"))).map_err(pkg)?;
            write_sdt_properties(w, &sdt.properties)?;
            w.write_event(Event::Start(start("w:sdtContent")))
                .map_err(pkg)?;
            for child in &sdt.inlines {
                write_inline(w, child, ctx, in_deletion)?;
            }
            w.write_event(Event::End(BytesEnd::new("w:sdtContent")))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:sdt")))
                .map_err(pkg)?;
        }
        // Drawings, text boxes, and note/comment references depend on media or
        // definition parts written in a later slice (P1B-005).
        InlineNode::Drawing(_)
        | InlineNode::TextBox(_)
        | InlineNode::NoteReference(_)
        | InlineNode::CommentReference(_) => {}
    }
    Ok(())
}

/// Emits `w:sdtPr` for an inline content control, or nothing when the control
/// carries no modeled properties. Children are ordered per `CT_SdtPr` (alias,
/// tag, id, type marker) for Word validity; the importer is order-independent.
fn write_sdt_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &SdtProperties,
) -> Result<(), ExportError> {
    if *properties == SdtProperties::default() {
        return Ok(());
    }
    w.write_event(Event::Start(start("w:sdtPr"))).map_err(pkg)?;
    for (value, name) in [
        (&properties.alias, "w:alias"),
        (&properties.tag, "w:tag"),
        (&properties.control_id, "w:id"),
    ] {
        if let Some(value) = value {
            let mut el = start(name);
            el.push_attribute(("w:val", value.as_str()));
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
    }
    if let Some(kind) = properties.control_kind {
        w.write_event(Event::Empty(start(sdt_kind_element(kind))))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:sdtPr")))
        .map_err(pkg)?;
    Ok(())
}

/// Maps a content-control kind to its `w:sdtPr` type-marker element. The
/// importer matches by local name, so the `w:`-prefixed spelling round-trips
/// (`Checkbox` re-imports even though real OOXML uses `w14:checkbox`).
fn sdt_kind_element(kind: SdtControlKind) -> &'static str {
    match kind {
        SdtControlKind::RichText => "w:richText",
        SdtControlKind::PlainText => "w:text",
        SdtControlKind::ComboBox => "w:comboBox",
        SdtControlKind::DropDownList => "w:dropDownList",
        SdtControlKind::Date => "w:date",
        SdtControlKind::Picture => "w:picture",
        SdtControlKind::Checkbox => "w:checkbox",
        SdtControlKind::Group => "w:group",
        SdtControlKind::BuildingBlockGallery => "w:docPartObj",
        SdtControlKind::RepeatingSection => "w:repeatingSection",
        SdtControlKind::Citation => "w:citation",
        SdtControlKind::Bibliography => "w:bibliography",
    }
}

fn write_run_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    properties: &RunProperties,
) -> Result<(), ExportError> {
    if *properties == RunProperties::default() {
        return Ok(());
    }
    w.write_event(Event::Start(start("w:rPr"))).map_err(pkg)?;
    for (value, name) in [
        (properties.bold, "w:b"),
        (properties.italic, "w:i"),
        (properties.strike, "w:strike"),
    ] {
        if let Some(on) = value {
            let mut el = start(name);
            // A `CT_OnOff` toggle: present means on; `w:val="0"` is an explicit
            // off (the importer's `is_true`).
            if !on {
                el.push_attribute(("w:val", "0"));
            }
            w.write_event(Event::Empty(el)).map_err(pkg)?;
        }
    }
    if let Some(on) = properties.underline {
        // Underline is NOT a plain toggle: the importer maps `w:u@val != "none"`
        // to on. Emit a bare `w:u` for on and `w:val="none"` for an explicit off.
        let mut el = start("w:u");
        if !on {
            el.push_attribute(("w:val", "none"));
        }
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(Color::Rgb(rgb)) = &properties.color {
        let mut el = start("w:color");
        el.push_attribute((
            "w:val",
            format!("{:02X}{:02X}{:02X}", rgb.r, rgb.g, rgb.b).as_str(),
        ));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    if let Some(size) = properties.size_half_points {
        let mut el = start("w:sz");
        el.push_attribute(("w:val", size.to_string().as_str()));
        w.write_event(Event::Empty(el)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("w:rPr")))
        .map_err(pkg)?;
    Ok(())
}

fn alignment_token(alignment: Alignment) -> &'static str {
    match alignment {
        Alignment::Start => "start",
        Alignment::Center => "center",
        Alignment::End => "end",
        Alignment::Justify => "both",
    }
}

fn break_token(kind: BreakKind) -> &'static str {
    match kind {
        BreakKind::Line => "textWrapping",
        BreakKind::Page => "page",
        BreakKind::Column => "column",
    }
}

fn pkg<E>(_: E) -> ExportError {
    ExportError::Package
}
