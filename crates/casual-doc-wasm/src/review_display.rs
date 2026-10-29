//! Review ▸ Display for review ▸ **Original**: the document as if every tracked
//! change were rejected (`docs/153` `review.display-mode-original`, `docs/109`
//! RM-31).
//!
//! # Why a copy, and why rejection
//!
//! Word's Original view is the document before review — every insertion gone,
//! every deletion back, every formatting change undone, every moved paragraph
//! where it came from. That is not a paint style; it is **Reject All**, and this
//! engine already has exactly one implementation of it
//! (`casual_doc_review::lower::decide_all_revisions`), which handles paragraph
//! marks, moves, formatting changes and every story. A second mechanism — a
//! layout mode that hides insertions and un-strikes deletions — would have to
//! re-derive each of those, and would disagree with Reject All the first time
//! one of them changed (ADR-061's reason for rejecting a second painter).
//!
//! So the host opens a **throwaway copy** of the document (the same bytes a save
//! would write, as the redline view does, ADR-065), calls [`show_as_original`]
//! on it, and puts that copy on the canvas read-only. The reader's document is
//! never touched: Original is a view, and leaving it puts the live document back.
//!
//! [`show_as_original`]: WasmDocument::show_as_original
//!
//! # Protection
//!
//! A copy restricted to `trackedChanges` or `readOnly` refuses Reject All, which
//! is the restriction doing its job on a document someone is editing — and wrong
//! here, where nothing will ever be saved. The copy's restriction is lifted first,
//! as its own step, because a batch is judged on its worst operation (ADR-052)
//! and the lift is the one operation exempt from the restriction it lifts
//! (ADR-059).
//!
//! # Complexity
//!
//! O(document): one Reject All, once, when the reader picks Original. Never per
//! interaction.

use casual_doc_edit::Operation;
use wasm_bindgen::prelude::*;

use casual_doc_review::lower as review_lower;

use crate::{HistoryKind, WasmDocument, to_js};

#[wasm_bindgen]
impl WasmDocument {
    /// Turns THIS handle — which must be a throwaway copy — into Word's Original
    /// view: every tracked change rejected, any editing restriction lifted
    /// first. Returns whether there was anything to reject.
    ///
    /// # Errors
    ///
    /// When Reject All itself refuses (an incomplete tracked move), with its
    /// sentence; the host then stays on the live document and says why.
    #[wasm_bindgen(js_name = showAsOriginal)]
    pub fn show_as_original(&mut self) -> Result<bool, JsValue> {
        self.show_as_original_inner().map_err(to_js)
    }
}

impl WasmDocument {
    /// [`WasmDocument::show_as_original`] before the `JsValue` boundary.
    pub(crate) fn show_as_original_inner(&mut self) -> Result<bool, String> {
        if self
            .document
            .definitions()
            .settings
            .document_protection
            .is_some()
        {
            self.apply_action_as(
                vec![Operation::SetDocumentProtection { protection: None }],
                HistoryKind::DocumentProperties,
            )?;
        }
        match review_lower::decide_all_revisions(&self.document, false) {
            Ok(lowered) => {
                self.commit_review(lowered)?;
                Ok(true)
            }
            // The one refusal that means "nothing to do": the original IS the
            // document. Matched on the code, not on the English sentence.
            Err(refusal) if refusal.contains("review.no-revisions") => Ok(false),
            Err(refusal) => Err(refusal),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use casual_doc_export::write_document;
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::{
        BlockNode, Definitions, Document, DocumentProtection, DocumentProtectionEdit, InlineNode,
        Paragraph, ParagraphProperties, Revision, RevisionKind, Run, RunProperties,
    };

    use crate::open_document;

    fn id(n: u64) -> NodeId {
        NodeId::from_parts(n, 177).unwrap()
    }

    fn run(n: u64, text: &str) -> InlineNode {
        InlineNode::Run(Run {
            id: id(n),
            properties: RunProperties::default().into(),
            text: text.to_owned(),
        })
    }

    fn revision(n: u64, kind: RevisionKind, text: &str) -> InlineNode {
        InlineNode::Revision(Revision {
            id: id(n),
            kind,
            author: Some("Reviewer".to_owned()),
            date: None,
            revision_id: Some(n.to_string()),
            editor_group: None,
            inlines: vec![run(n + 1, text)],
        })
    }

    /// "Keep [+added] [-removed]" with a tracked-changes restriction, as a DOCX.
    fn reviewed_docx() -> Vec<u8> {
        let mut definitions = Definitions::default();
        definitions.settings.document_protection = Some(DocumentProtection {
            edit: DocumentProtectionEdit::TrackedChanges,
            enforcement: true,
            formatting: false,
            password: None,
        });
        let document = Document::new(
            id(1),
            vec![BlockNode::Paragraph(Paragraph {
                id: id(10),
                properties: ParagraphProperties::default().into(),
                inlines: vec![
                    run(11, "Keep "),
                    revision(20, RevisionKind::Insertion, "added "),
                    revision(30, RevisionKind::Deletion, "removed"),
                ],
            })],
            definitions,
        )
        .expect("valid");
        write_document(&document, &BTreeMap::new()).expect("writes")
    }

    fn body_text(document: &crate::WasmDocument) -> String {
        let mut out = String::new();
        for block in document.document.body() {
            if let BlockNode::Paragraph(paragraph) = block {
                collect(&paragraph.inlines, &mut out);
            }
        }
        out
    }

    fn collect(inlines: &[InlineNode], out: &mut String) {
        for inline in inlines {
            match inline {
                InlineNode::Run(run) => out.push_str(&run.text),
                InlineNode::Revision(revision) => collect(&revision.inlines, out),
                _ => {}
            }
        }
    }

    /// The copy reads as the document before review — the insertion gone, the
    /// deletion back as plain text, no revision left — even though the file
    /// restricts editing to tracked changes, and a second call has nothing to do.
    /// Mutation: drop the protection lift -> Reject All is refused by the
    /// restriction and `expect("original")` panics with its sentence.
    #[test]
    fn the_original_view_rejects_every_change_on_a_protected_copy() {
        let mut copy = open_document(&reviewed_docx()).expect("opens");
        assert_eq!(
            body_text(&copy),
            "Keep added removed",
            "precondition: both changes are there"
        );
        assert!(copy.show_as_original_inner().expect("original"));
        assert_eq!(body_text(&copy), "Keep removed");
        assert!(
            !format!("{:?}", copy.document.body()).contains("Revision("),
            "no tracked change is left to paint"
        );
        assert!(
            !copy.show_as_original_inner().expect("again"),
            "nothing left to reject"
        );
    }
}
