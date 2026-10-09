// SPDX-License-Identifier: Apache-2.0

//! The **values-only workbook** behind a chart this editor wrote
//! (`docs/155` §5.3).
//!
//! # Why a chart part is not enough
//!
//! A DrawingML chart carries its numbers twice: in the part's own cache, which
//! is what every reader paints, and in an embedded workbook the part names with
//! `c:externalData`, which is what Word opens when the reader chooses *Edit
//! Data*. A chart with no workbook renders correctly everywhere and then cannot
//! be edited in Word — "the linked file isn't available" — so a document
//! authored here and handed to a Word user arrives with charts that look live
//! and are not. And a chart whose cache was edited while its workbook was not
//! shows one set of numbers on the page and another the moment Word opens the
//! data: precisely the silent inconsistency `docs/155` §5.3 forbids.
//!
//! So every chart the exporter REGENERATES gets a workbook holding exactly its
//! cache, and its series name ranges in that workbook with `c:f` formulas —
//! Word's own layout: row 1 is the series names, column A the categories (or the
//! x values of a scatter chart), one column per series beside it.
//!
//! # What this is not
//!
//! Not a spreadsheet engine and not the opencalc side of the line: no formulas
//! inside the sheet, no styles beyond the one Excel requires, no evaluation. It
//! is a serializer for a rectangular array of strings and numbers, written in the
//! same shape `docs/155` §5.3 already agreed to.
//!
//! # When it declines
//!
//! [`bind`] returns `None`, and the chart is written cache-only exactly as before,
//! when the grid cannot describe the chart: more than one plot group (a combo
//! chart), a range that already names a formula (that formula points into a
//! workbook this module did not write), series that do not share one category
//! column, or a workbook part name already taken by something else in the
//! package. Declining is safe — the cache is still the rendering truth.
//!
//! # Complexity
//!
//! O(series × rows) for one chart, bounded by the model's chart ceilings.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};

use casual_doc_import::RetainedParts;
use casual_doc_model::strip_xml_forbidden;
use casual_doc_model::v1::{Chart, ChartText, ChartValue, DataRange, EmbeddedPart};
use quick_xml::events::{BytesEnd, BytesText, Event};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::ExportError;
use crate::semantic::{finish, new_writer, pkg, start};

/// The content type of an embedded `.xlsx` package part.
pub(crate) const WORKBOOK_CT: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// The relationship type a chart part names its embedded workbook by.
pub(crate) const PACKAGE_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/package";

/// The one sheet's name, which every `c:f` formula below quotes.
const SHEET: &str = "Sheet1";

const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";

/// A workbook this export wrote, and the part it lands at.
pub(crate) struct GeneratedWorkbook {
    /// The package part name, e.g. `word/embeddings/Microsoft_Excel_Worksheet_chart7.xlsx`.
    pub(crate) part_name: String,
    /// The `.xlsx` bytes.
    pub(crate) bytes: Vec<u8>,
    /// Whether this REPLACES a workbook the source package carried, which is a
    /// loss the report has to name: the producer's workbook may have held more
    /// than the chart showed.
    pub(crate) replaces_retained: bool,
}

/// `chart` rebound to a workbook holding its cache, and that workbook — or
/// `None` when the grid cannot describe the chart (this module's header).
///
/// `chart_part` is the chart's own part name; a chart that already names a
/// workbook keeps that part name and relationship id, so the workbook is
/// replaced IN PLACE and nothing in the package is left pointing at the old one.
///
/// Complexity: O(series × rows), plus O(retained parts) for the name check.
pub(crate) fn bind(
    chart: &Chart,
    chart_part: &str,
    retained_parts: &RetainedParts,
) -> Result<Option<(Chart, GeneratedWorkbook)>, ExportError> {
    let [group] = chart.plot_area.groups.as_slice() else {
        return Ok(None);
    };
    let Some(first) = group.series.first() else {
        return Ok(None);
    };
    let scatter = first.x_values.is_some();
    let labels_of = |series: &casual_doc_model::v1::Series| {
        if scatter {
            series.x_values.clone()
        } else {
            series.categories.clone()
        }
    };
    let labels = labels_of(first);
    let has_formula =
        |range: Option<&DataRange>| range.is_some_and(|range| range.formula.is_some());
    for series in &group.series {
        if has_formula(Some(&series.values))
            || has_formula(series.categories.as_ref())
            || has_formula(series.x_values.as_ref())
            || series
                .name
                .as_ref()
                .is_some_and(|name| name.formula.is_some())
            // Custom error-bar lengths that name cells name them in the
            // producer's workbook, which this one would replace; the sheet
            // written here has no error-bar columns to point them at instead.
            || series
                .error_bars
                .iter()
                .any(|bars| has_formula(bars.plus.as_ref()) || has_formula(bars.minus.as_ref()))
            || labels_of(series).map(|range| range.points)
                != labels.as_ref().map(|range| range.points.clone())
        {
            return Ok(None);
        }
    }
    let rows = group
        .series
        .iter()
        .map(|series| series.values.point_count)
        .chain(labels.iter().map(|range| range.point_count))
        .max()
        .unwrap_or(0);
    if rows == 0 {
        return Ok(None);
    }

    let (part_name, relationship_id, replaces_retained) = match &chart.external_data {
        Some(existing) => (
            existing.part_name.clone(),
            existing.relationship_id.clone(),
            retained_parts
                .parts
                .iter()
                .any(|part| part.part_name == existing.part_name),
        ),
        None => {
            let stem = chart_part
                .rsplit('/')
                .next()
                .unwrap_or(chart_part)
                .trim_end_matches(".xml");
            let name = format!("word/embeddings/Microsoft_Excel_Worksheet_{stem}.xlsx");
            if retained_parts
                .parts
                .iter()
                .any(|part| part.part_name == name)
            {
                return Ok(None);
            }
            (name, "rId1".to_owned(), false)
        }
    };

    let last = rows + 1; // row 1 holds the series names
    let mut bound = chart.clone();
    for (column, series) in bound.plot_area.groups[0].series.iter_mut().enumerate() {
        let letter = column_letter(column + 1);
        series.values.formula = Some(format!("{SHEET}!${letter}$2:${letter}${last}"));
        let label_formula = format!("{SHEET}!$A$2:$A${last}");
        if let Some(range) = series.categories.as_mut() {
            range.formula = Some(label_formula.clone());
        }
        if let Some(range) = series.x_values.as_mut() {
            range.formula = Some(label_formula);
        }
        // A series with no name gets none in the sheet either: inventing
        // "Series 1" would put a name in Word's data sheet that the chart, and
        // the reader, never had.
        if let Some(name) = series.name.as_mut() {
            name.formula = Some(format!("{SHEET}!${letter}$1"));
        }
    }
    let bytes = write_workbook(&bound, rows)?;
    bound.external_data = Some(EmbeddedPart {
        relationship_id,
        relationship_type: PACKAGE_REL_TYPE.to_owned(),
        part_name: part_name.clone(),
    });
    Ok(Some((
        bound,
        GeneratedWorkbook {
            part_name,
            bytes,
            replaces_retained,
        },
    )))
}

/// The spreadsheet column name for a 0-based index: 0 -> `A`, 25 -> `Z`,
/// 26 -> `AA`. O(log index).
pub(crate) fn column_letter(index: usize) -> String {
    let mut n = index + 1;
    let mut out = Vec::new();
    while n > 0 {
        let rem = (n - 1) % 26;
        out.push(b'A' + u8::try_from(rem).unwrap_or(0));
        n = (n - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// One sheet cell: a number, a string, or nothing at all.
enum Cell<'a> {
    Number(&'a str),
    Text(&'a str),
}

fn cell_of(value: &ChartValue) -> Option<Cell<'_>> {
    match value {
        ChartValue::Number(text) => Some(Cell::Number(text)),
        ChartValue::Text(text) => Some(Cell::Text(text)),
        ChartValue::Blank => None,
    }
}

/// The `.xlsx` package for a bound chart: five parts, stored, with a fixed
/// timestamp so two exports of one document are byte-identical.
fn write_workbook(chart: &Chart, rows: u32) -> Result<Vec<u8>, ExportError> {
    let parts: [(&str, Vec<u8>); 6] = [
        ("[Content_Types].xml", content_types()?),
        ("_rels/.rels", package_rels()?),
        ("xl/workbook.xml", workbook()?),
        ("xl/_rels/workbook.xml.rels", workbook_rels()?),
        ("xl/styles.xml", styles()?),
        ("xl/worksheets/sheet1.xml", sheet(chart, rows)?),
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

/// A range's points keyed by index, so a sparse cache is looked up per row
/// rather than scanned per cell. O(points).
fn by_index(range: Option<&DataRange>) -> BTreeMap<u32, &ChartValue> {
    range
        .map(|range| {
            range
                .points
                .iter()
                .map(|(at, value)| (*at, value))
                .collect()
        })
        .unwrap_or_default()
}

/// `xl/worksheets/sheet1.xml`: row 1 the series names, then one row per
/// category. Numbers keep their verbatim lexical form — the same text the chart
/// cache holds — so the sheet and the cache agree to the character.
fn sheet(chart: &Chart, rows: u32) -> Result<Vec<u8>, ExportError> {
    let group = &chart.plot_area.groups[0];
    let first = &group.series[0];
    let labels = first.x_values.as_ref().or(first.categories.as_ref());
    let label_cells = by_index(labels);
    let columns: Vec<BTreeMap<u32, &ChartValue>> = group
        .series
        .iter()
        .map(|series| by_index(Some(&series.values)))
        .collect();

    let mut w = new_writer();
    let mut root = start("worksheet");
    root.push_attribute(("xmlns", MAIN_NS));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    w.write_event(Event::Start(start("sheetData")))
        .map_err(pkg)?;

    w.write_event(Event::Start(row_start(1))).map_err(pkg)?;
    for (column, series) in group.series.iter().enumerate() {
        if let Some(ChartText { text, .. }) = &series.name {
            write_cell(
                &mut w,
                &format!("{}1", column_letter(column + 1)),
                &Cell::Text(text),
            )?;
        }
    }
    w.write_event(Event::End(BytesEnd::new("row")))
        .map_err(pkg)?;

    for at in 0..rows {
        let row = at + 2;
        w.write_event(Event::Start(row_start(row))).map_err(pkg)?;
        if let Some(cell) = label_cells.get(&at).and_then(|value| cell_of(value)) {
            write_cell(&mut w, &format!("A{row}"), &cell)?;
        }
        for (column, values) in columns.iter().enumerate() {
            if let Some(cell) = values.get(&at).and_then(|value| cell_of(value)) {
                write_cell(
                    &mut w,
                    &format!("{}{row}", column_letter(column + 1)),
                    &cell,
                )?;
            }
        }
        w.write_event(Event::End(BytesEnd::new("row")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("sheetData")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("worksheet")))
        .map_err(pkg)?;
    Ok(finish(w))
}

fn row_start(row: u32) -> quick_xml::events::BytesStart<'static> {
    let mut element = quick_xml::events::BytesStart::new("row");
    element.push_attribute(("r", row.to_string().as_str()));
    element
}

/// One `c`: a number as `<v>`, a string as an inline string, which needs no
/// shared-strings part.
fn write_cell(
    w: &mut quick_xml::Writer<Cursor<Vec<u8>>>,
    reference: &str,
    cell: &Cell<'_>,
) -> Result<(), ExportError> {
    let mut element = start("c");
    element.push_attribute(("r", reference));
    match cell {
        Cell::Number(text) => {
            w.write_event(Event::Start(element)).map_err(pkg)?;
            w.write_event(Event::Start(start("v"))).map_err(pkg)?;
            w.write_event(Event::Text(BytesText::new(text.trim())))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("v"))).map_err(pkg)?;
        }
        Cell::Text(text) => {
            element.push_attribute(("t", "inlineStr"));
            w.write_event(Event::Start(element)).map_err(pkg)?;
            w.write_event(Event::Start(start("is"))).map_err(pkg)?;
            let mut t = start("t");
            t.push_attribute(("xml:space", "preserve"));
            w.write_event(Event::Start(t)).map_err(pkg)?;
            w.write_event(Event::Text(BytesText::new(&strip_xml_forbidden(text))))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("t"))).map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("is")))
                .map_err(pkg)?;
        }
    }
    w.write_event(Event::End(BytesEnd::new("c"))).map_err(pkg)
}

fn content_types() -> Result<Vec<u8>, ExportError> {
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
    for (part, ct) in [
        (
            "/xl/workbook.xml",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
        ),
        (
            "/xl/worksheets/sheet1.xml",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml",
        ),
        (
            "/xl/styles.xml",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml",
        ),
    ] {
        let mut o = start("Override");
        o.push_attribute(("PartName", part));
        o.push_attribute(("ContentType", ct));
        w.write_event(Event::Empty(o)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("Types")))
        .map_err(pkg)?;
    Ok(finish(w))
}

fn relationships(entries: &[(&str, &str, &str)]) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut root = start("Relationships");
    root.push_attribute(("xmlns", PKG_REL_NS));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    for (id, kind, target) in entries {
        let mut rel = start("Relationship");
        rel.push_attribute(("Id", *id));
        rel.push_attribute(("Type", *kind));
        rel.push_attribute(("Target", *target));
        w.write_event(Event::Empty(rel)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("Relationships")))
        .map_err(pkg)?;
    Ok(finish(w))
}

fn package_rels() -> Result<Vec<u8>, ExportError> {
    relationships(&[(
        "rId1",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
        "xl/workbook.xml",
    )])
}

fn workbook_rels() -> Result<Vec<u8>, ExportError> {
    relationships(&[
        (
            "rId1",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet",
            "worksheets/sheet1.xml",
        ),
        (
            "rId2",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles",
            "styles.xml",
        ),
    ])
}

fn workbook() -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut root = start("workbook");
    root.push_attribute(("xmlns", MAIN_NS));
    root.push_attribute(("xmlns:r", REL_NS));
    w.write_event(Event::Start(root)).map_err(pkg)?;
    w.write_event(Event::Start(start("sheets"))).map_err(pkg)?;
    let mut sheet = start("sheet");
    sheet.push_attribute(("name", SHEET));
    sheet.push_attribute(("sheetId", "1"));
    sheet.push_attribute(("r:id", "rId1"));
    w.write_event(Event::Empty(sheet)).map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("sheets")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("workbook")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// The smallest stylesheet Excel opens without a repair prompt: one font, the
/// two fills the format reserves, one border, one cell format.
fn styles() -> Result<Vec<u8>, ExportError> {
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<styleSheet xmlns=\"{MAIN_NS}\">\
<fonts count=\"1\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts>\
<fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills>\
<borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>\
<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>\
<cellXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/></cellXfs>\
<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>\
</styleSheet>"
    );
    Ok(xml.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::column_letter;

    #[test]
    fn column_letters_carry_past_z() {
        assert_eq!(column_letter(0), "A");
        assert_eq!(column_letter(1), "B");
        assert_eq!(column_letter(25), "Z");
        assert_eq!(column_letter(26), "AA");
        assert_eq!(column_letter(51), "AZ");
        assert_eq!(column_letter(52), "BA");
        assert_eq!(column_letter(255), "IV");
    }
}
