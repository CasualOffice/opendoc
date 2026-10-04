// SPDX-License-Identifier: Apache-2.0

//! `p:graphicFrame` with an `a:tbl` inside it, and `ppt/tableStyles.xml`.
//!
//! # Why a frame is written as a frame
//!
//! A table's frame imports as a `GroupChild::Shape` — an unpainted box — because
//! that is what lets the shared placement walk compute its rectangle with no
//! table arm in it (`casual-pres-import`'s `table` module carries the argument).
//! If this writer followed the model's enum it would therefore write a `p:sp`,
//! and a deck would lose its tables on the first save while every geometry guard
//! stayed green. So the element name is decided by `SlideNode::table`, not by the
//! `GroupChild` variant: a node carrying a table is a `p:graphicFrame`, and the
//! box comes out as the `p:xfrm` it went in as.
//!
//! `p:xfrm`, not `a:xfrm` — a reader looking for `a:xfrm` inside a
//! `p:graphicFrame` finds nothing, which makes a frame written with the wrong
//! prefix a table at the origin with no size.
//!
//! # Both encodings of a merge come back out
//!
//! A covered cell is still written, with its `@hMerge`/`@vMerge`, because a row
//! must hold one `a:tc` per `a:gridCol` to be well-formed. The origin keeps its
//! `@gridSpan`/`@rowSpan`. The two come from one [`CellMerge`] per axis, so the
//! writer cannot emit a cell that is both, and the round trip is symmetric with
//! the reader by construction rather than by two lists staying in step.
//!
//! # What is NOT written back, and is lost rather than invented
//!
//! A table style's formatting parts. The model carries a style's GUID and its
//! name and nothing else, so `tableStyles.xml` comes out with its `@def` and its
//! `a:tblStyle` entries and each entry is EMPTY. That is a real loss and it is
//! why `casual-pres-import` reports `a:wholeTbl`, `a:band1H` and their siblings:
//! writing a plausible `a:wholeTbl` from nothing would be inventing a design.
//! `export_pptx_retaining` carries the original part through, which is the call
//! an application should make.

use casual_pres_model::{
    CellMerge, SlideNode, SlideTable, TableCell, TableRow, TableStyles, TextAnchor,
};

use crate::text;

/// One `p:graphicFrame` holding an `a:tbl`.
///
/// `shape_id` is the producer-scoped `p:cNvPr@id`, assigned by position like
/// every other child of the tree.
///
/// # Complexity
///
/// O(cells), one pass.
pub(crate) fn graphic_frame_xml(node: &SlideNode, table: &SlideTable, shape_id: usize) -> String {
    let mut xml = format!(
        "<p:graphicFrame><p:nvGraphicFramePr>{}<p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr>",
        crate::shapes::non_visual_properties(shape_id, node.name.as_deref(), "Table", node.hidden)
    );
    // The frame's box, from the unpainted `GroupChild::Shape` the importer put it
    // in. A frame has no geometry, no fill and no outline, so none is written.
    let (offset, extent) = match &node.content {
        casual_doc_model::v1::GroupChild::Shape(shape) => (shape.offset, shape.extent),
        // A table on any other child kind cannot be produced by the importer and
        // is not a shape this writer can place. Written at the origin with no
        // extent rather than dropped: a table a host attached to a picture is
        // still the host's data.
        _ => (
            casual_doc_model::v1::PointEmu { x_emu: 0, y_emu: 0 },
            casual_doc_model::v1::Extent {
                width_emu: 0,
                height_emu: 0,
            },
        ),
    };
    xml.push_str(&format!(
        r#"<p:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></p:xfrm>"#,
        offset.x_emu, offset.y_emu, extent.width_emu, extent.height_emu
    ));
    xml.push_str(&format!(
        r#"<a:graphic><a:graphicData uri="{TABLE_GRAPHIC_URI}">"#
    ));
    xml.push_str(&table_xml(table));
    xml.push_str("</a:graphicData></a:graphic></p:graphicFrame>");
    xml
}

/// The `a:graphicData@uri` that declares a table payload.
///
/// Written even though this reader dispatches on the payload ELEMENT rather than
/// on the uri: PowerPoint dispatches on the uri, so a frame written without it
/// opens as an empty object.
const TABLE_GRAPHIC_URI: &str = "http://schemas.openxmlformats.org/drawingml/2006/table";

/// `a:tbl`.
fn table_xml(table: &SlideTable) -> String {
    let mut xml = String::from("<a:tbl>");
    xml.push_str(&table_properties_xml(table));
    xml.push_str("<a:tblGrid>");
    for column in &table.grid {
        // EMU, verbatim. A grid is the only place an `a:tbl` states a width.
        xml.push_str(&format!(r#"<a:gridCol w="{}"/>"#, column.width_emu));
    }
    xml.push_str("</a:tblGrid>");
    for row in &table.rows {
        xml.push_str(&row_xml(row));
    }
    xml.push_str("</a:tbl>");
    xml
}

/// `a:tblPr`, with each flag written only when it is set.
///
/// A flag at its schema default is omitted rather than written as `="0"`, which
/// is what PowerPoint does — and the asymmetry matters for exactly one reason:
/// `<a:tblPr/>` and `<a:tblPr firstRow="0" .../>` mean the same thing, so writing
/// the long form on every table would put six attributes into every diff of every
/// package this engine touches.
fn table_properties_xml(table: &SlideTable) -> String {
    let properties = &table.properties;
    let mut xml = String::from("<a:tblPr");
    for (name, set) in [
        ("firstRow", properties.first_row),
        ("lastRow", properties.last_row),
        ("firstCol", properties.first_column),
        ("lastCol", properties.last_column),
        ("bandRow", properties.banded_rows),
        ("bandCol", properties.banded_columns),
        ("rtl", properties.rtl),
    ] {
        if set {
            xml.push_str(&format!(r#" {name}="1""#));
        }
    }
    match properties.style_id.as_deref() {
        Some(id) => {
            // The GUID with its braces, exactly as it arrived. Escaped because it
            // is character data, even though a conformant GUID has nothing in it
            // to escape: a host that put something else in this field must not be
            // able to write malformed XML through it.
            xml.push_str(&format!(
                "><a:tableStyleId>{}</a:tableStyleId></a:tblPr>",
                text::escape(id)
            ));
        }
        None => xml.push_str("/>"),
    }
    xml
}

/// `a:tr`.
fn row_xml(row: &TableRow) -> String {
    // `@h` is required on an `a:tr` and is a MINIMUM height, in EMU.
    let mut xml = format!(r#"<a:tr h="{}">"#, row.height_emu);
    for cell in &row.cells {
        xml.push_str(&cell_xml(cell));
    }
    xml.push_str("</a:tr>");
    xml
}

/// `a:tc`, with the merge attributes its role calls for.
fn cell_xml(cell: &TableCell) -> String {
    let mut xml = String::from("<a:tc");
    for (merge, span_attribute, continuation_attribute) in [
        (cell.horizontal, "gridSpan", "hMerge"),
        (cell.vertical, "rowSpan", "vMerge"),
    ] {
        match merge {
            // An origin states its span and NEVER the continuation marker.
            CellMerge::Origin(span) => xml.push_str(&format!(r#" {span_attribute}="{span}""#)),
            // A covered cell states the marker and NEVER a span. It is still
            // written, because a row must hold one `a:tc` per `a:gridCol`.
            CellMerge::Continuation => xml.push_str(&format!(r#" {continuation_attribute}="1""#)),
            CellMerge::None => {}
        }
    }
    xml.push('>');
    // `CT_TableCell` requires `a:txBody` before `a:tcPr`, and a cell with no text
    // gets the minimal valid body rather than being written invalid.
    match cell.text.as_ref() {
        Some(body) => xml.push_str(&text::text_body_xml_named(body, "a:txBody")),
        None => xml.push_str("<a:txBody><a:bodyPr/><a:lstStyle/><a:p/></a:txBody>"),
    }
    xml.push_str(&cell_properties_xml(cell));
    xml.push_str("</a:tc>");
    xml
}

/// `a:tcPr`: the four EMU margins, the anchor, the borders and the fill.
///
/// The margins are written unconditionally. They are not defaulted to zero in the
/// model — `TableCellProperties::default` is DrawingML's own 91440/45720 — so
/// omitting one that happens to equal the default would be correct but would make
/// the writer's output depend on a comparison with a constant; writing all four
/// means a round trip cannot change a margin by changing a default.
fn cell_properties_xml(cell: &TableCell) -> String {
    let properties = &cell.properties;
    let mut xml = format!(
        r#"<a:tcPr marL="{}" marR="{}" marT="{}" marB="{}" anchor="{}""#,
        properties.margin_left_emu,
        properties.margin_right_emu,
        properties.margin_top_emu,
        properties.margin_bottom_emu,
        anchor_token(properties.anchor)
    );
    // `CT_TableCellProperties` fixes the order: the four lines, then the diagonals
    // (which the model does not carry), then the fill. Written out of order the
    // part is schema-invalid and PowerPoint repairs it, which loses the cell.
    let borders: String = [
        ("a:lnL", properties.border_left.as_ref()),
        ("a:lnR", properties.border_right.as_ref()),
        ("a:lnT", properties.border_top.as_ref()),
        ("a:lnB", properties.border_bottom.as_ref()),
    ]
    .into_iter()
    .filter_map(|(tag, stroke)| {
        stroke.map(|stroke| crate::shapes::line_properties_xml(tag, stroke))
    })
    .collect();
    let fill = properties
        .fill
        .as_ref()
        .map(crate::shapes::cell_fill_xml)
        .unwrap_or_default();
    if borders.is_empty() && fill.is_empty() {
        xml.push_str("/>");
    } else {
        xml.push_str(&format!(">{borders}{fill}</a:tcPr>"));
    }
    xml
}

/// `a:tcPr@anchor`'s token.
fn anchor_token(anchor: TextAnchor) -> &'static str {
    anchor.token()
}

/// `ppt/tableStyles.xml`.
///
/// Written whenever the deck carries one, and that includes a deck whose only
/// table-style fact is the `@def` GUID — which is the common case, because
/// PowerPoint writes `<a:tblStyleLst def="{GUID}"/>` into every package.
///
/// # Complexity
///
/// O(styles).
pub(crate) fn table_styles_part(styles: &TableStyles) -> Vec<u8> {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:tblStyleLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#,
    );
    if let Some(default) = styles.default_style_id.as_deref() {
        xml.push_str(&format!(r#" def="{}""#, text::escape(default)));
    }
    if styles.styles.is_empty() {
        xml.push_str("/>");
        return xml.into_bytes();
    }
    xml.push('>');
    for style in &styles.styles {
        // Self-closing, and that is the loss this writer declares rather than
        // papers over: the model carries no formatting part, so an entry comes
        // back as an id and a name with nothing inside it.
        xml.push_str(&format!(
            r#"<a:tblStyle styleId="{}""#,
            text::escape(&style.id)
        ));
        if let Some(name) = style.name.as_deref() {
            xml.push_str(&format!(r#" styleName="{}""#, text::escape(name)));
        }
        xml.push_str("/>");
    }
    xml.push_str("</a:tblStyleLst>");
    xml.into_bytes()
}
