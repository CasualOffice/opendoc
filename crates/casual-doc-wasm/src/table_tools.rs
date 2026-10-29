// SPDX-License-Identifier: Apache-2.0

//! The editor's table tools: style options, converting between text and tables,
//! splitting a table, inserting and deleting cells with a shift, nesting a table
//! in a cell, and cell text direction (`docs/153` table rows; `109` RM-26,
//! RM-27, RM-37).
//!
//! The lowerings are `casual_doc_command::table_tools`, shared with agents
//! (`docs/181` §4.2 step 1); this module resolves what the editor names — the
//! caret's paragraph, the two corners of a cell range, the paragraphs of a
//! selection — and applies the result as one undoable action. Every binding has
//! an `_inner` twin returning `Result<_, String>`, because a native test cannot
//! build the `JsValue` the boundary returns.
//!
//! # Complexity
//!
//! Resolving a node is the existing O(document) id lookup every table command
//! already makes (`locate_table_row`, `locate_block`); the work after it is
//! O(cells of the table) or O(inlines of the converted paragraphs). None of it
//! is on a keystroke path.

use std::str::FromStr;

use casual_doc_command::table_tools::{
    self, Autofit, CellRange, CellShift, LookOption, Separator, TextToTable,
};
use casual_doc_edit::{Pos, find_table, locate_cell, locate_table_cell, locate_table_row, refused};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, Document, Paragraph, Table};
use wasm_bindgen::prelude::*;

use crate::{EditResult, HistoryKind, WasmDocument, to_js};

/// The blocks of `container` (`None` = the body, else a table cell or content
/// control), searching the body. O(document).
fn container_blocks(document: &Document, container: Option<NodeId>) -> Option<&[BlockNode]> {
    fn walk(blocks: &[BlockNode], id: NodeId) -> Option<&[BlockNode]> {
        for block in blocks {
            match block {
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            if cell.id == id {
                                return Some(cell.blocks.as_slice());
                            }
                            if let Some(found) = walk(&cell.blocks, id) {
                                return Some(found);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if sdt.id == id {
                        return Some(sdt.blocks.as_slice());
                    }
                    if let Some(found) = walk(&sdt.blocks, id) {
                        return Some(found);
                    }
                }
                BlockNode::Paragraph(_) | BlockNode::AltChunk(_) => {}
            }
        }
        None
    }
    match container {
        None => Some(document.body()),
        Some(id) => walk(document.body(), id),
    }
}

/// The table holding paragraph `node`, with the row and cell index of the
/// paragraph's cell. O(document).
fn table_at(document: &Document, node: NodeId) -> Result<(Table, usize, usize), String> {
    let refusal = || refused!("table.caret-outside", "Put the caret in a table first.").to_owned();
    let (table, row, _) = locate_table_row(document, node).ok_or_else(refusal)?;
    let (_, cell) = locate_table_cell(document, node).ok_or_else(refusal)?;
    let table = find_table(document, table).ok_or_else(refusal)?.clone();
    Ok((table, row as usize, cell as usize))
}

/// The cell range between `anchor` and `focus` (`""` = the anchor's cell
/// alone), both in the same table. O(document).
fn range_at(document: &Document, anchor: &str, focus: &str) -> Result<(Table, CellRange), String> {
    let anchor = NodeId::from_str(anchor).map_err(|_| "invalid node id".to_owned())?;
    let (table, row, cell) = table_at(document, anchor)?;
    let other = if focus.is_empty() {
        (row, cell)
    } else {
        let focus = NodeId::from_str(focus).map_err(|_| "invalid node id".to_owned())?;
        let (focus_table, r, c) = table_at(document, focus)?;
        if focus_table.id != table.id {
            return Err("the two cells are in different tables".to_owned());
        }
        (r, c)
    };
    Ok((table, CellRange::between((row, cell), other)))
}

/// Where table `table` sits: its container and index. O(document).
fn table_place(document: &Document, table: NodeId) -> Result<(Option<NodeId>, usize), String> {
    casual_doc_command::references::locate_block(document, table).ok_or_else(|| {
        refused!(
            "table.body-only",
            "This works on tables in the document body, not in a header, footer or note."
        )
        .to_owned()
    })
}

impl WasmDocument {
    /// The content width of the document's default page, in twips. O(1).
    fn page_content_width(&self) -> i32 {
        let c = &self.default_config;
        c.page_size.width.raw() - c.margin_start.raw() - c.margin_end.raw()
    }

    /// The width a table made in `container` should span: the page's content
    /// width in the body, the cell's own content width in a cell. O(document).
    fn content_width_in(&self, container: Option<NodeId>, paragraph: NodeId) -> i32 {
        if container.is_some()
            && let Ok((table, row, cell)) = table_at(&self.document, paragraph)
            && let Some(width) = table_tools::cell_content_width(&table, row, cell)
        {
            return width;
        }
        self.page_content_width()
    }

    /// The paragraphs `from..=to`, which must be consecutive paragraphs of one
    /// container, with that container and the first one's index.
    /// O(document) to resolve, then O(paragraphs).
    fn paragraph_run(
        &self,
        from: &str,
        to: &str,
    ) -> Result<(Option<NodeId>, usize, Vec<Paragraph>), String> {
        let outside = || {
            refused!(
                "table.convert-outside",
                "Select paragraphs in the document body or in one table cell."
            )
            .to_owned()
        };
        let from = NodeId::from_str(from).map_err(|_| outside())?;
        let to = if to.is_empty() {
            from
        } else {
            NodeId::from_str(to).map_err(|_| outside())?
        };
        let locate = casual_doc_command::references::locate_block;
        let (container, a) = locate(&self.document, from).ok_or_else(outside)?;
        let (other, b) = locate(&self.document, to).ok_or_else(outside)?;
        if container != other {
            return Err(refused!(
                "table.convert-span",
                "Select paragraphs that sit together, outside any table, to convert them."
            )
            .to_owned());
        }
        let (first, last) = (a.min(b), a.max(b));
        let blocks = container_blocks(&self.document, container).ok_or_else(outside)?;
        let mut paragraphs = Vec::with_capacity(last - first + 1);
        for block in blocks.get(first..=last).ok_or_else(outside)? {
            match block {
                BlockNode::Paragraph(paragraph) => paragraphs.push(paragraph.clone()),
                BlockNode::Table(_) | BlockNode::Sdt(_) | BlockNode::AltChunk(_) => {
                    return Err(refused!(
                        "table.convert-span",
                        "Select paragraphs that sit together, outside any table, to convert them."
                    )
                    .to_owned());
                }
            }
        }
        Ok((container, first, paragraphs))
    }
}

#[wasm_bindgen]
impl WasmDocument {
    /// The table style options on for the table holding `node`, by name
    /// (`headerRow`, `firstColumn`, `totalRow`, `lastColumn`, `bandedRows`,
    /// `bandedColumns`); empty outside a table. **O(document)** for the lookup.
    #[wasm_bindgen(js_name = tableLookOptions)]
    #[must_use]
    pub fn table_look_options(&self, node: &str) -> Vec<String> {
        NodeId::from_str(node)
            .ok()
            .and_then(|node| table_at(&self.document, node).ok())
            .map(|(table, _, _)| {
                table_tools::look_options_on(&table.properties.look)
                    .into_iter()
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Turns one table style option (`w:tblLook`) on or off for the table
    /// holding `node`. One undoable step.
    ///
    /// # Errors
    /// Not in a table, or an unknown option name.
    #[wasm_bindgen(js_name = setTableLookOption)]
    pub fn set_table_look_option(
        &mut self,
        node: &str,
        option: &str,
        on: bool,
    ) -> Result<EditResult, JsValue> {
        self.set_table_look_option_inner(node, option, on)
            .map_err(to_js)
    }

    /// The Convert Text to Table dialog's suggested column count for the
    /// paragraphs `from..=to` at `separator` (`paragraph`, `tab`, `comma`,
    /// `semicolon`, `other` with `character`), or `-1` when they cannot be
    /// converted. **O(document)** to resolve.
    #[wasm_bindgen(js_name = textToTableColumns)]
    #[must_use]
    pub fn text_to_table_columns(
        &self,
        from: &str,
        to: &str,
        separator: &str,
        character: &str,
    ) -> i32 {
        let Ok(separator) = Separator::parse(separator, character) else {
            return -1;
        };
        let Ok((_, _, paragraphs)) = self.paragraph_run(from, to) else {
            return -1;
        };
        let refs: Vec<&Paragraph> = paragraphs.iter().collect();
        i32::try_from(table_tools::text_to_table_columns(&refs, separator)).unwrap_or(-1)
    }

    /// Converts the paragraphs `from..=to` into a table (Insert ▸ Table ▸
    /// Convert Text to Table): `separator` and `character` as for
    /// [`textToTableColumns`](Self::text_to_table_columns), `columns` (`0` =
    /// what the text needs), `autofit` `fixed` / `contents` / `window`. The
    /// caret goes to the first cell. One undoable step.
    ///
    /// # Errors
    /// The paragraphs are not consecutive in one container, the separator is
    /// unknown or empty, or the table would be too wide.
    #[wasm_bindgen(js_name = convertTextToTable)]
    pub fn convert_text_to_table(
        &mut self,
        from: &str,
        to: &str,
        separator: &str,
        character: &str,
        columns: u32,
        autofit: &str,
    ) -> Result<EditResult, JsValue> {
        self.convert_text_to_table_inner(from, to, separator, character, columns, autofit)
            .map_err(to_js)
    }

    /// Converts the table holding `node` to text (Table ▸ Convert to Text):
    /// `separator` and `character` as above, `nested` converting nested tables
    /// too. One undoable step.
    ///
    /// # Errors
    /// Not in a body table, or an unknown or empty separator.
    #[wasm_bindgen(js_name = convertTableToText)]
    pub fn convert_table_to_text(
        &mut self,
        node: &str,
        separator: &str,
        character: &str,
        nested: bool,
    ) -> Result<EditResult, JsValue> {
        self.convert_table_to_text_inner(node, separator, character, nested)
            .map_err(to_js)
    }

    /// Splits the table holding `node` so the caret's row starts a second
    /// table (Word's Split Table, Ctrl/Cmd+Shift+Enter); in the first row, an
    /// empty paragraph goes above the table instead. The caret goes to the
    /// paragraph between, as Word's does. One undoable step.
    ///
    /// # Errors
    /// Not in a body table.
    #[wasm_bindgen(js_name = splitTable)]
    pub fn split_table(&mut self, node: &str) -> Result<EditResult, JsValue> {
        self.split_table_inner(node).map_err(to_js)
    }

    /// Inserts cells at the range between `anchor` and `focus` (`""` = the
    /// anchor's cell) with `shift` `right` or `down` (Word's Insert Cells). One
    /// undoable step.
    ///
    /// # Errors
    /// Not in a table, a merged table, or another shift.
    #[wasm_bindgen(js_name = insertTableCells)]
    pub fn insert_table_cells(
        &mut self,
        anchor: &str,
        focus: &str,
        shift: &str,
    ) -> Result<EditResult, JsValue> {
        self.shift_table_cells_inner(anchor, focus, shift, true)
            .map_err(to_js)
    }

    /// Deletes the cells of the range between `anchor` and `focus` with
    /// `shift` `left` or `up` (Word's Delete Cells). One undoable step.
    ///
    /// # Errors
    /// Not in a table, a merged table, another shift, or a delete that would
    /// leave no cells.
    #[wasm_bindgen(js_name = deleteTableCells)]
    pub fn delete_table_cells(
        &mut self,
        anchor: &str,
        focus: &str,
        shift: &str,
    ) -> Result<EditResult, JsValue> {
        self.shift_table_cells_inner(anchor, focus, shift, false)
            .map_err(to_js)
    }

    /// The text direction of the cell holding `node`: `horizontal`,
    /// `rotateDown` or `rotateUp`; empty outside a table. **O(document).**
    #[wasm_bindgen(js_name = cellTextDirectionAt)]
    #[must_use]
    pub fn cell_text_direction_at(&self, node: &str) -> String {
        NodeId::from_str(node)
            .ok()
            .and_then(|node| table_at(&self.document, node).ok())
            .and_then(|(table, row, cell)| {
                table
                    .rows
                    .get(row)
                    .and_then(|r| r.cells.get(cell))
                    .map(|c| table_tools::text_direction_name(c.properties.text_direction))
            })
            .unwrap_or_default()
            .to_owned()
    }

    /// Sets the text direction (`horizontal`, `rotateDown`, `rotateUp`) of
    /// every cell in the range between `anchor` and `focus` (`""` = the
    /// anchor's cell), each merged cell once. One undoable step.
    ///
    /// # Errors
    /// Not in a table, or an unknown direction.
    #[wasm_bindgen(js_name = setCellTextDirection)]
    pub fn set_cell_text_direction(
        &mut self,
        anchor: &str,
        focus: &str,
        direction: &str,
    ) -> Result<EditResult, JsValue> {
        self.set_cell_text_direction_inner(anchor, focus, direction)
            .map_err(to_js)
    }
}

impl WasmDocument {
    /// See [`WasmDocument::set_table_look_option`].
    pub(crate) fn set_table_look_option_inner(
        &mut self,
        node: &str,
        option: &str,
        on: bool,
    ) -> Result<EditResult, String> {
        let option =
            LookOption::parse(option).ok_or_else(|| "unknown table style option".to_owned())?;
        let nid = NodeId::from_str(node).map_err(|_| "invalid node id".to_owned())?;
        let (table, _, _) = table_at(&self.document, nid)?;
        let ops = table_tools::set_look_option(&table, option, on);
        self.apply_action_as(ops, HistoryKind::TableFormatting)
    }

    /// See [`WasmDocument::convert_text_to_table`].
    pub(crate) fn convert_text_to_table_inner(
        &mut self,
        from: &str,
        to: &str,
        separator: &str,
        character: &str,
        columns: u32,
        autofit: &str,
    ) -> Result<EditResult, String> {
        let separator = Separator::parse(separator, character)?;
        let (container, index, paragraphs) = self.paragraph_run(from, to)?;
        let first = paragraphs.first().map(|p| p.id).ok_or("no paragraphs")?;
        let options = TextToTable {
            separator,
            columns,
            autofit: Autofit::parse(autofit),
            content_width: self.content_width_in(container, first),
        };
        let refs: Vec<&Paragraph> = paragraphs.iter().collect();
        let ids = &mut self.edit_ids;
        let edit = table_tools::text_to_table(container, index, &refs, options, &mut || {
            ids.next_id().ok()
        })?;
        let caret = Pos::new(edit.caret.unwrap_or(first), 0);
        self.apply_action_caret_as(edit.operations, caret, HistoryKind::TableStructure)
    }

    /// See [`WasmDocument::convert_table_to_text`].
    pub(crate) fn convert_table_to_text_inner(
        &mut self,
        node: &str,
        separator: &str,
        character: &str,
        nested: bool,
    ) -> Result<EditResult, String> {
        let separator = Separator::parse(separator, character)?;
        let nid = NodeId::from_str(node).map_err(|_| "invalid node id".to_owned())?;
        let (table, _, _) = table_at(&self.document, nid)?;
        let (container, index) = table_place(&self.document, table.id)?;
        let ids = &mut self.edit_ids;
        let edit =
            table_tools::table_to_text(container, index, &table, separator, nested, &mut || {
                ids.next_id().ok()
            })?;
        let caret = Pos::new(edit.caret.unwrap_or(nid), 0);
        self.apply_action_caret_as(edit.operations, caret, HistoryKind::TableStructure)
    }

    /// See [`WasmDocument::split_table`].
    pub(crate) fn split_table_inner(&mut self, node: &str) -> Result<EditResult, String> {
        let nid = NodeId::from_str(node).map_err(|_| "invalid node id".to_owned())?;
        let (table, row, _) = table_at(&self.document, nid)?;
        let (container, index) = table_place(&self.document, table.id)?;
        let ids = &mut self.edit_ids;
        let (ops, between) =
            table_tools::split_table(container, index, &table, row, &mut || ids.next_id().ok())?;
        self.apply_action_caret_as(ops, Pos::new(between, 0), HistoryKind::TableStructure)
    }

    /// See [`WasmDocument::insert_table_cells`] and
    /// [`WasmDocument::delete_table_cells`].
    pub(crate) fn shift_table_cells_inner(
        &mut self,
        anchor: &str,
        focus: &str,
        shift: &str,
        insert: bool,
    ) -> Result<EditResult, String> {
        let shift = CellShift::parse(shift)?;
        let (table, range) = range_at(&self.document, anchor, focus)?;
        let fallback = NodeId::from_str(anchor).map_err(|_| "invalid node id".to_owned())?;
        let ids = &mut self.edit_ids;
        let mut mint = || ids.next_id().ok();
        let edit = if insert {
            table_tools::insert_cells(&table, range, shift, &mut mint)?
        } else {
            table_tools::delete_cells(&table, range, shift, &mut mint)?
        };
        let caret = Pos::new(edit.caret.unwrap_or(fallback), 0);
        self.apply_action_caret_as(edit.operations, caret, HistoryKind::TableStructure)
    }

    /// See [`WasmDocument::set_cell_text_direction`].
    pub(crate) fn set_cell_text_direction_inner(
        &mut self,
        anchor: &str,
        focus: &str,
        direction: &str,
    ) -> Result<EditResult, String> {
        let direction = table_tools::text_direction_named(direction)?;
        let (table, range) = range_at(&self.document, anchor, focus)?;
        let cells = casual_doc_command::table::range_cells(
            &table,
            (range.rows.0, range.cells.0),
            (range.rows.1, range.cells.1),
        )?;
        let ops = casual_doc_command::table::set_cell_properties(cells, |properties| {
            properties.text_direction = direction;
        });
        self.apply_action_as(ops, HistoryKind::TableFormatting)
    }

    /// Where Insert ▸ Table puts a new table for the caret's paragraph `node`:
    /// inside the caret's table cell, after the paragraph (`docs/153`
    /// `table.nested`), when the paragraph sits directly in a cell; otherwise
    /// after the caret's top-level body block, as it always has. Returns the
    /// container, the index and the width to span. O(document).
    pub(crate) fn table_insertion_point(&self, node: NodeId) -> (Option<NodeId>, usize, i32) {
        if let Some((_, cell)) = locate_cell(&self.document, node)
            && let Some((Some(container), index)) =
                casual_doc_command::references::locate_block(&self.document, node)
            && container == cell
        {
            return (
                Some(cell),
                index + 1,
                self.content_width_in(Some(cell), node),
            );
        }
        (
            None,
            casual_doc_command::table::body_index_after(&self.document, node),
            self.page_content_width(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_plain_text;
    use crate::tests::wasm_document;
    use casual_doc_model::v1::TextDirection;

    /// A document from body markup, imported so the tables are real ones.
    fn document(body: &str) -> WasmDocument {
        let xml = format!(r#"<w:document xmlns:w="urn:w"><w:body>{body}</w:body></w:document>"#);
        let document = casual_doc_import::import_main_document_xml(
            xml.as_bytes(),
            casual_doc_import::ImportConfig::default(),
        )
        .expect("import")
        .document;
        wasm_document(document)
    }

    fn p(text: &str) -> String {
        format!(r#"<w:p><w:r><w:t>{text}</w:t></w:r></w:p>"#)
    }

    /// A `rows` x `columns` table whose cells read "rc".
    fn table_xml(rows: usize, columns: usize) -> String {
        let mut xml = String::from("<w:tbl><w:tblGrid>");
        for _ in 0..columns {
            xml.push_str(r#"<w:gridCol w:w="2000"/>"#);
        }
        xml.push_str("</w:tblGrid>");
        for r in 0..rows {
            xml.push_str("<w:tr>");
            for c in 0..columns {
                xml.push_str(&format!("<w:tc>{}</w:tc>", p(&format!("{r}{c}"))));
            }
            xml.push_str("</w:tr>");
        }
        xml.push_str("</w:tbl>");
        xml
    }

    /// The id of the first paragraph (at any depth) reading exactly `text`.
    fn paragraph_reading(d: &WasmDocument, text: &str) -> String {
        fn walk(blocks: &[BlockNode], text: &str) -> Option<NodeId> {
            blocks.iter().find_map(|block| match block {
                BlockNode::Paragraph(p) if node_plain_text(&p.inlines) == text => Some(p.id),
                BlockNode::Table(t) => t
                    .rows
                    .iter()
                    .flat_map(|r| r.cells.iter())
                    .find_map(|c| walk(&c.blocks, text)),
                BlockNode::Sdt(s) => walk(&s.blocks, text),
                _ => None,
            })
        }
        walk(d.document.body(), text)
            .unwrap_or_else(|| panic!("no paragraph reading {text:?}"))
            .to_string()
    }

    fn body_shape(d: &WasmDocument) -> Vec<String> {
        d.document
            .body()
            .iter()
            .map(|block| match block {
                BlockNode::Paragraph(p) => format!("p:{}", node_plain_text(&p.inlines)),
                BlockNode::Table(t) => format!("t:{}x{}", t.rows.len(), t.rows[0].cells.len()),
                _ => "other".to_owned(),
            })
            .collect()
    }

    #[test]
    fn a_look_option_is_set_read_back_and_undone() {
        let mut d = document(&table_xml(2, 2));
        let caret = paragraph_reading(&d, "00");
        let before = d.table_look_options(&caret);
        d.set_table_look_option_inner(&caret, "headerRow", true)
            .expect("sets");
        assert!(
            d.table_look_options(&caret)
                .contains(&"headerRow".to_owned())
        );
        d.set_table_look_option_inner(&caret, "bandedRows", false)
            .expect("sets");
        assert!(
            !d.table_look_options(&caret)
                .contains(&"bandedRows".to_owned())
        );
        d.undo_inner().expect("undo");
        d.undo_inner().expect("undo");
        assert_eq!(d.table_look_options(&caret), before);
        assert!(
            d.set_table_look_option_inner(&caret, "zebra", true)
                .is_err()
        );
    }

    #[test]
    fn text_converts_to_a_table_and_back_through_the_facade() {
        let mut d = document(&format!("{}{}{}", p("Title"), p("a,b,c"), p("d,e")));
        let from = paragraph_reading(&d, "a,b,c");
        let to = paragraph_reading(&d, "d,e");
        assert_eq!(d.text_to_table_columns(&from, &to, "comma", ""), 3);
        d.convert_text_to_table_inner(&from, &to, "comma", "", 0, "fixed")
            .expect("converts");
        assert_eq!(body_shape(&d), vec!["p:Title", "t:2x3"]);

        let caret = paragraph_reading(&d, "a");
        d.convert_table_to_text_inner(&caret, "semicolon", "", false)
            .expect("converts back");
        assert_eq!(body_shape(&d), vec!["p:Title", "p:a;b;c", "p:d;e;"]);

        d.undo_inner().expect("undo");
        d.undo_inner().expect("undo");
        assert_eq!(body_shape(&d), vec!["p:Title", "p:a,b,c", "p:d,e"]);
    }

    #[test]
    fn a_selection_that_crosses_a_table_is_refused_by_name() {
        let mut d = document(&format!("{}{}{}", p("one"), table_xml(1, 1), p("two")));
        let from = paragraph_reading(&d, "one");
        let to = paragraph_reading(&d, "two");
        let refusal = d
            .convert_text_to_table_inner(&from, &to, "tab", "", 0, "fixed")
            .unwrap_err();
        assert!(refusal.contains("Select paragraphs"), "{refusal}");
        assert_eq!(d.text_to_table_columns(&from, &to, "tab", ""), -1);
        let refusal = d
            .convert_text_to_table_inner(&from, &from, "other", "", 0, "fixed")
            .unwrap_err();
        assert!(refusal.contains("Type a character"), "{refusal}");
    }

    #[test]
    fn splitting_at_the_caret_row_leaves_two_tables_and_a_paragraph_between() {
        let mut d = document(&format!("{}{}", table_xml(3, 2), p("after")));
        let caret = paragraph_reading(&d, "10");
        d.split_table_inner(&caret).expect("splits");
        assert_eq!(body_shape(&d), vec!["t:1x2", "p:", "t:2x2", "p:after"]);
        d.undo_inner().expect("undo");
        assert_eq!(body_shape(&d), vec!["t:3x2", "p:after"]);
        let top = paragraph_reading(&d, "00");
        d.split_table_inner(&top).expect("first row");
        assert_eq!(body_shape(&d), vec!["p:", "t:3x2", "p:after"]);
    }

    #[test]
    fn cells_shift_right_and_up_through_the_facade() {
        let mut d = document(&table_xml(2, 2));
        let caret = paragraph_reading(&d, "00");
        d.shift_table_cells_inner(&caret, "", "right", true)
            .expect("inserts");
        let BlockNode::Table(table) = &d.document.body()[0] else {
            panic!("table");
        };
        assert_eq!(table.rows[0].cells.len(), 3);
        assert_eq!(table.grid.len(), 3);
        assert_eq!(table.rows[1].properties.grid_after, Some(1));
        d.undo_inner().expect("undo");

        let caret = paragraph_reading(&d, "00");
        d.shift_table_cells_inner(&caret, "", "up", false)
            .expect("deletes");
        assert_eq!(
            node_plain_text(match &d.document.body()[0] {
                BlockNode::Table(t) => match &t.rows[0].cells[0].blocks[0] {
                    BlockNode::Paragraph(p) => &p.inlines,
                    _ => panic!(),
                },
                _ => panic!(),
            }),
            "10"
        );
        assert!(d.shift_table_cells_inner(&caret, "", "up", true).is_err());
    }

    #[test]
    fn insert_table_from_inside_a_cell_nests_it_in_that_cell() {
        let mut d = document(&format!("{}{}", table_xml(1, 2), p("after")));
        let caret = paragraph_reading(&d, "01");
        let nid = NodeId::from_str(&caret).expect("id");
        let (container, index, width) = d.table_insertion_point(nid);
        assert!(
            container.is_some(),
            "a caret in a cell inserts into the cell"
        );
        assert_eq!(index, 1);
        assert!(width < 2000, "the nested table fits its cell: {width}");
        d.insert_table_inner(&caret, 2, 2).expect("inserts");
        assert_eq!(
            body_shape(&d),
            vec!["t:1x2", "p:after"],
            "nothing lands after the table"
        );
        let BlockNode::Table(outer) = &d.document.body()[0] else {
            panic!();
        };
        assert!(matches!(
            outer.rows[0].cells[1].blocks[1],
            BlockNode::Table(_)
        ));

        // From a body paragraph, the table still goes into the body.
        let after = paragraph_reading(&d, "after");
        d.insert_table_inner(&after, 1, 1).expect("inserts");
        assert_eq!(body_shape(&d), vec!["t:1x2", "p:after", "t:1x1"]);
    }

    #[test]
    fn a_cell_text_direction_is_set_on_the_range_and_read_back() {
        let mut d = document(&table_xml(2, 2));
        let a = paragraph_reading(&d, "00");
        let b = paragraph_reading(&d, "01");
        assert_eq!(d.cell_text_direction_at(&a), "horizontal");
        d.set_cell_text_direction_inner(&a, &b, "rotateDown")
            .expect("sets");
        assert_eq!(d.cell_text_direction_at(&a), "rotateDown");
        assert_eq!(d.cell_text_direction_at(&b), "rotateDown");
        let BlockNode::Table(table) = &d.document.body()[0] else {
            panic!();
        };
        assert_eq!(
            table.rows[0].cells[0].properties.text_direction,
            Some(TextDirection::TbRl)
        );
        assert_eq!(table.rows[1].cells[0].properties.text_direction, None);
        d.set_cell_text_direction_inner(&a, "", "rotateUp")
            .expect("sets");
        assert_eq!(d.cell_text_direction_at(&a), "rotateUp");
        d.set_cell_text_direction_inner(&a, "", "horizontal")
            .expect("clears");
        assert_eq!(d.cell_text_direction_at(&a), "horizontal");
        assert_eq!(d.cell_text_direction_at("not-a-node"), "");
    }
}
