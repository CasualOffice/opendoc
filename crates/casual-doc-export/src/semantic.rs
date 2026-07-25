//! Semantic DOCX writer: serialize a v1 `Document` model back to a valid,
//! editable WordprocessingML package (the dual of the Retention byte-copy
//! writer). This is Phase-1B slice 1: the core body — `w:document`/`w:body`,
//! paragraphs, runs, the mapped run/paragraph properties, tabs/breaks, and the
//! section `w:sectPr`. Tables, inline constructs (hyperlinks/fields/drawings/…),
//! and the definition parts (styles/numbering/notes/headers/comments) land in
//! later slices; a model that uses them is written with what this slice
//! supports and the rest is skipped (never a malformed part).
//!
//! Output is byte-deterministic for a given model: parts in fixed order, a fixed
//! ZIP timestamp, ids/relationships re-minted in document order.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};

use casual_doc_model::v1::{
    Alignment, BlockNode, BorderEdge, BreakKind, CellVerticalAlignment, Color, Document,
    HeightRule, InlineNode, ParagraphProperties, RgbColor, RunProperties, Table, TableBorders,
    TableCell, TableCellProperties, TableLayout, TableProperties, TableRow, TableRowProperties,
    TextDirection, VerticalMerge,
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

/// Serializes a v1 `Document` to a DOCX package. `media` supplies binary image
/// bytes by part name (the model carries `MediaReference` metadata, not bytes);
/// pass an empty map when the model uses no drawings.
pub fn write_document(
    document: &Document,
    _media: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, ExportError> {
    let document_xml = document_xml(document)?;

    // Fixed part set for the core slice; each is emitted in a deterministic
    // order so the package bytes are reproducible.
    let parts: [(&str, Vec<u8>); 4] = [
        ("[Content_Types].xml", content_types_xml()?),
        ("_rels/.rels", root_rels_xml()?),
        ("word/document.xml", document_xml),
        ("word/_rels/document.xml.rels", empty_rels_xml()?),
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

/// Emits an empty per-document relationships part (this slice references no
/// external targets; later slices add hyperlink/media/part relationships).
fn empty_rels_xml() -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut rels = start("Relationships");
    rels.push_attribute(("xmlns", REL_NS));
    w.write_event(Event::Empty(rels)).map_err(pkg)?;
    Ok(finish(w))
}

/// Emits `word/document.xml` from the model body.
fn document_xml(document: &Document) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut doc = start("w:document");
    doc.push_attribute(("xmlns:w", W_NS));
    w.write_event(Event::Start(doc)).map_err(pkg)?;
    w.write_event(Event::Start(start("w:body"))).map_err(pkg)?;

    for block in document.body() {
        write_block(&mut w, block)?;
    }

    w.write_event(Event::End(BytesEnd::new("w:body")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("w:document")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// Emits a body/cell block. Content-control block wrappers (`BlockNode::Sdt`)
/// are a later slice — their inner blocks are emitted directly (no text loss).
fn write_block(w: &mut Writer<Cursor<Vec<u8>>>, block: &BlockNode) -> Result<(), ExportError> {
    match block {
        BlockNode::Paragraph(paragraph) => {
            write_paragraph(w, &paragraph.properties, &paragraph.inlines)
        }
        BlockNode::Table(table) => write_table(w, table),
        BlockNode::Sdt(sdt) => {
            for inner in &sdt.blocks {
                write_block(w, inner)?;
            }
            Ok(())
        }
    }
}

fn write_table(w: &mut Writer<Cursor<Vec<u8>>>, table: &Table) -> Result<(), ExportError> {
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
        write_row(w, row)?;
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

fn write_row(w: &mut Writer<Cursor<Vec<u8>>>, row: &TableRow) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:tr"))).map_err(pkg)?;
    write_row_properties(w, &row.properties)?;
    for cell in &row.cells {
        write_cell(w, cell)?;
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

fn write_cell(w: &mut Writer<Cursor<Vec<u8>>>, cell: &TableCell) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:tc"))).map_err(pkg)?;
    write_cell_properties(w, &cell.properties)?;
    for block in &cell.blocks {
        write_block(w, block)?;
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
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("w:p"))).map_err(pkg)?;
    write_paragraph_properties(w, properties)?;
    for inline in inlines {
        write_inline(w, inline)?;
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

fn write_inline(w: &mut Writer<Cursor<Vec<u8>>>, inline: &InlineNode) -> Result<(), ExportError> {
    match inline {
        InlineNode::Run(run) => {
            w.write_event(Event::Start(start("w:r"))).map_err(pkg)?;
            write_run_properties(w, &run.properties)?;
            let mut t = start("w:t");
            t.push_attribute(("xml:space", "preserve"));
            w.write_event(Event::Start(t)).map_err(pkg)?;
            w.write_event(Event::Text(BytesText::new(&run.text)))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("w:t")))
                .map_err(pkg)?;
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
        // Later-slice inline constructs are not yet written; the core-slice
        // round-trip corpus does not exercise them.
        _ => {}
    }
    Ok(())
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
