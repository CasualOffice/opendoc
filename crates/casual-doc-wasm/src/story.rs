// SPDX-License-Identifier: Apache-2.0

//! The story contract: every caret position belongs to exactly one STORY — the
//! body, one header, one footer, one note body or one text box — and nothing
//! the keyboard does moves the caret, a selection or an edit out of it
//! (`docs/109` UX-057, audit CRITIC-01).
//!
//! The flat paragraph order (`ordered_paragraphs`) appends every header,
//! footer and note body after the body, and the order of two stories in it is
//! arbitrary. Every "the first / last / next paragraph" read of that list
//! therefore leaks: Ctrl+End took the LAST paragraph of the whole list, which
//! is a footer, and typing then edited the footer of every page with no header
//! mode, no dim and no message. Word, Google Docs and ONLYOFFICE all send
//! Ctrl+End to the end of the main body; a header or footer is entered only
//! deliberately (double-click, Insert ▸ Header).
//!
//! So the document edges are the BODY's edges, and a story's own edges are a
//! separate, explicit question the host asks only while that story is open.
//!
//! Complexity: each query is one walk of every story's paragraphs, O(document),
//! the same as the `ordered_paragraphs` read it replaces; nothing here runs per
//! keystroke of typing, only on a navigation key.

use casual_doc_edit::Pos;
use casual_doc_model::NodeId;
use casual_doc_model::v1::BlockNode;
use std::str::FromStr;
use wasm_bindgen::prelude::*;

use crate::{Caret, WasmDocument, empty_paragraph_block, node_plain_text};

/// The index of the body in `surface_block_lists` (it is always first).
pub(crate) const BODY_STORY: usize = 0;

impl WasmDocument {
    /// Where the caret lands after a whole-block paste at body block
    /// `block_index` (`docs/109` UX-063, audit CRITIC-03): at the END of what
    /// was pasted, as in Word, Docs and ONLYOFFICE. It used to land at the
    /// START of the first pasted block, so the next keystrokes went in front of
    /// everything just pasted.
    ///
    /// A paste that ends in a paragraph puts the caret at that paragraph's end.
    /// One that ends in a table puts it in the paragraph AFTER the table: the
    /// caret's own paragraph when the paste went in front of it (`start` at
    /// offset 0), the split-off tail when the paste split it (`split`), else
    /// the next body paragraph — and when there is none (end of the body, or
    /// another table), an empty paragraph is appended to `blocks`, which is
    /// what Word does, so the caret is never stranded inside the table.
    /// O(pasted blocks) plus one body lookup.
    pub(crate) fn paste_landing(
        &mut self,
        blocks: &mut Vec<BlockNode>,
        start: Pos,
        block_index: usize,
        split: Option<NodeId>,
    ) -> Result<Pos, String> {
        if let Some(BlockNode::Paragraph(last)) = blocks.last() {
            return Ok(Pos::new(
                last.id,
                node_plain_text(&last.inlines).len() as u32,
            ));
        }
        if start.offset == 0 {
            return Ok(Pos::new(start.node, 0));
        }
        if let Some(tail) = split {
            return Ok(Pos::new(tail, 0));
        }
        if let Some(BlockNode::Paragraph(next)) = self.document.body().get(block_index + 1) {
            return Ok(Pos::new(next.id, 0));
        }
        let filler = empty_paragraph_block(&mut self.edit_ids)?;
        let BlockNode::Paragraph(paragraph) = &filler else {
            return Err("an empty paragraph is a paragraph".to_owned());
        };
        let landing = Pos::new(paragraph.id, 0);
        blocks.push(filler);
        Ok(landing)
    }

    /// The story index `node` belongs to, with the ordered paragraphs, so a
    /// caller can take that story's edges without a second walk. `None` when
    /// `node` is not a paragraph of any story.
    pub(crate) fn story_of(&self, node: NodeId) -> (Option<usize>, Vec<(NodeId, u32, usize)>) {
        let paragraphs = self.ordered_paragraphs_by_surface();
        let story = paragraphs
            .iter()
            .find(|(id, _, _)| *id == node)
            .map(|(_, _, story)| *story);
        (story, paragraphs)
    }

    /// The first or last caret position of story `story`, or `None` when the
    /// story holds no paragraph.
    pub(crate) fn story_edge_of(
        paragraphs: &[(NodeId, u32, usize)],
        story: usize,
        toward_end: bool,
    ) -> Option<Caret> {
        let mut inside = paragraphs.iter().filter(|(_, _, s)| *s == story);
        let found = if toward_end {
            inside.next_back()
        } else {
            inside.next()
        };
        found.map(|(id, len, _)| Caret {
            node: id.to_string(),
            offset: if toward_end { *len } else { 0 },
        })
    }

    /// The edge of the BODY story — what `firstPosition` / `lastPosition`
    /// answer. Falls back to the document root when the body is empty.
    pub(crate) fn body_edge(&self, toward_end: bool) -> Caret {
        Self::story_edge_of(
            &self.ordered_paragraphs_by_surface(),
            BODY_STORY,
            toward_end,
        )
        .unwrap_or_else(|| Caret {
            node: self.document.id().to_string(),
            offset: 0,
        })
    }

    /// The kind of story index `story` is, by `surface_block_lists`' order:
    /// body, then every header, every footer, every footnote, every endnote,
    /// then every text box.
    fn story_kind_of(&self, story: usize) -> &'static str {
        let definitions = self.document.definitions();
        let bands = [
            ("header", definitions.headers.iter().count()),
            ("footer", definitions.footers.iter().count()),
            ("footnote", definitions.footnotes.iter().count()),
            ("endnote", definitions.endnotes.iter().count()),
        ];
        if story == BODY_STORY {
            return "body";
        }
        let mut first = 1;
        for (kind, count) in bands {
            if story < first + count {
                return kind;
            }
            first += count;
        }
        "textBox"
    }
}

#[wasm_bindgen]
impl WasmDocument {
    /// The first (`toward_end = false`) or last caret position of the story
    /// `node` is in — a header's own end while that header is open for editing,
    /// a note's end while the caret is in the note. The host asks this only for
    /// an OPEN story; the document's edges are `firstPosition`/`lastPosition`,
    /// which are always the body's. An unknown node answers the body's edge.
    #[wasm_bindgen(js_name = storyEdge)]
    #[must_use]
    pub fn story_edge(&self, node: &str, toward_end: bool) -> Caret {
        let Ok(id) = NodeId::from_str(node) else {
            return self.body_edge(toward_end);
        };
        let (story, paragraphs) = self.story_of(id);
        story
            .and_then(|story| Self::story_edge_of(&paragraphs, story, toward_end))
            .unwrap_or_else(|| self.body_edge(toward_end))
    }

    /// Which story `node` belongs to: `"body"`, `"header"`, `"footer"`,
    /// `"footnote"`, `"endnote"` or `"textBox"`, or `""` for a node that is no
    /// paragraph. The host uses it to keep navigation in the body unless a
    /// header or footer is deliberately open.
    #[wasm_bindgen(js_name = storyKind)]
    #[must_use]
    pub fn story_kind(&self, node: &str) -> String {
        let Ok(id) = NodeId::from_str(node) else {
            return String::new();
        };
        match self.story_of(id).0 {
            Some(story) => self.story_kind_of(story).to_owned(),
            None => String::new(),
        }
    }
}
