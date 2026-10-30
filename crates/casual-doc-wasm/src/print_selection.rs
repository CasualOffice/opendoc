// SPDX-License-Identifier: Apache-2.0

//! Print ▸ Selection (`docs/153` `shell.print-selection`): a throwaway copy of
//! the document that holds only the selected content, for the print path to
//! paginate and print.
//!
//! # The shape, and why
//!
//! Word prints a selection by flowing ONLY the selected content onto fresh
//! pages from the top of page one, with the document's page setup, headers and
//! footers; ONLYOFFICE does the same (`Asc.c_oAscPrintType.Selection`, set by
//! the document context menu's `txtPrintSelection` in
//! `web-apps/apps/documenteditor/main/app/view/DocumentHolder.js`). That is a
//! *document*, not a crop of the painted pages, so the copy is built the way the
//! redline view builds its copy (`compare_view.rs`): a normalized snapshot of
//! this session opened as a second session the host prints and frees. The live
//! session is never edited, so nothing reaches its undo stack, its dirty state
//! or a shared room.
//!
//! What the copy keeps: the selected text with every property it had, whole
//! tables the selection crosses (Word keeps a table whole when a selection
//! crosses it, as `copyStructured` does), the section setup, headers, footers,
//! styles, numbering and pictures. What it removes: every top-level block before
//! the selection's first and after its last, and the parts of the boundary
//! paragraphs outside it.
//!
//! **Complexity**: O(document), once per print — the snapshot, the second
//! session's layout and the trim. Printing is O(document) anyway.

use casual_doc_edit::{Operation, Pos, Range as EditRange, refused};
use casual_doc_io::{FormatId, FormatSelection, formats};
use casual_doc_model::v1::BlockNode;
use wasm_bindgen::prelude::*;

use crate::{HistoryKind, WasmDocument, block_holds, open_document_as, to_js};

/// The refusal for an empty selection.
const NOTHING_SELECTED: &str = refused!("print.selection-empty", "Select the text to print first.");

/// The refusal for a selection outside the body.
const BODY_ONLY: &str = refused!(
    "print.selection-body-only",
    "Only a selection in the document body can be printed on its own."
);

#[wasm_bindgen]
impl WasmDocument {
    /// A new document holding only the selection `[start, end)`, laid out and
    /// ready for `printDocument`. The caller frees it after printing. The live
    /// document is not changed.
    #[wasm_bindgen(js_name = selectionDocument)]
    pub fn selection_document(
        &self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> Result<WasmDocument, JsValue> {
        self.selection_document_inner(start_node, start_offset, end_node, end_offset)
            .map_err(to_js)
    }
}

impl WasmDocument {
    pub(crate) fn selection_document_inner(
        &self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> Result<WasmDocument, String> {
        let (start, end) = self.order_endpoints(start_node, start_offset, end_node, end_offset)?;
        if start == end {
            return Err(NOTHING_SELECTED.to_owned());
        }
        let body = self.document.body();
        let (Some(first), Some(last)) = (
            body.iter().position(|block| block_holds(block, start.node)),
            body.iter().position(|block| block_holds(block, end.node)),
        ) else {
            return Err(BODY_ONLY.to_owned());
        };
        let snapshot = self
            .document
            .to_json()
            .map_err(|error| format!("snapshot: {error}"))?;
        let format = FormatId::new(formats::NORMALIZED_JSON)
            .map_err(|error| format!("format id: {error}"))?;
        let mut copy = open_document_as(&snapshot, FormatSelection::Explicit(format))?;
        copy.resources = self.resources.clone();

        let blocks = copy.document.body().len();
        let paragraph_end = |block: &BlockNode, node| match block {
            BlockNode::Paragraph(paragraph) if paragraph.id == node => Some(
                paragraph
                    .inlines
                    .iter()
                    .map(casual_doc_edit::inline_text_len)
                    .sum::<u32>(),
            ),
            _ => None,
        };
        let mut ops = Vec::new();
        // Tail first, then head: removing what follows the selection never
        // moves an offset or an index before it.
        if let Some(length) = paragraph_end(&copy.document.body()[last], end.node)
            && end.offset < length
        {
            ops.push(Operation::DeleteText {
                range: EditRange {
                    start: end,
                    end: Pos::new(end.node, length),
                },
            });
        }
        if paragraph_end(&copy.document.body()[first], start.node).is_some() && start.offset > 0 {
            ops.push(Operation::DeleteText {
                range: EditRange {
                    start: Pos::new(start.node, 0),
                    end: start,
                },
            });
        }
        if last + 1 < blocks {
            ops.push(Operation::DeleteBlocks {
                container: None,
                index: u32::try_from(last + 1).map_err(|_| "too many blocks".to_owned())?,
                count: u32::try_from(blocks - last - 1)
                    .map_err(|_| "too many blocks".to_owned())?,
            });
        }
        if first > 0 {
            ops.push(Operation::DeleteBlocks {
                container: None,
                index: 0,
                count: u32::try_from(first).map_err(|_| "too many blocks".to_owned())?,
            });
        }
        if !ops.is_empty() {
            copy.apply_action_as(ops, HistoryKind::Delete)?;
        }
        Ok(copy)
    }
}

#[cfg(test)]
mod tests {
    use crate::open_document;

    fn paragraphs(document: &crate::WasmDocument) -> Vec<String> {
        let pairs: Vec<(String, String)> =
            serde_json::from_str(&document.searchable_text_inner()).expect("pairs");
        pairs.into_iter().map(|(_, text)| text).collect()
    }

    fn ids(document: &crate::WasmDocument) -> Vec<String> {
        let pairs: Vec<(String, String)> =
            serde_json::from_str(&document.searchable_text_inner()).expect("pairs");
        pairs.into_iter().map(|(id, _)| id).collect()
    }

    #[test]
    fn the_copy_holds_only_the_selection_and_the_live_document_is_untouched() {
        let document = open_document(b"one two\nthree four\nfive six\nseven").expect("opens");
        let node = ids(&document);
        // From "two" in the first paragraph to "five" in the third.
        let copy = document
            .selection_document_inner(&node[0], 4, &node[2], 4)
            .expect("a selection prints");
        assert_eq!(paragraphs(&copy), ["two", "three four", "five"]);
        assert_eq!(
            paragraphs(&document),
            ["one two", "three four", "five six", "seven"],
            "the session being printed from is never edited"
        );
        assert!(copy.page_count() >= 1);
    }

    #[test]
    fn a_selection_inside_one_paragraph_keeps_just_that_text() {
        let document = open_document(b"alpha beta gamma\nnext").expect("opens");
        let node = ids(&document);
        let copy = document
            .selection_document_inner(&node[0], 6, &node[0], 10)
            .expect("prints");
        assert_eq!(paragraphs(&copy), ["beta"]);
    }

    #[test]
    fn a_backward_selection_is_the_same_selection() {
        let document = open_document(b"one\ntwo\nthree").expect("opens");
        let node = ids(&document);
        let copy = document
            .selection_document_inner(&node[2], 2, &node[1], 1)
            .expect("prints");
        assert_eq!(paragraphs(&copy), ["wo", "th"]);
    }

    #[test]
    fn an_empty_selection_is_refused_with_a_reason() {
        let document = open_document(b"one").expect("opens");
        let node = ids(&document);
        let error = document
            .selection_document_inner(&node[0], 1, &node[0], 1)
            .expect_err("refused");
        assert!(error.contains("print.selection-empty"), "{error}");
    }
}
