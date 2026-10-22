//! The block structure of a copied range, for the clipboard's `text/html`
//! flavour (`docs/174` TA-12).
//!
//! [`copy_rich_runs`](crate::WasmDocument::copy_rich_runs) is the run stream a
//! copy carries: formatted text with one `paragraphBreak` marker between
//! consecutive paragraphs. It says nothing about what each paragraph IS, so the
//! visible HTML built from it was a `<p>` per paragraph, and a list, a heading or
//! a table pasted into Word or Google Docs arrived as flat paragraphs.
//!
//! [`copy_block_outline`](crate::WasmDocument::copy_block_outline) answers that
//! question with one entry per paragraph of the SAME range, in the SAME order —
//! the paragraph sequence `copy_rich_runs` walks — so the host zips the two:
//! entry `i` describes the runs between paragraph break `i - 1` and `i`. Each
//! entry carries the paragraph's heading level (the engine's one heading rule,
//! `casual_doc_layout::fold::heading_level`, which the outline panel and the
//! accessibility projection also use), its list kind and level, and the cell of
//! the outermost table it sits in. A nested table flattens into its outer cell:
//! HTML clipboards are read by apps that handle one level reliably, and the text
//! is never lost.
//!
//! # Complexity
//!
//! O(document): one walk for the paragraph sequence (shared with
//! `copy_rich_runs`), one walk recording each paragraph's descriptor in a map,
//! and one map lookup per paragraph in the range. Never on a keystroke path —
//! it runs once per copy, like `copy_rich_runs` itself.

use std::collections::HashMap;

use casual_doc_edit::surface_block_lists;
use casual_doc_layout::cascade::StyleCascade;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, Paragraph};
use wasm_bindgen::prelude::*;

use crate::{WasmDocument, collect_block_text_all_surfaces, parse_range, visit_paragraphs};

/// One paragraph of a copied range. Every field is optional and omitted when
/// absent, so a plain paragraph is `{}`.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OutlineEntry {
    /// 1-based heading level (`Heading 1` = 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) heading: Option<u8>,
    /// The list the paragraph is an item of.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) list: Option<OutlineList>,
    /// The outermost table cell holding the paragraph.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cell: Option<OutlineCell>,
}

/// A list item's kind and 0-based nesting level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OutlineList {
    pub(crate) ordered: bool,
    pub(crate) level: u8,
}

/// A table cell's coordinates. `table` is the table's node id, so two adjacent
/// tables never merge into one.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OutlineCell {
    pub(crate) table: String,
    pub(crate) row: u32,
    pub(crate) col: u32,
}

#[wasm_bindgen]
impl WasmDocument {
    /// The block structure of a copied range as JSON: one `OutlineEntry` per
    /// paragraph [`copy_rich_runs`](Self::copy_rich_runs) walks for the same
    /// range, in the same order. `"[]"` when either endpoint is unknown. See the
    /// module documentation for how the host zips the two. O(document).
    #[wasm_bindgen(js_name = copyBlockOutline)]
    #[must_use]
    pub fn copy_block_outline(
        &self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> String {
        let Some(range) = parse_range(start_node, start_offset, end_node, end_offset) else {
            return "[]".to_owned();
        };
        serde_json::to_string(&self.block_outline(range.start.node, range.end.node))
            .unwrap_or_else(|_| "[]".to_owned())
    }

    /// The entries for the paragraphs from `a` to `b` (either order), or none
    /// when either is not a paragraph.
    pub(crate) fn block_outline(&self, a: NodeId, b: NodeId) -> Vec<OutlineEntry> {
        // The sequence `copy_rich_runs_inner` walks; sharing it is what keeps
        // entry `i` and paragraph `i` of the run stream the same paragraph.
        let mut nodes: Vec<(NodeId, String)> = Vec::new();
        collect_block_text_all_surfaces(&self.document, &mut nodes);
        let (Some(si), Some(ei)) = (
            nodes.iter().position(|(id, _)| *id == a),
            nodes.iter().position(|(id, _)| *id == b),
        ) else {
            return Vec::new();
        };
        let (lo, hi) = (si.min(ei), si.max(ei));
        let described = self.describe_paragraphs();
        nodes[lo..=hi]
            .iter()
            .map(|(id, _)| described.get(id).cloned().unwrap_or_default())
            .collect()
    }

    /// Every paragraph's descriptor, by id, from one walk of every surface.
    fn describe_paragraphs(&self) -> HashMap<NodeId, OutlineEntry> {
        let cascade = StyleCascade::new(self.document.definitions());
        let mut out = HashMap::new();
        let mut describe = |paragraph: &Paragraph, cell: Option<OutlineCell>| {
            let heading = self.heading_level_of(paragraph.properties.get(), &cascade);
            let list = paragraph
                .properties
                .numbering
                .as_ref()
                .map(|reference| OutlineList {
                    ordered: self.numbering_ordered(reference),
                    level: reference.level,
                });
            out.insert(
                paragraph.id,
                OutlineEntry {
                    heading,
                    list,
                    cell,
                },
            );
        };
        for blocks in surface_block_lists(&self.document) {
            walk(blocks, &mut describe);
        }
        out
    }
}

/// Visits every paragraph under `blocks` with the outermost table cell holding
/// it. A table's paragraphs — its nested tables' and text boxes' included — all
/// report the outer cell.
fn walk(blocks: &[BlockNode], describe: &mut impl FnMut(&Paragraph, Option<OutlineCell>)) {
    for block in blocks {
        match block {
            BlockNode::Table(table) => {
                let id = table.id.to_string();
                for (row, cells) in table.rows.iter().enumerate() {
                    for (col, cell) in cells.cells.iter().enumerate() {
                        let here = OutlineCell {
                            table: id.clone(),
                            row: u32::try_from(row).unwrap_or(u32::MAX),
                            col: u32::try_from(col).unwrap_or(u32::MAX),
                        };
                        visit_paragraphs(&cell.blocks, &mut |paragraph| {
                            describe(paragraph, Some(here.clone()));
                        });
                    }
                }
            }
            BlockNode::Sdt(sdt) => walk(&sdt.blocks, describe),
            other => visit_paragraphs(std::slice::from_ref(other), &mut |paragraph| {
                describe(paragraph, None);
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::wasm_document;
    use casual_doc_model::v1::Document;

    fn import(body: &str) -> Document {
        let xml = format!(r#"<w:document xmlns:w="urn:w"><w:body>{body}</w:body></w:document>"#);
        casual_doc_import::import_main_document_xml(
            xml.as_bytes(),
            casual_doc_import::ImportConfig::default(),
        )
        .expect("import")
        .document
    }

    fn p(text: &str) -> String {
        format!(r#"<w:p><w:r><w:t>{text}</w:t></w:r></w:p>"#)
    }

    /// A heading, a numbered item, a bullet item, a 2x2 table and a plain
    /// paragraph. The lists are made with the editor's own command, so they
    /// carry real numbering definitions.
    fn mixed() -> WasmDocument {
        let heading =
            r#"<w:p><w:pPr><w:outlineLvl w:val="1"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p>"#;
        let cell = |text: &str| format!("<w:tc>{}</w:tc>", p(text));
        let table = format!(
            "<w:tbl><w:tr>{}{}</w:tr><w:tr>{}{}</w:tr></w:tbl>",
            cell("c1"),
            cell("c2"),
            cell("c3"),
            cell("c4")
        );
        let mut document = wasm_document(import(&format!(
            "{heading}{}{}{table}{}",
            p("one"),
            p("two"),
            p("after")
        )));
        let ids = all_paragraph_ids(&document);
        for (index, kind) in [(1, "numbered"), (2, "bullet")] {
            let id = ids[index].to_string();
            document.toggle_list(&id, 0, &id, 0, kind).expect("list");
        }
        document
    }

    fn all_paragraph_ids(document: &WasmDocument) -> Vec<NodeId> {
        let mut nodes = Vec::new();
        collect_block_text_all_surfaces(&document.document, &mut nodes);
        nodes.into_iter().map(|(id, _)| id).collect()
    }

    #[test]
    fn each_paragraph_is_described_as_what_it_is() {
        let document = mixed();
        let ids = all_paragraph_ids(&document);
        let outline = document.block_outline(ids[0], *ids.last().expect("paragraphs"));
        assert_eq!(outline.len(), 8, "{outline:?}");
        assert_eq!(outline[0].heading, Some(2));
        assert_eq!(
            outline[1].list,
            Some(OutlineList {
                ordered: true,
                level: 0
            })
        );
        assert_eq!(
            outline[2].list,
            Some(OutlineList {
                ordered: false,
                level: 0
            })
        );
        assert_eq!(outline[1].cell, None);
        let cells: Vec<(u32, u32)> = outline[3..7]
            .iter()
            .map(|entry| {
                let cell = entry.cell.as_ref().expect("in the table");
                (cell.row, cell.col)
            })
            .collect();
        assert_eq!(cells, [(0, 0), (0, 1), (1, 0), (1, 1)]);
        assert_eq!(outline[7], OutlineEntry::default());
    }

    /// The contract the host relies on: one entry per paragraph of the run
    /// stream `copyRichRuns` returns for the same range.
    #[test]
    fn the_outline_has_one_entry_per_copied_paragraph() {
        let document = mixed();
        let ids = all_paragraph_ids(&document);
        for (a, b) in [(0, 7), (2, 5), (5, 2), (4, 4)] {
            let (start, end) = (ids[a].to_string(), ids[b].to_string());
            let runs: Vec<serde_json::Value> =
                serde_json::from_str(&document.copy_rich_runs(&start, 0, &end, 1)).expect("runs");
            let breaks = runs
                .iter()
                .filter(|run| run["paragraphBreak"] == serde_json::Value::Bool(true))
                .count();
            let outline: Vec<serde_json::Value> =
                serde_json::from_str(&document.copy_block_outline(&start, 0, &end, 1))
                    .expect("outline");
            assert_eq!(outline.len(), breaks + 1, "range {a}..{b}");
        }
    }

    #[test]
    fn an_unknown_endpoint_is_an_empty_outline() {
        let document = mixed();
        assert_eq!(document.copy_block_outline("nope", 0, "nope", 0), "[]");
    }
}
