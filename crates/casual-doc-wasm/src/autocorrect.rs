// SPDX-License-Identifier: Apache-2.0

//! The one engine question AutoCorrect asks that the editing API did not
//! answer: is this text already inside a hyperlink (`docs/153`
//! `text.autoformat-hyperlink`, `docs/168` batch 4, `docs/109` RM-19).
//!
//! A typed URL becomes a link when the word ends — but not a URL typed inside
//! a link that already exists, which would re-target it. ONLYOFFICE asks the
//! same question first (`oRun.IsInHyperlink()`,
//! `sdkjs/word/Editor/Paragraph/Run/RunAutoCorrect.js:704`). Everything else
//! AutoCorrect does is plain text in, an existing edit out (`replaceSelection`,
//! `setHyperlink`), so this is the whole of its engine surface.

use super::*;

/// Whether any hyperlink in paragraph `node` overlaps `[start, end)`, or holds
/// the collapsed position `start` when `start == end`. Offsets are the editing
/// byte space `setHyperlink` takes. `false` for an unknown node.
///
/// Complexity: O(inlines in the paragraph); never O(document).
pub(crate) fn hyperlink_touches_inner(
    document: &Document,
    node: &str,
    start: u32,
    end: u32,
) -> Result<bool, String> {
    let id = NodeId::from_str(node).map_err(|_| format!("`{node}` is not a node id"))?;
    let paragraph =
        find_paragraph_any(document, id).ok_or_else(|| format!("`{node}` is not a paragraph"))?;
    let notes = NoteAnchorLengths::of(document);
    Ok(paragraph_links(&notes, paragraph).iter().any(|link| {
        let (a, b) = (link.range.start.offset, link.range.end.offset);
        if start == end {
            a < start && start <= b
        } else {
            a < end && start < b
        }
    }))
}

#[wasm_bindgen]
impl WasmDocument {
    /// Whether a hyperlink already covers `[start, end)` of paragraph `node`
    /// (see `hyperlink_touches_inner`). AutoCorrect asks before it links a
    /// typed URL, so it never re-targets a link the user is typing inside.
    #[wasm_bindgen(js_name = hyperlinkTouches)]
    #[must_use]
    pub fn hyperlink_touches(&self, node: &str, start: u32, end: u32) -> bool {
        hyperlink_touches_inner(&self.document, node, start, end).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_range_inside_an_existing_link_is_reported_and_one_beside_it_is_not() {
        let mut d = open_document(b"see https://a.example now").unwrap();
        let mut nodes = Vec::new();
        collect_block_text(d.document.body(), &mut nodes);
        let node = nodes[0].0;
        let id = node.to_string();
        assert!(!hyperlink_touches_inner(&d.document, &id, 4, 21).unwrap());
        let ids = &mut d.edit_ids;
        let ops = casual_doc_command::links::set_hyperlink(
            node,
            4,
            21,
            "https://a.example",
            None,
            &mut || ids.next_id().ok(),
        )
        .unwrap();
        d.apply_action(ops).unwrap();
        assert!(hyperlink_touches_inner(&d.document, &id, 4, 21).unwrap());
        assert!(hyperlink_touches_inner(&d.document, &id, 10, 12).unwrap());
        assert!(
            hyperlink_touches_inner(&d.document, &id, 21, 21).unwrap(),
            "a caret at the link's end is typing into it"
        );
        assert!(!hyperlink_touches_inner(&d.document, &id, 0, 3).unwrap());
        assert!(!hyperlink_touches_inner(&d.document, &id, 22, 25).unwrap());
        assert!(!hyperlink_touches_inner(&d.document, &id, 4, 4).unwrap());
        assert!(hyperlink_touches_inner(&d.document, "nope", 0, 1).is_err());
    }
}
