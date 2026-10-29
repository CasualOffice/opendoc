// SPDX-License-Identifier: Apache-2.0

//! Insert ▸ Text from file (`docs/153` `page.textFromFile`): another document's
//! body, merged in at the caret as ONE undoable action.
//!
//! # What Word does, and what this does
//!
//! Word's Insert ▸ Object ▸ Text from File (ONLYOFFICE: `btnTextFromFile`,
//! `web-apps/apps/documenteditor/main/app/view/Toolbar.js`) opens a DOCX, ODT,
//! RTF or text file and puts its content where the insertion point is, keeping
//! the source's formatting. A style the destination already has (by name) takes
//! the destination's definition, which is Word's "use destination styles" rule
//! for a name clash; a style it lacks is copied in.
//!
//! The merge is the structured-paste pipeline (`paste_structured`) with the one
//! thing a same-document paste never needs — **definition remapping**. Style,
//! numbering and media ids are minted per import, so the source's ids name
//! nothing (or the wrong thing) here:
//!
//! - **styles** match by kind and name, or are installed (their `basedOn` chain
//!   first) with fresh ids;
//! - **lists** get their own abstract definition and instance, so an inserted
//!   numbered list keeps its numbers and does not join a list of the
//!   destination's;
//! - **pictures** get a fresh media id and an editor-owned part name, and their
//!   bytes join this session's resources.
//!
//! What cannot travel is dropped and **reported** through `paste_loss`, never
//! silently: section breaks inside the inserted body (the destination's section
//! setup governs, as in Word), embedded objects and charts (their parts live in
//! the source package), and the reference markers structured paste already
//! drops (bookmarks, comments, notes, field ranges, moves).
//!
//! **Complexity**: O(source document) to open and walk it, plus O(styles) per
//! distinct style name matched. One transaction.

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_edit::clone::CloneReport;
use casual_doc_edit::{Operation, Pos, refused};
use casual_doc_io::{FormatId, FormatSelection};
use casual_doc_model::v1::{
    BlockNode, GroupChild, InlineNode, MediaId, MediaReference, NumberingInstanceId,
    ParagraphProperties, RunProperties, StyleId,
};
use casual_doc_model::v1::{InlineDescentMut, inline_descent_mut};
use wasm_bindgen::prelude::*;

use crate::{
    EditResult, HistoryKind, WasmDocument, block_holds, open_document, open_document_as, to_js,
};

const BODY_PARAGRAPH_ONLY: &str = refused!(
    "insertFile.body-paragraph-only",
    "A file can only be inserted at an insertion point in the document body."
);

const NOTHING_TO_INSERT: &str = refused!("insertFile.empty", "That file has no content to insert.");

#[wasm_bindgen]
impl WasmDocument {
    /// Inserts the body of the document in `bytes` at the collapsed caret
    /// `(node, offset)`, which must sit in a top-level body paragraph.
    /// `format_id` selects the importer (`"text.markdown"` for a `.md`); empty
    /// detects it from the bytes, as Open does. One undoable action; what could
    /// not be carried is in the result's `pasteLoss`.
    #[wasm_bindgen(js_name = insertDocumentFile)]
    pub fn insert_document_file(
        &mut self,
        node: &str,
        offset: u32,
        bytes: &[u8],
        format_id: &str,
    ) -> Result<EditResult, JsValue> {
        self.insert_document_file_inner(node, offset, bytes, format_id)
            .map_err(to_js)
    }
}

/// What the walk drops, beside the clone's own report.
#[derive(Default)]
struct Dropped {
    section_breaks: u32,
    objects: u32,
}

/// Source-to-destination id maps, plus the operations that install what the
/// destination lacked, in the order they must apply.
#[derive(Default)]
struct Remap {
    styles: BTreeMap<StyleId, Option<StyleId>>,
    numbering: BTreeMap<NumberingInstanceId, NumberingInstanceId>,
    media: BTreeMap<MediaId, MediaId>,
    media_ops: Vec<Operation>,
    numbering_ops: Vec<Operation>,
    style_ops: Vec<Operation>,
    in_progress: BTreeSet<StyleId>,
}

impl WasmDocument {
    pub(crate) fn insert_document_file_inner(
        &mut self,
        node: &str,
        offset: u32,
        bytes: &[u8],
        format_id: &str,
    ) -> Result<EditResult, String> {
        let (start, end) = self.order_endpoints(node, offset, node, offset)?;
        debug_assert_eq!(start, end);
        let body = self.document.body();
        let block_index = body
            .iter()
            .position(|block| block_holds(block, start.node))
            .ok_or_else(|| BODY_PARAGRAPH_ONLY.to_owned())?;
        let paragraph_len = match body.get(block_index) {
            Some(BlockNode::Paragraph(paragraph)) if paragraph.id == start.node => paragraph
                .inlines
                .iter()
                .map(casual_doc_edit::inline_text_len)
                .sum::<u32>(),
            _ => return Err(BODY_PARAGRAPH_ONLY.to_owned()),
        };

        let source = if format_id.is_empty() {
            open_document(bytes)?
        } else {
            let format =
                FormatId::new(format_id).map_err(|error| format!("invalid format id: {error}"))?;
            open_document_as(bytes, FormatSelection::Explicit(format))?
        };
        let mut blocks: Vec<BlockNode> = source.document.body().to_vec();
        if blocks.is_empty() {
            return Err(NOTHING_TO_INSERT.to_owned());
        }

        let mut remap = Remap::default();
        let mut dropped = Dropped::default();
        for block in &mut blocks {
            self.remap_block(block, &source, &mut remap, &mut dropped)?;
        }

        let mut report = CloneReport::default();
        let mut fresh = Vec::with_capacity(blocks.len());
        for block in &blocks {
            fresh.push(self.fresh_block(block, &mut report)?);
        }
        let caret = fresh
            .first()
            .map(crate::first_pos_of_block)
            .unwrap_or_else(|| Pos::new(start.node, 0));

        let mut ops = Vec::new();
        ops.append(&mut remap.media_ops);
        ops.append(&mut remap.numbering_ops);
        ops.append(&mut remap.style_ops);
        let index = if start.offset == 0 {
            block_index
        } else if start.offset >= paragraph_len {
            block_index + 1
        } else {
            let new_id = self
                .edit_ids
                .next_id()
                .map_err(|_| "id space exhausted".to_owned())?;
            ops.push(Operation::SplitParagraph {
                at: start,
                new_id,
                properties: None,
            });
            block_index + 1
        };
        ops.push(Operation::InsertBlocks {
            container: None,
            index: u32::try_from(index).map_err(|_| "too many blocks".to_owned())?,
            blocks: fresh,
        });
        let mut result = self.apply_action_caret_as(ops, caret, HistoryKind::Paste)?;
        let mut loss: Vec<String> = report.kinds().into_iter().map(str::to_owned).collect();
        if dropped.section_breaks > 0 {
            loss.push("sectionBreak".to_owned());
        }
        if dropped.objects > 0 {
            loss.push("embeddedObject".to_owned());
        }
        result.paste_loss = loss;
        Ok(result)
    }

    fn remap_block(
        &mut self,
        block: &mut BlockNode,
        source: &WasmDocument,
        remap: &mut Remap,
        dropped: &mut Dropped,
    ) -> Result<(), String> {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let mut properties = (*paragraph.properties).clone();
                self.remap_paragraph_properties(&mut properties, source, remap, dropped)?;
                paragraph.properties = properties.into();
                self.remap_inlines(&mut paragraph.inlines, source, remap, dropped)?;
            }
            BlockNode::Table(table) => {
                table.properties.style_ref =
                    self.map_style(table.properties.style_ref, source, remap)?;
                if let Some(change) = &mut table.properties.prop_change {
                    change.prior.style_ref =
                        self.map_style(change.prior.style_ref, source, remap)?;
                }
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        for inner in &mut cell.blocks {
                            self.remap_block(inner, source, remap, dropped)?;
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(run) = &mut sdt.properties.run_properties {
                    self.remap_run_properties(run, source, remap)?;
                }
                for inner in &mut sdt.blocks {
                    self.remap_block(inner, source, remap, dropped)?;
                }
            }
            BlockNode::AltChunk(_) => {}
        }
        Ok(())
    }

    fn remap_paragraph_properties(
        &mut self,
        properties: &mut ParagraphProperties,
        source: &WasmDocument,
        remap: &mut Remap,
        dropped: &mut Dropped,
    ) -> Result<(), String> {
        properties.style_ref = self.map_style(properties.style_ref, source, remap)?;
        if let Some(reference) = &mut properties.numbering {
            reference.instance = self.map_numbering(reference.instance, source, remap)?;
        }
        if properties.section_break.take().is_some() {
            dropped.section_breaks += 1;
        }
        if let Some(mark) = &mut properties.mark_run {
            self.remap_run_properties(mark, source, remap)?;
        }
        if let Some(change) = &mut properties.prop_change {
            let prior = &mut change.prior;
            prior.style_ref = self.map_style(prior.style_ref, source, remap)?;
            if let Some(reference) = &mut prior.numbering {
                reference.instance = self.map_numbering(reference.instance, source, remap)?;
            }
            prior.section_break = None;
            if let Some(mark) = &mut prior.mark_run {
                self.remap_run_properties(mark, source, remap)?;
            }
        }
        Ok(())
    }

    fn remap_run_properties(
        &mut self,
        properties: &mut RunProperties,
        source: &WasmDocument,
        remap: &mut Remap,
    ) -> Result<(), String> {
        properties.style_ref = self.map_style(properties.style_ref, source, remap)?;
        if let Some(change) = &mut properties.prop_change {
            change.prior.style_ref = self.map_style(change.prior.style_ref, source, remap)?;
        }
        Ok(())
    }

    fn remap_inlines(
        &mut self,
        inlines: &mut Vec<InlineNode>,
        source: &WasmDocument,
        remap: &mut Remap,
        dropped: &mut Dropped,
    ) -> Result<(), String> {
        let before = inlines.len();
        inlines.retain(|inline| !matches!(inline, InlineNode::EmbeddedObject(_)));
        dropped.objects += u32::try_from(before - inlines.len()).unwrap_or(u32::MAX);
        for inline in inlines.iter_mut() {
            match inline {
                InlineNode::Run(run) => {
                    let mut properties = (*run.properties).clone();
                    self.remap_run_properties(&mut properties, source, remap)?;
                    run.properties = properties.into();
                    continue;
                }
                InlineNode::Drawing(drawing) => {
                    drawing.media = self.map_media(drawing.media, source, remap)?;
                    continue;
                }
                InlineNode::AnchoredDrawing(drawing) => {
                    drawing.media = self.map_media(drawing.media, source, remap)?;
                    continue;
                }
                _ => {}
            }
            match inline_descent_mut(inline) {
                InlineDescentMut::Inlines(children) => {
                    self.remap_inlines(children, source, remap, dropped)?;
                }
                InlineDescentMut::Blocks(blocks) => {
                    for block in blocks.iter_mut() {
                        self.remap_block(block, source, remap, dropped)?;
                    }
                }
                InlineDescentMut::Group(children) => {
                    self.remap_group(children, source, remap, dropped)?;
                }
                InlineDescentMut::Leaf => {}
            }
        }
        Ok(())
    }

    fn remap_group(
        &mut self,
        children: &mut [GroupChild],
        source: &WasmDocument,
        remap: &mut Remap,
        dropped: &mut Dropped,
    ) -> Result<(), String> {
        for child in children {
            match child {
                GroupChild::Picture(picture) => {
                    picture.media = self.map_media(picture.media, source, remap)?;
                }
                GroupChild::TextBox(text_box) => {
                    for block in &mut text_box.blocks {
                        self.remap_block(block, source, remap, dropped)?;
                    }
                }
                GroupChild::Shape(_) => {}
                GroupChild::Group(group) => {
                    self.remap_group(&mut group.children, source, remap, dropped)?;
                }
            }
        }
        Ok(())
    }

    /// The destination style for a source style: the destination's own of the
    /// same kind and name, else a copy installed with a fresh id (its `basedOn`
    /// chain first). `None` stays `None`; a source id that names nothing (it
    /// cannot, in a validated document) becomes `None`.
    fn map_style(
        &mut self,
        style: Option<StyleId>,
        source: &WasmDocument,
        remap: &mut Remap,
    ) -> Result<Option<StyleId>, String> {
        let Some(style) = style else {
            return Ok(None);
        };
        if let Some(mapped) = remap.styles.get(&style) {
            return Ok(*mapped);
        }
        let Some(definition) = source.document.definitions().styles.get(&style) else {
            remap.styles.insert(style, None);
            return Ok(None);
        };
        if let Some(name) = definition.name.as_deref()
            && let Some((existing, _)) = self.document.definitions().styles.iter().find(|(_, s)| {
                s.kind == definition.kind
                    && s.name
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
        {
            remap.styles.insert(style, Some(*existing));
            return Ok(Some(*existing));
        }
        // A cycle in `basedOn` cannot validate, but the walk must not loop on
        // one: an id already being installed resolves to nothing.
        if !remap.in_progress.insert(style) {
            return Ok(None);
        }
        let mut copy = definition.clone();
        copy.is_default = false;
        copy.based_on = self.map_style(definition.based_on, source, remap)?;
        copy.next = None;
        copy.link = None;
        if let Some(paragraph) = &mut copy.paragraph {
            paragraph.style_ref = None;
            paragraph.section_break = None;
            paragraph.prop_change = None;
            paragraph.mark_run = None;
            if let Some(reference) = &mut paragraph.numbering {
                reference.instance = self.map_numbering(reference.instance, source, remap)?;
            }
        }
        if let Some(run) = &mut copy.run {
            run.style_ref = None;
            run.prop_change = None;
        }
        if let Some(table) = &mut copy.table {
            table.style_ref = None;
            table.prop_change = None;
        }
        let id = StyleId::new(
            self.edit_ids
                .next_id()
                .map_err(|_| "id space exhausted".to_owned())?,
        );
        remap.style_ops.push(Operation::SetStyleDefinition {
            id,
            style: Some(Box::new(copy)),
        });
        remap.in_progress.remove(&style);
        remap.styles.insert(style, Some(id));
        Ok(Some(id))
    }

    /// A fresh abstract definition and instance for a source list, so the
    /// inserted list counts on its own. Level style links are cleared: they
    /// name source styles, and the level's own formatting is what the list
    /// paints with.
    fn map_numbering(
        &mut self,
        instance: NumberingInstanceId,
        source: &WasmDocument,
        remap: &mut Remap,
    ) -> Result<NumberingInstanceId, String> {
        use casual_doc_model::v1::AbstractNumberingId;
        if let Some(mapped) = remap.numbering.get(&instance) {
            return Ok(*mapped);
        }
        let definitions = source.document.definitions();
        let Some(source_instance) = definitions.numbering.get(&instance) else {
            return Err("the inserted file names a list it does not define".to_owned());
        };
        let Some(source_abstract) = definitions
            .abstract_numbering
            .get(&source_instance.abstract_ref)
        else {
            return Err("the inserted file names a list it does not define".to_owned());
        };
        let mut abstract_copy = source_abstract.clone();
        abstract_copy.num_style_link = None;
        abstract_copy.style_link = None;
        for level in &mut abstract_copy.levels {
            level.style_ref = None;
            level.pstyle = None;
            level.picture_bullet = None;
            if let Some(paragraph) = &mut level.paragraph_properties {
                paragraph.style_ref = None;
                paragraph.numbering = None;
                paragraph.section_break = None;
                paragraph.prop_change = None;
            }
            if let Some(run) = &mut level.run_properties {
                run.style_ref = None;
                run.prop_change = None;
            }
        }
        let mut instance_copy = source_instance.clone();
        let mint = |ids: &mut casual_doc_model::IdGenerator| {
            ids.next_id().map_err(|_| "id space exhausted".to_owned())
        };
        let abstract_id = AbstractNumberingId::new(mint(&mut self.edit_ids)?);
        let instance_id = NumberingInstanceId::new(mint(&mut self.edit_ids)?);
        instance_copy.abstract_ref = abstract_id;
        for over in &mut instance_copy.overrides {
            if let Some(level) = &mut over.definition {
                level.style_ref = None;
                level.pstyle = None;
                level.picture_bullet = None;
                if let Some(paragraph) = &mut level.paragraph_properties {
                    paragraph.style_ref = None;
                    paragraph.numbering = None;
                    paragraph.section_break = None;
                    paragraph.prop_change = None;
                }
                if let Some(run) = &mut level.run_properties {
                    run.style_ref = None;
                    run.prop_change = None;
                }
            }
        }
        remap.numbering_ops.push(Operation::SetAbstractNumbering {
            id: abstract_id,
            definition: Some(Box::new(abstract_copy)),
        });
        remap.numbering_ops.push(Operation::SetNumberingInstance {
            id: instance_id,
            instance: Some(Box::new(instance_copy)),
        });
        remap.numbering.insert(instance, instance_id);
        Ok(instance_id)
    }

    /// A fresh media id and editor-owned part for a source picture, with its
    /// bytes copied into this session's resources.
    fn map_media(
        &mut self,
        media: MediaId,
        source: &WasmDocument,
        remap: &mut Remap,
    ) -> Result<MediaId, String> {
        if let Some(mapped) = remap.media.get(&media) {
            return Ok(*mapped);
        }
        let reference = source
            .document
            .definitions()
            .media
            .get(&media)
            .ok_or_else(|| "the inserted file names a picture it does not carry".to_owned())?;
        let bytes = source
            .resources
            .get(&reference.part_name)
            .ok_or_else(|| "the inserted file names a picture it does not carry".to_owned())?
            .to_vec();
        let seq = self
            .edit_ids
            .next_id()
            .map_err(|_| "id space exhausted".to_owned())?;
        let extension = reference
            .part_name
            .rsplit_once('.')
            .map_or("bin", |(_, extension)| extension);
        let part_name = format!("word/media/editor-{seq}.{extension}");
        self.resources.insert(part_name.clone(), bytes);
        let id = MediaId::new(seq);
        remap.media_ops.push(Operation::SetMediaReference {
            id,
            reference: Some(Box::new(MediaReference {
                relationship_id: format!("rIdEditorImg{seq}"),
                media_type: reference.media_type.clone(),
                part_name,
            })),
        });
        remap.media.insert(media, id);
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use casual_doc_export::write_document;
    use casual_doc_model::v1::{BlockNode, Document, InlineNode};

    use crate::open_document;

    const TABLE_LIST: &[u8] =
        include_bytes!("../../../fixtures/corpus/real-producer-table-list.docx");
    const TABLE_LIST_ODT: &[u8] =
        include_bytes!("../../../fixtures/corpus/real-producer-table-list.odt");
    const PICTURE: &[u8] = include_bytes!("../../../fixtures/generated/picture-hyperlink.docx");

    fn first_node(document: &crate::WasmDocument) -> String {
        let pairs: Vec<(String, String)> =
            serde_json::from_str(&document.searchable_text_inner()).expect("pairs");
        pairs[0].0.clone()
    }

    fn texts(document: &crate::WasmDocument) -> Vec<String> {
        let pairs: Vec<(String, String)> =
            serde_json::from_str(&document.searchable_text_inner()).expect("pairs");
        pairs.into_iter().map(|(_, text)| text).collect()
    }

    fn numbered(document: &Document) -> usize {
        document
            .body()
            .iter()
            .filter(|block| {
                matches!(block, BlockNode::Paragraph(p) if p.properties.numbering.is_some())
            })
            .count()
    }

    fn tables(document: &Document) -> usize {
        document
            .body()
            .iter()
            .filter(|block| matches!(block, BlockNode::Table(_)))
            .count()
    }

    fn drawings(document: &Document) -> usize {
        let mut count = 0;
        casual_doc_model::v1::visit_paragraphs(document.body(), &mut |paragraph| {
            count += paragraph
                .inlines
                .iter()
                .filter(|inline| matches!(inline, InlineNode::Drawing(_)))
                .count();
        });
        count
    }

    #[test]
    fn a_text_file_lands_at_the_caret_splitting_the_paragraph() {
        let mut document = open_document(b"before after").expect("opens");
        let node = first_node(&document);
        document
            .insert_document_file_inner(&node, 7, b"one\ntwo", "")
            .expect("inserts");
        assert_eq!(texts(&document), ["before ", "one", "two", "after"]);
        document.undo_inner().expect("undoes");
        assert_eq!(
            texts(&document),
            ["before after"],
            "one Ctrl+Z removes the insert"
        );
    }

    #[test]
    fn a_docx_brings_its_tables_lists_and_styles_and_the_result_saves() {
        let source = open_document(TABLE_LIST).expect("fixture opens");
        assert!(
            numbered(&source.document) > 0,
            "the fixture exercises lists"
        );
        assert!(tables(&source.document) > 0, "the fixture exercises tables");
        let mut document = open_document(b"start").expect("opens");
        let node = first_node(&document);
        document
            .insert_document_file_inner(&node, 5, TABLE_LIST, "")
            .expect("inserts");
        assert_eq!(numbered(&document.document), numbered(&source.document));
        assert_eq!(tables(&document.document), tables(&source.document));
        // Every style the inserted body names resolves to a style of the same
        // name here: copied in, or matched.
        for block in source.document.body() {
            let BlockNode::Paragraph(paragraph) = block else {
                continue;
            };
            let Some(style) = paragraph.properties.style_ref else {
                continue;
            };
            let name = source
                .document
                .definitions()
                .styles
                .get(&style)
                .and_then(|s| s.name.clone());
            assert!(
                document
                    .document
                    .definitions()
                    .styles
                    .iter()
                    .any(|(_, candidate)| candidate.name == name),
                "the style {name:?} arrives with the paragraph"
            );
        }
        document
            .document
            .validate()
            .expect("the merged document validates");
        write_document(&document.document, &BTreeMap::new()).expect("the result saves as DOCX");
    }

    #[test]
    fn an_odt_inserts_through_the_same_path() {
        let source = open_document(TABLE_LIST_ODT).expect("fixture opens");
        let mut document = open_document(b"start").expect("opens");
        let node = first_node(&document);
        document
            .insert_document_file_inner(&node, 0, TABLE_LIST_ODT, "")
            .expect("inserts");
        assert_eq!(tables(&document.document), tables(&source.document));
        document.document.validate().expect("validates");
    }

    #[test]
    fn a_picture_arrives_with_its_bytes() {
        let source = open_document(PICTURE).expect("fixture opens");
        let expected = drawings(&source.document);
        assert!(expected > 0, "the fixture exercises pictures");
        let mut document = open_document(b"start").expect("opens");
        let node = first_node(&document);
        document
            .insert_document_file_inner(&node, 5, PICTURE, "")
            .expect("inserts");
        assert_eq!(drawings(&document.document), expected);
        for (_, reference) in document.document.definitions().media.iter() {
            assert!(
                document.resources.get(&reference.part_name).is_some(),
                "{} has its bytes",
                reference.part_name
            );
        }
    }

    #[test]
    fn a_caret_outside_the_body_is_refused_with_a_reason() {
        let mut document = open_document(b"x").expect("opens");
        let error = document
            .insert_document_file_inner("not-a-node", 0, b"y", "")
            .expect_err("refused");
        assert!(!error.is_empty());
    }
}
