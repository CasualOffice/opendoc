//! Footnotes and endnotes: the options dialog, going to the next or previous
//! note, and converting one kind to the other (`docs/153` `page.note.settings`,
//! `page.note.goto`, `page.note.convert`; `docs/109` RM-24).
//!
//! # The options
//!
//! Word's Footnote and Endnote dialog edits `w:footnotePr`/`w:endnotePr`, which
//! live in two places: each section's `w:sectPr`, and the document's defaults in
//! `w:settings`. Layout already reads both, section first
//! (`casual-doc-layout` `note_numbering.rs`), so an imported document's numbering
//! was always drawn right and could not be changed. [`note_settings`] answers what
//! the caret's section RESOLVES to — the same layering — and
//! [`set_note_settings`] writes the dialog's choice to that section or, for
//! "Whole document", to every section and the default, as one undoable step
//! through `Operation::SetNoteProperties`.
//!
//! # Going to a note
//!
//! [`adjacent_note`] walks the body's note references in document order — inside
//! tables, content controls and hyperlinks too — and answers the first one after
//! (or before) the caret, the way Word's References ▸ Next Footnote does.
//!
//! # Converting
//!
//! [`convert_notes`] is `casual_doc_command::notes::convert_notes`: each note
//! removed and re-inserted as the other kind at the same place with the same
//! content, all of them one step.
//!
//! [`note_settings`]: WasmDocument::note_settings
//! [`set_note_settings`]: WasmDocument::set_note_settings
//! [`adjacent_note`]: WasmDocument::adjacent_note
//! [`convert_notes`]: WasmDocument::convert_notes

use std::str::FromStr;

use casual_doc_command::notes::{self as lowering, NoteConversion, NoteScope};
use casual_doc_edit::{Pos, inline_text_len};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, InlineNode, NoteKind, NoteNumberRestart, NotePosition, NoteProperties, NumberFormat,
    ReviewProjection,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::{EditResult, HistoryKind, WasmDocument, body_section_of, to_js};

/// What the dialog opens on: the options the caret's section resolves to, and
/// whether that section states its own or inherits the document's.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NoteSettingsJson {
    position: NotePosition,
    number_format: NumberFormat,
    start: i32,
    restart: NoteNumberRestart,
    /// How many sections the document has, so the dialog can say whether
    /// "This section" and "Whole document" differ.
    sections: usize,
}

/// One note reference, for navigation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NoteAt {
    kind: &'static str,
    node: String,
    offset: u32,
}

fn kind_of(token: &str) -> Result<NoteKind, String> {
    match token {
        "footnote" => Ok(NoteKind::Footnote),
        "endnote" => Ok(NoteKind::Endnote),
        _ => Err(format!("\u{201c}{token}\u{201d} isn't a kind of note.")),
    }
}

const fn kind_token(kind: NoteKind) -> &'static str {
    match kind {
        NoteKind::Footnote => "footnote",
        NoteKind::Endnote => "endnote",
    }
}

/// Whether `position` is one `kind` can take (`ST_FtnPos` / `ST_EdnPos`), the
/// filter layout applies, so the dialog never shows a value layout ignores.
const fn position_applies(kind: NoteKind, position: NotePosition) -> bool {
    matches!(
        (kind, position),
        (
            NoteKind::Footnote,
            NotePosition::PageBottom | NotePosition::BeneathText
        ) | (
            NoteKind::Endnote,
            NotePosition::SectionEnd | NotePosition::DocumentEnd
        )
    )
}

/// The options `section` (or the defaults alone) resolves to for `kind`: each
/// field from the section if it states it, else from `w:settings`, else Word's
/// default — `note_numbering.rs`'s layering. O(1).
fn resolve(
    own: Option<&NoteProperties>,
    defaults: &NoteProperties,
    kind: NoteKind,
) -> NoteSettingsJson {
    let layers = || own.into_iter().chain(std::iter::once(defaults));
    NoteSettingsJson {
        position: layers()
            .find_map(|props| {
                props
                    .position
                    .filter(|position| position_applies(kind, *position))
            })
            .unwrap_or(match kind {
                NoteKind::Footnote => NotePosition::PageBottom,
                NoteKind::Endnote => NotePosition::DocumentEnd,
            }),
        number_format: layers()
            .find_map(|props| props.number_format.clone())
            .unwrap_or(NumberFormat::Decimal),
        start: layers().find_map(|props| props.number_start).unwrap_or(1),
        restart: layers()
            .find_map(|props| props.number_restart)
            .unwrap_or(NoteNumberRestart::Continuous),
        sections: 0,
    }
}

/// Every note reference in `blocks`, in document order, with its paragraph and
/// offset — through tables, content controls and the wrappers whose text counts
/// (a hyperlink, an insertion). O(blocks).
fn references(blocks: &[BlockNode], out: &mut Vec<(NoteKind, NodeId, u32)>) {
    fn inlines(
        items: &[InlineNode],
        paragraph: NodeId,
        offset: &mut u32,
        out: &mut Vec<(NoteKind, NodeId, u32)>,
    ) {
        for inline in items {
            match inline {
                InlineNode::NoteReference(reference) => {
                    out.push((reference.kind, paragraph, *offset))
                }
                InlineNode::Hyperlink(link) => inlines(&link.inlines, paragraph, offset, out),
                InlineNode::Sdt(sdt) => inlines(&sdt.inlines, paragraph, offset, out),
                InlineNode::Revision(revision)
                    if revision
                        .kind
                        .contributes_to(ReviewProjection::FinalWithMarkup) =>
                {
                    inlines(&revision.inlines, paragraph, offset, out);
                }
                other => *offset += inline_text_len(other),
            }
        }
    }
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                inlines(&paragraph.inlines, paragraph.id, &mut 0, out)
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        references(&cell.blocks, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => references(&sdt.blocks, out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

/// The body paragraphs in document order. O(blocks).
fn paragraphs(blocks: &[BlockNode], out: &mut Vec<NodeId>) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => out.push(paragraph.id),
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        paragraphs(&cell.blocks, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => paragraphs(&sdt.blocks, out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

fn parse_node(node: &str) -> Result<NodeId, String> {
    NodeId::from_str(node).map_err(|_| "invalid node id".to_owned())
}

#[wasm_bindgen]
impl WasmDocument {
    /// The footnote (`kind` `"footnote"`) or endnote (`"endnote"`) options the
    /// section holding paragraph `node` resolves to, as JSON:
    /// `{position, numberFormat, start, restart, sections}`, with the model's
    /// own tokens (`pageBottom`, `lowerRoman`, `eachSection`, …). O(blocks) to
    /// find the section.
    ///
    /// # Errors
    ///
    /// An unknown `kind`.
    #[wasm_bindgen(js_name = noteSettings)]
    pub fn note_settings(&self, kind: &str, node: &str) -> Result<String, JsValue> {
        self.note_settings_inner(kind, node).map_err(to_js)
    }

    /// Word's Footnote and Endnote dialog, Apply: `properties_json` (the model's
    /// `NoteProperties`: `position`, `numberFormat`, `numberStart`,
    /// `numberRestart`) for `kind`, written to the section holding `node`
    /// (`scope` `"section"`) or to every section and the document default
    /// (`"document"`), as one undoable step.
    ///
    /// # Errors
    ///
    /// An unknown kind or scope, malformed options, or a value the model refuses.
    #[wasm_bindgen(js_name = setNoteSettings)]
    pub fn set_note_settings(
        &mut self,
        kind: &str,
        node: &str,
        scope: &str,
        properties_json: &str,
    ) -> Result<EditResult, JsValue> {
        self.set_note_settings_inner(kind, node, scope, properties_json)
            .map_err(to_js)
    }

    /// Every footnote and endnote reference in the body, in document order, as
    /// JSON `[{kind, node, offset}]`. O(blocks).
    #[wasm_bindgen(js_name = noteReferences)]
    #[must_use]
    pub fn note_references(&self) -> String {
        let mut found = Vec::new();
        references(self.document.body(), &mut found);
        let rows: Vec<NoteAt> = found
            .into_iter()
            .map(|(kind, node, offset)| NoteAt {
                kind: kind_token(kind),
                node: node.to_string(),
                offset,
            })
            .collect();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_owned())
    }

    /// Word's Next / Previous Footnote (or Endnote): the first reference of
    /// `kind` after (`forward`) or before the caret at `node`/`offset`, as JSON
    /// `{node, offset}`, or `null` when there is none that way. A caret outside
    /// the body (in a header, or inside a note) starts from the document's top
    /// going forward and its end going back. O(blocks).
    ///
    /// # Errors
    ///
    /// An unknown `kind`.
    #[wasm_bindgen(js_name = adjacentNote)]
    pub fn adjacent_note(
        &self,
        kind: &str,
        node: &str,
        offset: u32,
        forward: bool,
    ) -> Result<String, JsValue> {
        self.adjacent_note_inner(kind, node, offset, forward)
            .map_err(to_js)
    }

    /// Word's Convert Notes: `conversion` is `footnotesToEndnotes`,
    /// `endnotesToFootnotes` or `swap`; with `start_node`/`end_node` only the
    /// notes inside that selection convert. One undoable step.
    ///
    /// # Errors
    ///
    /// An unknown conversion, nothing to convert, or the choke point refusing.
    #[wasm_bindgen(js_name = convertNotes)]
    pub fn convert_notes(
        &mut self,
        conversion: &str,
        start_node: Option<String>,
        start_offset: u32,
        end_node: Option<String>,
        end_offset: u32,
    ) -> Result<EditResult, JsValue> {
        self.convert_notes_inner(
            conversion,
            start_node.as_deref(),
            start_offset,
            end_node.as_deref(),
            end_offset,
        )
        .map_err(to_js)
    }
}

impl WasmDocument {
    pub(crate) fn note_settings_inner(&self, kind: &str, node: &str) -> Result<String, String> {
        let kind = kind_of(kind)?;
        let definitions = self.document.definitions();
        let section = NodeId::from_str(node)
            .ok()
            .and_then(|paragraph| body_section_of(&self.document, paragraph))
            .and_then(|id| definitions.sections.iter().find(|section| section.id == id))
            .or_else(|| definitions.sections.last());
        let own = section.map(|section| match kind {
            NoteKind::Footnote => &section.footnote_props,
            NoteKind::Endnote => &section.endnote_props,
        });
        let defaults = match kind {
            NoteKind::Footnote => &definitions.settings.footnote_props,
            NoteKind::Endnote => &definitions.settings.endnote_props,
        };
        let mut resolved = resolve(own, defaults, kind);
        resolved.sections = definitions.sections.len();
        serde_json::to_string(&resolved).map_err(|error| error.to_string())
    }

    pub(crate) fn set_note_settings_inner(
        &mut self,
        kind: &str,
        node: &str,
        scope: &str,
        properties_json: &str,
    ) -> Result<EditResult, String> {
        let kind = kind_of(kind)?;
        let properties: NoteProperties = serde_json::from_str(properties_json)
            .map_err(|error| format!("note options: {error}"))?;
        if let Some(position) = properties.position
            && !position_applies(kind, position)
        {
            return Err(format!("A {} can't be placed there.", kind_token(kind)));
        }
        let scope = match scope {
            "document" => NoteScope::Document,
            "section" => {
                let paragraph = parse_node(node)?;
                let section = body_section_of(&self.document, paragraph).or_else(|| {
                    self.document
                        .definitions()
                        .sections
                        .last()
                        .map(|section| section.id)
                });
                match section {
                    Some(section) => NoteScope::Section(section),
                    // A document with no section entries has only the defaults.
                    None => NoteScope::Document,
                }
            }
            other => return Err(format!("\u{201c}{other}\u{201d} isn't a scope.")),
        };
        let ops = lowering::set_note_properties(&self.document, scope, kind, &properties);
        self.apply_action_as(ops, HistoryKind::PageSetup)
    }

    pub(crate) fn adjacent_note_inner(
        &self,
        kind: &str,
        node: &str,
        offset: u32,
        forward: bool,
    ) -> Result<String, String> {
        let kind = kind_of(kind)?;
        let mut order = Vec::new();
        paragraphs(self.document.body(), &mut order);
        let index = |paragraph: NodeId| order.iter().position(|candidate| *candidate == paragraph);
        let caret = NodeId::from_str(node)
            .ok()
            .and_then(index)
            .map(|at| (at, offset));
        let mut found = Vec::new();
        references(self.document.body(), &mut found);
        let candidates = found
            .into_iter()
            .filter(|(candidate, _, _)| *candidate == kind)
            .filter_map(|(_, paragraph, at)| {
                index(paragraph).map(|order| ((order, at), paragraph, at))
            });
        let hit = if forward {
            candidates
                .filter(|(key, _, _)| caret.is_none_or(|caret| *key > caret))
                .min_by_key(|(key, _, _)| *key)
        } else {
            candidates
                .filter(|(key, _, _)| caret.is_none_or(|caret| *key < caret))
                .max_by_key(|(key, _, _)| *key)
        };
        Ok(match hit {
            Some((_, paragraph, at)) => format!("{{\"node\":\"{paragraph}\",\"offset\":{at}}}"),
            None => "null".to_owned(),
        })
    }

    pub(crate) fn convert_notes_inner(
        &mut self,
        conversion: &str,
        start_node: Option<&str>,
        start_offset: u32,
        end_node: Option<&str>,
        end_offset: u32,
    ) -> Result<EditResult, String> {
        let conversion = match conversion {
            "footnotesToEndnotes" => NoteConversion::FootnotesToEndnotes,
            "endnotesToFootnotes" => NoteConversion::EndnotesToFootnotes,
            "swap" => NoteConversion::Swap,
            other => return Err(format!("\u{201c}{other}\u{201d} isn't a note conversion.")),
        };
        let range = match (start_node, end_node) {
            (Some(start), Some(end)) => Some((
                (parse_node(start)?, start_offset),
                (parse_node(end)?, end_offset),
            )),
            _ => None,
        };
        let ids = &mut self.edit_ids;
        let edit = lowering::convert_notes(&self.document, conversion, range, &mut || {
            ids.next_id().ok()
        })?;
        if edit.operations.is_empty() {
            return Err(casual_doc_edit::refusal::marked(
                "notes.nothing-to-convert",
                "There are no notes of that kind to convert here.",
            ));
        }
        // The caret stays where the reader is: the first converted note's place.
        let caret = edit
            .operations
            .iter()
            .find_map(|op| match op {
                casual_doc_edit::Operation::InsertNote { at, .. } => Some(*at),
                _ => None,
            })
            .unwrap_or_else(|| Pos::new(self.document.id(), 0));
        self.apply_action_caret_as(edit.operations, caret, HistoryKind::NoteChange)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use casual_doc_export::write_document;
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::{
        BlockNode, Definitions, Document, InlineNode, Note, NoteId, NoteKind, NoteReference,
        Paragraph, ParagraphProperties, Run, RunProperties,
    };

    use crate::open_document;

    fn id(n: u64) -> NodeId {
        NodeId::from_parts(n, 178).unwrap()
    }

    fn run(n: u64, text: &str) -> InlineNode {
        InlineNode::Run(Run {
            id: id(n),
            properties: RunProperties::default().into(),
            text: text.to_owned(),
        })
    }

    fn note(n: u64, text: &str) -> Note {
        Note {
            blocks: vec![BlockNode::Paragraph(Paragraph {
                id: id(n),
                properties: ParagraphProperties::default().into(),
                inlines: vec![run(n + 1, text)],
            })],
        }
    }

    /// "Alpha[f] Beta[e] Gamma[f]", saved and reopened, so the test runs on an
    /// IMPORTED document as the dialog must.
    fn imported() -> crate::WasmDocument {
        imported_with(false)
    }

    /// [`imported`], optionally with one body section, so "this section" has
    /// something to name (a document with no page setup has no section).
    fn imported_with(section: bool) -> crate::WasmDocument {
        let mut definitions = Definitions::default();
        if section {
            definitions.sections.push(crate::tests::sections_boundary(
                970,
                (12_240, 15_840),
                1_440,
                Vec::new(),
                Vec::new(),
                false,
            ));
        }
        definitions
            .footnotes
            .insert(NoteId::new(id(50)), note(500, "first"));
        definitions
            .footnotes
            .insert(NoteId::new(id(51)), note(510, "third"));
        definitions
            .endnotes
            .insert(NoteId::new(id(60)), note(600, "second"));
        let reference = |n: u64, kind, note: u64| {
            InlineNode::NoteReference(NoteReference {
                id: id(n),
                kind,
                note: NoteId::new(id(note)),
            })
        };
        let document = Document::new(
            id(1),
            vec![BlockNode::Paragraph(Paragraph {
                id: id(10),
                properties: ParagraphProperties::default().into(),
                inlines: vec![
                    run(11, "Alpha"),
                    reference(12, NoteKind::Footnote, 50),
                    run(13, " Beta"),
                    reference(14, NoteKind::Endnote, 60),
                    run(15, " Gamma"),
                    reference(16, NoteKind::Footnote, 51),
                ],
            })],
            definitions,
        )
        .expect("valid");
        open_document(&write_document(&document, &BTreeMap::new()).expect("writes"))
            .expect("reopens")
    }

    fn first_paragraph(document: &crate::WasmDocument) -> String {
        match &document.document.body()[0] {
            BlockNode::Paragraph(paragraph) => paragraph.id.to_string(),
            _ => panic!("a paragraph"),
        }
    }

    fn kinds(document: &crate::WasmDocument) -> Vec<String> {
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&document.note_references()).expect("json");
        rows.iter()
            .map(|row| row["kind"].as_str().unwrap().to_owned())
            .collect()
    }

    /// The dialog's write reaches an imported document, reads back through the
    /// same layering, leaves the document default alone when it was asked for
    /// one section, and is one undo step. Mutation: make `set_note_properties`'s
    /// Section arm write the settings default (`section: None`) -> the
    /// "default untouched" assertion goes red.
    #[test]
    fn note_options_apply_to_an_imported_document_as_one_step() {
        // With a section: a section-scoped write to a document with none falls
        // back to the default by design, so the fixture must carry one.
        let mut document = imported_with(true);
        let node = first_paragraph(&document);
        let before: serde_json::Value =
            serde_json::from_str(&document.note_settings_inner("footnote", &node).unwrap())
                .unwrap();
        assert_eq!(before["numberFormat"], "decimal");
        assert_eq!(before["position"], "pageBottom");
        document
            .set_note_settings_inner(
                "footnote",
                &node,
                "section",
                r#"{"numberFormat":"lowerRoman","numberStart":3,"numberRestart":"eachPage"}"#,
            )
            .expect("applies");
        let after: serde_json::Value =
            serde_json::from_str(&document.note_settings_inner("footnote", &node).unwrap())
                .unwrap();
        assert_eq!(after["numberFormat"], "lowerRoman");
        assert_eq!(after["start"], 3);
        assert_eq!(after["restart"], "eachPage");
        assert_eq!(
            document
                .document
                .definitions()
                .settings
                .footnote_props
                .number_format,
            None,
            "This section leaves the document default untouched"
        );
        assert!(
            document
                .set_note_settings_inner(
                    "footnote",
                    &node,
                    "section",
                    r#"{"position":"documentEnd"}"#
                )
                .is_err(),
            "a footnote cannot go at the end of the document"
        );
        document.undo_inner().expect("undo");
        let undone: serde_json::Value =
            serde_json::from_str(&document.note_settings_inner("footnote", &node).unwrap())
                .unwrap();
        assert_eq!(undone["numberFormat"], "decimal", "one step undoes it");
    }

    /// Next and previous walk the references of one kind in document order and
    /// stop at the ends. Mutation: `>=` for `>` in the forward filter -> the
    /// caret sitting at a footnote finds that same footnote again.
    #[test]
    fn next_and_previous_footnote_walk_in_document_order() {
        let document = imported();
        let node = first_paragraph(&document);
        let at = |json: String| -> Option<u32> {
            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            value
                .get("offset")
                .and_then(serde_json::Value::as_u64)
                .map(|n| n as u32)
        };
        // "Alpha" is 5 and a reference is zero-width, so the footnotes sit at 5
        // and 16 ("Alpha Beta Gamma"), the endnote at 10.
        assert_eq!(
            at(document
                .adjacent_note_inner("footnote", &node, 0, true)
                .unwrap()),
            Some(5)
        );
        assert_eq!(
            at(document
                .adjacent_note_inner("footnote", &node, 5, true)
                .unwrap()),
            Some(16)
        );
        assert_eq!(
            at(document
                .adjacent_note_inner("footnote", &node, 16, true)
                .unwrap()),
            None
        );
        assert_eq!(
            at(document
                .adjacent_note_inner("footnote", &node, 16, false)
                .unwrap()),
            Some(5)
        );
        assert_eq!(
            at(document
                .adjacent_note_inner("endnote", &node, 0, true)
                .unwrap()),
            Some(10)
        );
    }

    /// Converting footnotes to endnotes leaves three endnotes with the bodies
    /// they had, is ONE undo step, and a second conversion with none left is
    /// refused by name. Mutation: return the lowered operations without the
    /// `RemoveNote`s -> the footnotes survive and the kinds assertion goes red.
    #[test]
    fn footnotes_convert_to_endnotes_as_one_step() {
        let mut document = imported();
        assert_eq!(kinds(&document), ["footnote", "endnote", "footnote"]);
        document
            .convert_notes_inner("footnotesToEndnotes", None, 0, None, 0)
            .expect("converts");
        assert_eq!(kinds(&document), ["endnote", "endnote", "endnote"]);
        let refusal = document
            .convert_notes_inner("footnotesToEndnotes", None, 0, None, 0)
            .expect_err("nothing left");
        assert!(refusal.contains("notes.nothing-to-convert"), "{refusal}");
        document.undo_inner().expect("undo");
        assert_eq!(
            kinds(&document),
            ["footnote", "endnote", "footnote"],
            "one step undoes all"
        );
    }
}
