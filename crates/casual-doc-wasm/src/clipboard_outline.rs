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

/// Word's built-in heading sizes in half-points, `Heading 1`..`Heading 6`, used
/// only when the document has no heading style of that level to reuse.
const PASTED_HEADING_HALF_POINTS: [u32; 6] = [32, 26, 24, 22, 22, 22];

impl WasmDocument {
    /// The paragraph style a pasted `<hN>` takes (`docs/174` TA-11): the
    /// document's own heading style of that level when it has one (by name,
    /// `Heading N`, case- and space-insensitive — the engine's heading naming
    /// rule), else a new `Heading N` whose creation is pushed onto `install`, so
    /// the caller puts it in the SAME transaction as the paste and one undo
    /// removes both (`147`, ADR-005). A style created earlier in the same paste
    /// is found in `install` and reused, never created twice.
    ///
    /// The created style is Word's shape: outline level `N - 1` (which is what
    /// makes it a heading to the outline, folding and the accessibility tree),
    /// bold, Word's size for the level, kept with the next paragraph.
    ///
    /// **Complexity: O(styles + install)**, bounded by the style registry.
    pub(crate) fn pasted_heading_style(
        &mut self,
        level: u8,
        install: &mut Vec<casual_doc_edit::Operation>,
    ) -> Result<casual_doc_model::v1::StyleId, String> {
        use casual_doc_edit::Operation;
        use casual_doc_layout::fold::heading_level_from_name;
        use casual_doc_model::v1::{
            ParagraphProperties, RunProperties, Spacing, Style, StyleId, StyleKind,
        };
        let level = level.clamp(1, 6);
        let is_heading = |style: &Style| {
            style.kind == StyleKind::Paragraph
                && style
                    .name
                    .as_deref()
                    .is_some_and(|name| name.trim().to_ascii_lowercase().starts_with("heading"))
                && heading_level_from_name(style.name.as_deref()) == Some(level)
        };
        if let Some(id) = self
            .document
            .definitions()
            .styles
            .iter()
            .find(|(_, style)| is_heading(style))
            .map(|(id, _)| *id)
        {
            return Ok(id);
        }
        if let Some(id) = install.iter().find_map(|op| match op {
            Operation::SetStyleDefinition {
                id,
                style: Some(style),
            } if is_heading(style) => Some(*id),
            _ => None,
        }) {
            return Ok(id);
        }
        let based_on = self
            .document
            .definitions()
            .styles
            .iter()
            .find(|(_, style)| style.kind == StyleKind::Paragraph && style.is_default)
            .map(|(id, _)| *id);
        let id = StyleId::new(
            self.edit_ids
                .next_id()
                .map_err(|_| "id space exhausted".to_owned())?,
        );
        let style = Style {
            kind: StyleKind::Paragraph,
            is_default: false,
            name: Some(format!("Heading {level}")),
            aliases: None,
            based_on,
            next: based_on,
            link: None,
            hidden: false,
            ui_priority: Some(9),
            semi_hidden: false,
            unhide_when_used: true,
            q_format: true,
            locked: false,
            paragraph: Some(ParagraphProperties {
                outline_level: Some(level - 1),
                keep_next: Some(true),
                spacing: Some(Spacing {
                    before_twips: Some(240),
                    after_twips: Some(80),
                    ..Spacing::default()
                }),
                ..ParagraphProperties::default()
            }),
            run: Some(RunProperties {
                bold: Some(true),
                size_half_points: Some(PASTED_HEADING_HALF_POINTS[usize::from(level - 1)]),
                ..RunProperties::default()
            }),
            table: None,
            table_row: None,
            table_cell: None,
            conditional: Vec::new(),
            custom_style: false,
        };
        install.push(Operation::SetStyleDefinition {
            id,
            style: Some(Box::new(style)),
        });
        Ok(id)
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

    fn heading_styles(document: &WasmDocument) -> Vec<String> {
        let mut names: Vec<String> = document
            .document
            .definitions()
            .styles
            .iter()
            .filter_map(|(_, style)| style.name.clone())
            .filter(|name| name.starts_with("Heading"))
            .collect();
        names.sort();
        names
    }

    /// TA-11: a pasted `<h2>` and `<h3>` become Heading 2 and Heading 3, the
    /// styles are created once and in the paste's own undo step.
    #[test]
    fn pasted_headings_take_heading_styles_created_once_and_undone_with_the_paste() {
        let mut document = wasm_document(import(&p("x")));
        let first = all_paragraph_ids(&document)[0].to_string();
        assert!(
            heading_styles(&document).is_empty(),
            "the fixture has no heading styles"
        );
        let fragment = r#"{"blocks":[
            {"kind":"paragraph","runs":[{"text":"Two"}],"heading":2},
            {"kind":"paragraph","runs":[{"text":"Three"}],"heading":3},
            {"kind":"paragraph","runs":[{"text":"Again"}],"heading":2},
            {"kind":"paragraph","runs":[{"text":"Body"}]}
        ]}"#;
        document
            .paste_external_structured(&first, 0, &first, 0, fragment.to_owned())
            .expect("paste");
        assert_eq!(heading_styles(&document), ["Heading 2", "Heading 3"]);
        let ids = all_paragraph_ids(&document);
        let levels: Vec<Option<u8>> = document
            .block_outline(ids[0], ids[3])
            .into_iter()
            .map(|entry| entry.heading)
            .collect();
        assert_eq!(levels, [Some(2), Some(3), Some(2), None]);

        // A second paste reuses the styles the first one created.
        let last = ids.last().expect("paragraphs").to_string();
        document
            .paste_external_structured(&last, 0, &last, 0, fragment.to_owned())
            .expect("paste again");
        assert_eq!(heading_styles(&document), ["Heading 2", "Heading 3"]);

        document.undo().expect("undo");
        document.undo().expect("undo");
        assert!(
            heading_styles(&document).is_empty(),
            "undo removes the created styles"
        );
    }

    #[test]
    fn an_unknown_endpoint_is_an_empty_outline() {
        let document = mixed();
        assert_eq!(document.copy_block_outline("nope", 0, "nope", 0), "[]");
    }
}
