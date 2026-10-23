//! Formatting as you type: the engine half of `docs/174` TA-05.
//!
//! Word, Google Docs and ONLYOFFICE turn a paragraph that starts `1. ` or `- `
//! into a real list the moment the space is typed, and an immediate Undo gives
//! the literal text back. That only works if the conversion — removing the typed
//! marker AND making the paragraph a list item — is ONE undoable action, separate
//! from the typing burst before it. Two host calls (`deleteRange`, then
//! `toggleList`) would be two undo steps, and the first Undo would leave a list
//! item with its marker already gone.
//!
//! The host (`webapp/src/input_rules.mjs`) decides WHEN a marker was typed; this
//! module only performs the conversion, and refuses anything it cannot do
//! faithfully rather than half-doing it.
//!
//! # Complexity
//!
//! O(document) for the one paragraph lookup, like every by-id command; never per
//! keystroke — it runs once, on the space that completes a marker.

use casual_doc_edit::refused;
use casual_doc_edit::{Operation, Pos, Range as EditRange, paragraph_properties};
use casual_doc_model::NodeId;
use casual_doc_model::v1::NumberingRef;
use wasm_bindgen::prelude::*;

use crate::{EditResult, HistoryKind, WasmDocument, node_id, to_js};

#[wasm_bindgen]
impl WasmDocument {
    /// Converts the paragraph `node`, whose text starts with a typed list marker
    /// of `marker_bytes` UTF-8 bytes (`"1. "`, `"- "`), into a `"numbered"` or
    /// `"bullet"` list item: the marker is removed and the paragraph joins the
    /// editor's shared list of that kind, as ONE undoable action whose Undo
    /// restores the literal marker. The caret lands where it was, less the marker
    /// (`caret_offset` is the caret's byte offset before the conversion).
    ///
    /// Refused, changing nothing, when the paragraph is already a list item, when
    /// `marker_bytes` is not a character boundary inside the paragraph's text,
    /// when the caret is before the marker's end, and while Track Changes is on (a
    /// tracked conversion would need the deletion and the property change tracked
    /// together; the host does not ask then, and the engine does not guess).
    #[wasm_bindgen(js_name = autoFormatList)]
    pub fn auto_format_list(
        &mut self,
        node: &str,
        marker_bytes: u32,
        caret_offset: u32,
        kind: &str,
    ) -> Result<EditResult, JsValue> {
        let nid = node_id(node)?;
        self.auto_format_list_in(nid, marker_bytes, caret_offset, kind == "numbered")
            .map_err(to_js)
    }
}

impl WasmDocument {
    /// [`auto_format_list`](Self::auto_format_list) with the node resolved and
    /// the refusal as a plain message, so it is testable off the wasm target.
    pub(crate) fn auto_format_list_in(
        &mut self,
        nid: NodeId,
        marker_bytes: u32,
        caret_offset: u32,
        numbered: bool,
    ) -> Result<EditResult, String> {
        let refuse = || -> String {
            refused!(
                "autoformat.list-not-here",
                "This paragraph cannot become a list automatically."
            )
            .into()
        };
        if self.track_revisions() {
            return Err(refuse());
        }
        let Some(properties) = paragraph_properties(&self.document, nid) else {
            return Err(refuse());
        };
        if properties.numbering.is_some() {
            return Err(refuse());
        }
        let text = self.paragraph_text(nid);
        let marker = marker_bytes as usize;
        if marker == 0
            || marker > text.len()
            || !text.is_char_boundary(marker)
            || caret_offset < marker_bytes
            || caret_offset as usize > text.len()
        {
            return Err(refuse());
        }
        // Where Undo leaves the caret is decided by the LAST inverse it applies,
        // which is the inverse of the FIRST op here. So the first op deletes the
        // marker's final character on its own (its trailing space): a deletion
        // inside one run inverts to an `InsertText`, whose caret is after the text
        // it restores, so Undo lands after "1. ", ready to keep typing. Deleting
        // the whole marker as one op removes the whole run when the paragraph is
        // just "1. ", which inverts to a run snapshot whose caret is the paragraph
        // start; and a numbering definition first would put it at the document
        // start. The rest of the marker goes second.
        let last = text[..marker]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index) as u32;
        let mut ops = vec![Operation::DeleteText {
            range: EditRange {
                start: Pos::new(nid, last),
                end: Pos::new(nid, marker_bytes),
            },
        }];
        if last > 0 {
            ops.push(Operation::DeleteText {
                range: EditRange {
                    start: Pos::new(nid, 0),
                    end: Pos::new(nid, last),
                },
            });
        }
        let (instance, mut install) = self.ensure_list(numbered)?;
        ops.append(&mut install);
        let mut list_item = properties;
        list_item.numbering = Some(NumberingRef { instance, level: 0 });
        ops.push(Operation::SetParagraphProperties {
            node: nid,
            properties: Box::new(list_item),
        });
        self.apply_action_caret_as(
            ops,
            Pos::new(nid, caret_offset - marker_bytes),
            HistoryKind::ListFormatting,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::WasmDocument;
    use crate::tests::wasm_document;
    use casual_doc_model::v1::{BlockNode, Document};
    use std::str::FromStr;

    fn convert(
        document: &mut WasmDocument,
        id: &str,
        marker: u32,
        caret: u32,
        kind: &str,
    ) -> Result<(), String> {
        let nid = casual_doc_model::NodeId::from_str(id).expect("id");
        document
            .auto_format_list_in(nid, marker, caret, kind == "numbered")
            .map(drop)
    }

    fn document_with(text: &str) -> (WasmDocument, String) {
        let xml = format!(
            r#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p></w:body></w:document>"#
        );
        let document: Document = casual_doc_import::import_main_document_xml(
            xml.as_bytes(),
            casual_doc_import::ImportConfig::default(),
        )
        .expect("import")
        .document;
        let document = wasm_document(document);
        let id = match document.document.body().first() {
            Some(BlockNode::Paragraph(paragraph)) => paragraph.id.to_string(),
            _ => panic!("a paragraph"),
        };
        (document, id)
    }

    fn text_of(document: &WasmDocument, id: &str) -> String {
        let length = document.paragraph_length(id);
        document.copy_text(id, 0, id, length)
    }

    #[test]
    fn a_typed_marker_becomes_a_list_in_one_step_and_one_undo_gives_it_back() {
        let (mut document, id) = document_with("1. first");
        convert(&mut document, &id, 3, 3, "numbered").expect("convert");
        assert_eq!(text_of(&document, &id), "first");
        assert_eq!(document.list_style_at(&id), "numbered");

        let undone = document.undo().expect("undo");
        assert_eq!(
            (undone.node(), undone.offset()),
            (id.clone(), 3),
            "Undo leaves the caret after the restored marker, so typing continues the text"
        );
        assert_eq!(
            text_of(&document, &id),
            "1. first",
            "one Undo restores the literal marker"
        );
        assert_eq!(
            document.list_style_at(&id),
            "",
            "and the paragraph is no longer a list item"
        );
    }

    /// The real sequence: the marker is TYPED (one typing burst), converted,
    /// more text is typed, and two Undos come back to the literal marker with
    /// the caret after it.
    #[test]
    fn after_typing_on_two_undos_return_to_the_typed_marker_with_the_caret_after_it() {
        let (mut document, id) = document_with("");
        for (offset, key) in ["1", ".", " "].iter().enumerate() {
            let at = u32::try_from(offset).expect("small");
            document
                .type_text(&id, at, &id, at, (*key).to_owned(), 7)
                .expect("type");
        }
        convert(&mut document, &id, 3, 3, "numbered").expect("convert");
        for (offset, key) in ["Q", "Z"].iter().enumerate() {
            let at = u32::try_from(offset).expect("small");
            document
                .type_text(&id, at, &id, at, (*key).to_owned(), 8)
                .expect("type");
        }
        document.undo().expect("undo the typing");
        assert_eq!(text_of(&document, &id), "");
        let undone = document.undo().expect("undo the conversion");
        assert_eq!(text_of(&document, &id), "1. ");
        assert_eq!((undone.node(), undone.offset()), (id.clone(), 3));
    }

    #[test]
    fn a_bullet_marker_becomes_a_bullet_list() {
        let (mut document, id) = document_with("- ");
        convert(&mut document, &id, 2, 2, "bullet").expect("convert");
        assert_eq!(text_of(&document, &id), "");
        assert_eq!(document.list_style_at(&id), "bullet");
    }

    #[test]
    fn what_cannot_be_converted_faithfully_is_refused_and_changes_nothing() {
        let (mut document, id) = document_with("1. first");
        // Past the text, not a boundary, and the caret before the marker's end.
        assert!(convert(&mut document, &id, 40, 3, "numbered").is_err());
        assert!(convert(&mut document, &id, 3, 1, "numbered").is_err());
        assert_eq!(text_of(&document, &id), "1. first");
        // Already a list item.
        convert(&mut document, &id, 3, 3, "numbered").expect("convert");
        assert!(convert(&mut document, &id, 1, 1, "bullet").is_err());
    }
}
