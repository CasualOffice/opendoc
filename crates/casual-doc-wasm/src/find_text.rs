// SPDX-License-Identifier: Apache-2.0

//! The engine half of regular-expression Find & Replace (`docs/153`
//! `shell.find-regex`).
//!
//! # Why the pattern runs in JavaScript
//!
//! The browser already carries a complete, Unicode-aware regular-expression
//! engine (`RegExp` with the `u` flag), and it is the dialect a person typing
//! into a web page expects: `\d`, lookbehind, named groups, `$1` and `$<name>`
//! in the replacement. Linking the `regex` crate into the editor's module would
//! add a second dialect that differs from it in the corners (no lookaround, no
//! backreferences) and grow the download for no gain. So the engine does the two
//! things only it can do and the pattern stays in the page:
//!
//! - [`searchable_text`](WasmDocument::searchable_text): every text-bearing
//!   paragraph, on every surface, in the order `findText` scans them, with the
//!   shaped text that offsets are measured in. The page runs the pattern over
//!   each string and converts its UTF-16 indices to the engine's UTF-8 offsets.
//! - [`replace_ranges_each`](WasmDocument::replace_ranges_each): Replace All
//!   where every match has its OWN replacement (`$1` differs per match), as one
//!   undoable action, exactly like `replaceRanges` for a literal query.
//!
//! ONLYOFFICE's search panel offers the same option
//! (`web-apps/apps/common/main/lib/view/SearchPanel.js`, `textMatchUsingRegExp`)
//! and passes the pattern to the engine as a flag on the search settings.
//!
//! **Complexity**: `searchable_text` is O(document) once per scan, which is what
//! the literal scan already costs; `replace_ranges_each` is O(matches) ordering
//! tests plus one transaction.

use casual_doc_edit::Operation;
use wasm_bindgen::prelude::*;

use crate::{EditResult, HistoryKind, WasmDocument, collect_block_text_all_surfaces, to_js};

#[wasm_bindgen]
impl WasmDocument {
    /// Every text-bearing paragraph as JSON `[[node, text], …]`, in the order
    /// `findText` scans them (every surface: body, headers, footers, notes,
    /// text boxes). Offsets into `text` are UTF-8 bytes, the unit every
    /// position in this API uses. O(document).
    #[wasm_bindgen(js_name = searchableText)]
    #[must_use]
    pub fn searchable_text(&self) -> String {
        self.searchable_text_inner()
    }

    /// Replaces MANY ranges, each with its own text, as ONE undoable action:
    /// regular-expression Replace All, where `$1` expands differently per
    /// match. The arrays are parallel and the ranges must arrive in DESCENDING
    /// document order (last match first), the contract `replaceRanges` has, so
    /// applying one never shifts a range not yet applied.
    #[wasm_bindgen(js_name = replaceRangesEach)]
    pub fn replace_ranges_each(
        &mut self,
        start_nodes: Vec<String>,
        start_offsets: Vec<u32>,
        end_nodes: Vec<String>,
        end_offsets: Vec<u32>,
        texts: Vec<String>,
    ) -> Result<EditResult, JsValue> {
        self.replace_ranges_each_inner(
            &start_nodes,
            &start_offsets,
            &end_nodes,
            &end_offsets,
            &texts,
        )
        .map_err(to_js)
    }
}

impl WasmDocument {
    pub(crate) fn searchable_text_inner(&self) -> String {
        let mut nodes = Vec::new();
        collect_block_text_all_surfaces(&self.document, &mut nodes);
        let pairs: Vec<(String, String)> = nodes
            .into_iter()
            .map(|(id, text)| (id.to_string(), text))
            .collect();
        serde_json::to_string(&pairs).unwrap_or_else(|_| "[]".to_owned())
    }

    pub(crate) fn replace_ranges_each_inner(
        &mut self,
        start_nodes: &[String],
        start_offsets: &[u32],
        end_nodes: &[String],
        end_offsets: &[u32],
        texts: &[String],
    ) -> Result<EditResult, String> {
        let n = start_nodes.len();
        if start_offsets.len() != n
            || end_nodes.len() != n
            || end_offsets.len() != n
            || texts.len() != n
        {
            return Err("replaceRangesEach: mismatched range arrays".into());
        }
        if n == 0 {
            return Err("nothing to do".into());
        }
        let mut ops = Vec::new();
        for (index, text) in texts.iter().enumerate() {
            let (start, _end) = self.order_endpoints(
                &start_nodes[index],
                start_offsets[index],
                &end_nodes[index],
                end_offsets[index],
            )?;
            ops.extend(self.selection_delete_ops(
                &start_nodes[index],
                start_offsets[index],
                &end_nodes[index],
                end_offsets[index],
            )?);
            if !text.is_empty() {
                ops.push(Operation::InsertText {
                    at: start,
                    text: text.clone(),
                });
            }
        }
        if ops.is_empty() {
            return Err("nothing to do".into());
        }
        self.apply_action_as(ops, HistoryKind::Replace)
    }
}

#[cfg(test)]
mod tests {
    use crate::open_document;

    fn texts(json: &str) -> Vec<(String, String)> {
        serde_json::from_str(json).expect("the searchable text is JSON pairs")
    }

    #[test]
    fn the_searchable_text_is_every_paragraph_in_scan_order() {
        let document = open_document(b"alpha 12\nbeta 345\n").expect("opens as text");
        let pairs = texts(&document.searchable_text_inner());
        let only_text: Vec<&str> = pairs.iter().map(|(_, text)| text.as_str()).collect();
        assert_eq!(only_text, ["alpha 12", "beta 345", ""]);
        assert!(pairs.iter().all(|(id, _)| !id.is_empty()));
    }

    #[test]
    fn each_range_takes_its_own_text_in_one_undoable_action() {
        let mut document = open_document(b"a 12\nb 345").expect("opens as text");
        let pairs = texts(&document.searchable_text_inner());
        let (first, second) = (pairs[0].0.clone(), pairs[1].0.clone());
        // Descending order: the second paragraph's match first.
        document
            .replace_ranges_each_inner(
                &[second.clone(), first.clone()],
                &[2, 2],
                &[second, first],
                &[5, 4],
                &["<345>".to_owned(), "<12>".to_owned()],
            )
            .expect("replaces");
        let after = texts(&document.searchable_text_inner());
        assert_eq!(after[0].1, "a <12>");
        assert_eq!(after[1].1, "b <345>");
        document.undo_inner().expect("undoes");
        let undone = texts(&document.searchable_text_inner());
        assert_eq!(
            (undone[0].1.as_str(), undone[1].1.as_str()),
            ("a 12", "b 345"),
            "one Ctrl+Z puts every replacement back"
        );
    }

    #[test]
    fn mismatched_arrays_are_refused_with_a_reason() {
        let mut document = open_document(b"a").expect("opens as text");
        let error = document
            .replace_ranges_each_inner(&["x".to_owned()], &[0], &[], &[], &[])
            .expect_err("refused");
        assert!(error.contains("mismatched"), "{error}");
    }
}
