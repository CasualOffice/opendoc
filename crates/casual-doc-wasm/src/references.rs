//! Captions and cross-references, host side (`docs/105` OO-005).
//!
//! `casual-doc-edit`'s `references` module writes the OOXML field markup and reads
//! no document. `casual_doc_command::references` reads the document — **once per
//! command** — and lowers a request to operations; agents call the same lowering
//! (`docs/181` §12, lane references). This module is the editor's glue: it parses
//! the host's arguments, lends the command what the editor's layout knows
//! ([`EditorFacts`]: the heading cascade, and for a page reference the page the
//! target is painted on), and applies the result as one undoable action.
//!
//! # The performance rule the lowering obeys
//!
//! The feature that most wants to enumerate a whole document is the one that
//! lists every caption in it. The defect this repository already paid for
//! (`docs/116`) was `documentOutline` calling `paragraph_properties` per node —
//! a by-id lookup that is itself a whole-document walk, so 1.3M paragraphs cost
//! 1.3M walks. The shape is always one walk carrying what it needs.
//! `casual_doc_edit::document_scans` counts the walks, and
//! `references_read_the_document_a_bounded_number_of_times` in the test module
//! asserts that count does not grow with the document — which a timing
//! threshold cannot do.
//!
//! Typing stays O(1) because nothing here is on the keystroke path: a caption's
//! number is resolved when the caption is inserted, cached in the model as the
//! field's result, and repainted from the model thereafter. A document full of
//! captions costs a keystroke exactly what a document full of text costs.

use std::collections::HashMap;

use casual_doc_command::references::{self, ReferenceScan, ScanRequest, TargetRecord};
use casual_doc_edit::Pos;
use casual_doc_edit::references::{
    CaptionChapter, CaptionNumberFormat, CaptionPosition, CaptionSeparator, CaptionSpec,
    ReferenceTo,
};
use wasm_bindgen::prelude::*;

use crate::toc::EditorFacts;
use crate::{EditResult, HistoryKind, WasmDocument, node_id, to_js};

#[wasm_bindgen]
impl WasmDocument {
    /// Every caption label the document uses, unioned with Word's built-ins
    /// (`Figure`, `Table`, `Equation`), sorted — the Insert Caption dialog's label
    /// list. A label is a `SEQ` sequence name, so a document another producer
    /// wrote teaches us its labels without any configuration.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = captionLabels)]
    #[must_use]
    pub fn caption_labels(&self) -> Vec<String> {
        self.scan_references(ScanRequest::default())
            .caption_labels()
    }

    /// Every caption in the document as `"{node}\t{label}\t{number}\t{text}"`
    /// rows in document order — what the cross-reference picker lists and what a
    /// table of figures will enumerate.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = captionEntries)]
    #[must_use]
    pub fn caption_entries(&self) -> Vec<String> {
        self.scan_references(ScanRequest::default())
            .captions
            .iter()
            .map(|caption| {
                format!(
                    "{}\t{}\t{}\t{}",
                    caption.node,
                    caption.label,
                    // The number the document SHOWS, not the one a field update
                    // would compute — a picker lists what the reader sees. The two
                    // agree for every caption this engine wrote, which is what
                    // `a_caption_after_an_existing_one_continues_its_sequence`
                    // asserts.
                    caption.cached_number,
                    caption.text
                )
            })
            .collect()
    }

    /// The cross-reference targets of one reference type, as `"{node}\t{text}"`
    /// rows in document order.
    ///
    /// `kind` is `"heading"`, `"bookmark"`, `"footnote"`, `"endnote"`, or a
    /// caption label (`"Figure"`, `"Table"`, or any label the document uses) —
    /// Word's *Reference type* list. An unknown label is not an error, so a host
    /// offering a label the document has since stopped using gets an empty list
    /// and says so, instead of throwing.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = referenceTargets)]
    #[must_use]
    pub fn reference_targets(&self, kind: &str) -> Vec<String> {
        let scan = self.scan_references(ScanRequest::default());
        let mut rows: Vec<TargetRecord> = match kind {
            "heading" => scan
                .headings
                .iter()
                .enumerate()
                .map(|(order, (_, node, text))| TargetRecord {
                    node: *node,
                    order,
                    text: text.clone(),
                })
                .collect(),
            "bookmark" => references::bookmark_targets(&self.document),
            "footnote" | "endnote" => references::note_targets(&self.document, kind == "footnote"),
            label => scan
                .captions
                .iter()
                .filter(|caption| caption.label == label)
                .map(|caption| TargetRecord {
                    node: caption.node,
                    order: caption.order,
                    text: caption.text.clone(),
                })
                .collect(),
        };
        rows.sort_by_key(|row| row.order);
        rows.iter()
            .map(|row| format!("{}\t{}", row.node, row.text))
            .collect()
    }

    /// Every caption whose `SEQ` field **shows** a number that disagrees with the
    /// number the document would resolve now, as `"{node}\t{shown}\t{correct}"`.
    ///
    /// # Dirty, not silently stale — and why this exists at all
    ///
    /// Word caches a field's result and leaves it stale until the reader presses
    /// F9; nothing in the document says it is stale, and nothing in the UI says so
    /// either. We diverge deliberately: inserting a caption renumbers what follows
    /// in the same undoable action, so this engine never *creates* staleness. But
    /// two things still produce it and neither is ours to prevent:
    ///
    /// 1. a caption paragraph deleted with Backspace, which is ordinary text
    ///    editing and must not trigger a document-wide renumber (that would make a
    ///    keystroke O(document) — see
    ///    `a_keystroke_costs_a_bounded_number_of_document_scans`);
    /// 2. a document another producer wrote, which arrives with whatever its last
    ///    reader cached.
    ///
    /// So the answer is not "always correct" and not "silently wrong": it is
    /// **reportable**. This is the read a host uses to say so, and
    /// [`updateCaptionNumbers`](Self::update_caption_numbers) is how a reader fixes
    /// it. An empty result means every caption shows the right number.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = staleCaptionNumbers)]
    #[must_use]
    pub fn stale_caption_numbers(&self) -> Vec<String> {
        self.scan_references(ScanRequest::default())
            .captions
            .iter()
            .filter_map(|caption| {
                let correct = caption.format.render(caption.number);
                (caption.cached_number != correct)
                    .then(|| format!("{}\t{}\t{}", caption.node, caption.cached_number, correct))
            })
            .collect()
    }

    /// Rewrites every caption's `SEQ` result to the number the document resolves
    /// now — Word's F9 over the caption sequences, as ONE undoable action.
    ///
    /// Refuses with a reason when nothing is stale, rather than pushing an empty
    /// history entry a user would then have to undo twice.
    ///
    /// **O(document)** — one walk, then one operation per caption that actually
    /// changed. A caption already showing the right number produces no operation,
    /// so updating a correct document costs no history.
    #[wasm_bindgen(js_name = updateCaptionNumbers)]
    pub fn update_caption_numbers(&mut self) -> Result<EditResult, JsValue> {
        self.update_caption_numbers_inner().map_err(to_js)
    }

    /// Inserts a caption paragraph above or below the block that holds
    /// `target_node`, and renumbers the captions of the same label that follow it
    /// — Word's Insert Caption, as one undoable action.
    ///
    /// The paragraph is `Caption`-styled; the style is created first when the
    /// document has not got one, which is what Word does and is the difference
    /// between a caption and a line of body text that happens to hold a number.
    ///
    /// `position` is `"above"`/`"below"`, `number_format` and `separator` take the
    /// wire names from `casual_doc_edit::references`, and `chapter_level` is `0`
    /// for "do not include a chapter number" or a 1-based heading level.
    ///
    /// **O(document)**: two walks — one to number and enumerate, one to locate the
    /// target block — plus O(captions after the insertion point) renumbering
    /// operations. Never a lookup by id inside a loop.
    #[wasm_bindgen(js_name = insertCaption)]
    #[allow(clippy::too_many_arguments)]
    pub fn insert_caption(
        &mut self,
        target_node: &str,
        label: &str,
        text: String,
        position: &str,
        exclude_label: bool,
        number_format: &str,
        chapter_level: u8,
        separator: &str,
    ) -> Result<EditResult, JsValue> {
        let target = node_id(target_node)?;
        let position = match position {
            "above" => CaptionPosition::Above,
            "below" => CaptionPosition::Below,
            other => return Err(to_js(format!("unknown caption position {other:?}"))),
        };
        let number_format = CaptionNumberFormat::parse(number_format)
            .ok_or_else(|| to_js(format!("unknown caption number format {number_format:?}")))?;
        let chapter = if chapter_level == 0 {
            None
        } else {
            Some(CaptionChapter {
                heading_level: chapter_level,
                separator: CaptionSeparator::parse(separator)
                    .ok_or_else(|| to_js(format!("unknown caption separator {separator:?}")))?,
            })
        };
        let spec = CaptionSpec {
            label: label.to_owned(),
            text,
            position,
            exclude_label,
            number_format,
            chapter,
        };
        let facts = EditorFacts::with_pages(self.document.definitions(), HashMap::new());
        let ids = &mut self.edit_ids;
        let placed = references::insert_caption(&self.document, &facts, target, spec, &mut || {
            ids.next_id().ok()
        })
        .map_err(to_js)?;
        drop(facts);
        self.apply_action_caret_as(placed.operations, placed.caret, HistoryKind::FieldChange)
            .map_err(to_js)
    }

    /// Inserts a cross-reference field at the caret, pointing at `target_node` —
    /// Word's Cross-reference, as one undoable action.
    ///
    /// The target is bookmarked first when it is not already: Word names its own
    /// automatic bookmarks `_Ref` plus a decimal, and we reuse that prefix and
    /// reuse an existing `_Ref` bookmark on the same target, so a document with
    /// ten references to one figure carries one bookmark rather than ten.
    ///
    /// `reference_to` takes the wire names from `casual_doc_edit::references`
    /// (`"entireCaption"`, `"pageNumber"`, `"aboveBelow"`, …); `hyperlink` writes
    /// the `\h` switch, which is Word's *Insert as hyperlink*.
    ///
    /// `include_above_below` is Word's *Include above/below* checkbox, and it is
    /// **two fields, one action**: the reference, then a space, then a second
    /// ` REF … \p ` naming the same bookmark — which is exactly what Word writes
    /// for it. It is ignored for a reference that already IS the above/below word.
    /// Taking it as an argument rather than leaving the checkbox disabled is
    /// deliberate: a control that cannot run is a dead control (`SKILL.md` §10),
    /// and the alternative was shipping one.
    ///
    /// **O(document)** — one walk for the target's text and document order, plus,
    /// for `"pageNumber"` only, one layout query for the page the target is on.
    /// That query is the single hard coupling to layout in this feature, and it is
    /// why a page reference is the one result that can go stale after a reflow,
    /// exactly as it does in Word.
    #[wasm_bindgen(js_name = insertCrossReference)]
    pub fn insert_cross_reference(
        &mut self,
        caret_node: &str,
        caret_offset: u32,
        target_node: &str,
        reference_to: &str,
        hyperlink: bool,
        include_above_below: bool,
    ) -> Result<EditResult, JsValue> {
        let caret = Pos::new(node_id(caret_node)?, caret_offset);
        let target = node_id(target_node)?;
        let to = ReferenceTo::parse(reference_to)
            .ok_or_else(|| to_js(format!("unknown reference kind {reference_to:?}")))?;
        let operations = self
            .cross_reference_operations(caret, target, to, hyperlink, include_above_below)
            .map_err(to_js)?;
        self.apply_action_caret_as(operations, caret, HistoryKind::FieldChange)
            .map_err(to_js)
    }
}

impl WasmDocument {
    /// See [`WasmDocument::update_caption_numbers`]. Split out so native guards
    /// read its two refusals: a `JsValue` cannot be constructed on a native
    /// target, so a refusal asserted only through the binding panics inside
    /// `wasm-bindgen` before it reaches the code under test.
    pub(crate) fn update_caption_numbers_inner(&mut self) -> Result<EditResult, String> {
        let facts = EditorFacts::with_pages(self.document.definitions(), HashMap::new());
        let ids = &mut self.edit_ids;
        let placed =
            references::update_caption_numbers(&self.document, &facts, &mut || ids.next_id().ok())?;
        drop(facts);
        self.apply_action_caret_as(placed.operations, placed.caret, HistoryKind::FieldChange)
    }

    /// The operations [`WasmDocument::insert_cross_reference`] applies. A page
    /// reference reads the page the target's first position is painted on — the
    /// editor's one layout query in this feature, made once, for that one node.
    pub(crate) fn cross_reference_operations(
        &mut self,
        caret: Pos,
        target: casual_doc_model::NodeId,
        to: ReferenceTo,
        hyperlink: bool,
        include_above_below: bool,
    ) -> Result<Vec<casual_doc_edit::Operation>, String> {
        let mut pages = HashMap::new();
        if to == ReferenceTo::PageNumber
            && let Some(page) = self.page_label_of(target)
        {
            pages.insert(target, page);
        }
        let facts = EditorFacts::with_pages(self.document.definitions(), pages);
        let ids = &mut self.edit_ids;
        references::insert_cross_reference(
            &self.document,
            &facts,
            caret,
            target,
            to,
            hyperlink,
            include_above_below,
            &mut || ids.next_id().ok(),
        )
    }

    /// ONE walk of every block surface, carrying everything captions and
    /// cross-references need (`casual_doc_command::references::scan`).
    ///
    /// **O(document) time, and exactly one whole-document scan.**
    fn scan_references(&self, request: ScanRequest<'_>) -> ReferenceScan {
        let facts = EditorFacts::with_pages(self.document.definitions(), HashMap::new());
        references::scan(&self.document, &facts, request)
    }

    /// The label the page printing the start of `node` shows (`v` on the first page
    /// of a section numbered in lower-case roman from 5, as Word's `PAGEREF` shows),
    /// or `None` when the layout cannot say (a node the painted layout does not
    /// hold).
    ///
    /// The page is the one the target's first position is painted on; its label
    /// comes from `casual_doc_layout::paginate::page_number_labels`, the table the
    /// table of contents reads too, so an editor's page reference and an agent's
    /// print the same thing (`docs/181` §12, lane references, where the editor
    /// printed the PHYSICAL page number instead). A body laid out a window at a
    /// time has no whole-document label table, and keeps the physical number.
    ///
    /// **O(lines in the document)** for the geometry query plus O(pages) for the
    /// labels, once per inserted page reference. It is deliberately not called per
    /// caption: doing that inside a loop is the shape of the `docs/116` defect.
    pub(crate) fn page_label_of(&self, node: casual_doc_model::NodeId) -> Option<String> {
        let snapshot = self.painted_snapshot();
        let page = snapshot
            .caret_rect(self.view_pos(casual_doc_layout::model::ModelPos::new(node, 0)))
            .map(|(page, _)| page)?;
        if self.layout.is_windowed() {
            return Some(page.to_string());
        }
        let labels = casual_doc_layout::paginate::page_number_labels(
            self.layout.resident(),
            &self.document.definitions().sections,
        );
        let index = usize::try_from(page).ok()?.checked_sub(1)?;
        Some(
            labels
                .get(index)
                .cloned()
                .unwrap_or_else(|| page.to_string()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{node_plain_text, open_document};
    use casual_doc_command::references::{caption_target_refusal, locate_block, renumbered};
    use casual_doc_edit::Operation;
    use casual_doc_edit::references::{CAPTION_STYLE_NAME, REFERENCE_BOOKMARK_PREFIX};
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::{BlockNode, Document, FieldKind, InlineNode, Paragraph};

    impl WasmDocument {
        /// The operations Update Caption Numbers would apply and the first
        /// caption (for the caret), without applying them: empty operations
        /// means nothing is stale. The command refuses both empty cases, so the
        /// guards below ask it what it would do through the same lowering.
        fn caption_renumber_operations(&self) -> (Vec<Operation>, Option<NodeId>) {
            let facts = EditorFacts::with_pages(self.document.definitions(), HashMap::new());
            let mut ids = self.edit_ids.clone();
            let first = self
                .scan_references(ScanRequest::default())
                .captions
                .first()
                .map(|caption| caption.node);
            let operations =
                references::update_caption_numbers(&self.document, &facts, &mut || {
                    ids.next_id().ok()
                })
                .map(|placed| placed.operations)
                .unwrap_or_default();
            (operations, first)
        }
    }

    /// The one genuinely Microsoft-Word-produced file in the repository, so a
    /// round-trip assertion is against a real producer's package rather than our
    /// own writer's idea of one.
    const SAMPLE_DOCX: &[u8] = include_bytes!("../../../sample.docx");

    /// Arabic, below, label shown, no chapter number — the defaults Word's dialog
    /// opens with.
    fn insert_figure(document: &mut WasmDocument, target: &str, text: &str) -> Result<(), String> {
        document
            .insert_caption(
                target,
                "Figure",
                text.to_owned(),
                "below",
                false,
                "arabic",
                0,
                "hyphen",
            )
            .map(|_| ())
            .map_err(|error| format!("{error:?}"))
    }

    /// `captionEntries` parsed into `(label, number, text)`, dropping the node id.
    fn captions(document: &WasmDocument) -> Vec<(String, String, String)> {
        document
            .caption_entries()
            .iter()
            .map(|row| {
                let mut parts = row.split('\t');
                let _node = parts.next().unwrap_or_default();
                (
                    parts.next().unwrap_or_default().to_owned(),
                    parts.next().unwrap_or_default().to_owned(),
                    parts.collect::<Vec<_>>().join(" "),
                )
            })
            .collect()
    }

    /// A plain-text document of `lines` paragraphs — the cheapest way to get a
    /// document of a known size, and the same shape the class guard uses.
    fn plain_text_of(lines: usize) -> Vec<u8> {
        "A paragraph of ordinary body text.\r\n"
            .repeat(lines)
            .into_bytes()
    }

    /// Every top-level body paragraph id, in document order.
    fn body_paragraph_ids(document: &WasmDocument) -> Vec<String> {
        document
            .document
            .body()
            .iter()
            .filter_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph.id.to_string()),
                _ => None,
            })
            .collect()
    }

    fn first_body_paragraph(document: &WasmDocument) -> String {
        document
            .document
            .body()
            .iter()
            .find_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph.id.to_string()),
                _ => None,
            })
            .expect("the fixture has a body paragraph")
    }

    /// The exported package's `word/document.xml`, as text — the only way to
    /// assert what another word processor will actually read.
    fn exported_document_xml(document: &WasmDocument) -> String {
        let bytes = document.export_docx().expect("export docx");
        let mut package = crate::DocxPackage::open(&bytes, crate::viewer_limits())
            .expect("the export opens as a package");
        String::from_utf8(
            package
                .read_part("word/document.xml")
                .expect("read document.xml"),
        )
        .expect("document.xml is utf-8")
    }

    #[test]
    fn a_caption_is_numbered_from_one_and_reads_back_with_its_label() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert");

        assert_eq!(
            captions(&document),
            vec![(
                "Figure".to_owned(),
                "1".to_owned(),
                "Figure 1: Wiring diagram".to_owned()
            )],
            "the first caption of a label is number 1 and reads back whole"
        );
        assert_eq!(
            document.undo_label(),
            "Field change",
            "a caption is one undoable action a user can name"
        );
    }

    #[test]
    fn a_caption_inserted_before_an_existing_one_renumbers_the_one_after_it() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let body: Vec<String> = body_paragraph_ids(&document);
        // Caption the LATER paragraph first, so the second insertion lands before
        // it and must renumber it. A document that reads "Figure 1, Figure 1" is
        // the defect this guards.
        insert_figure(&mut document, &body[3], ": the later one").expect("insert later");
        insert_figure(&mut document, &body[0], ": the earlier one").expect("insert earlier");

        let numbers: Vec<(String, String)> = captions(&document)
            .into_iter()
            .map(|(_, number, text)| (number, text))
            .collect();
        assert_eq!(
            numbers,
            vec![
                ("1".to_owned(), "Figure 1: the earlier one".to_owned()),
                ("2".to_owned(), "Figure 2: the later one".to_owned()),
            ],
            "inserting a caption before another renumbers it, as Word does"
        );
    }

    #[test]
    fn undoing_a_caption_removes_the_paragraph_and_puts_the_numbers_back() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let body: Vec<String> = body_paragraph_ids(&document);
        insert_figure(&mut document, &body[3], ": the later one").expect("insert later");
        let before = captions(&document);
        insert_figure(&mut document, &body[0], ": the earlier one").expect("insert earlier");
        document.undo().expect("undo");

        assert_eq!(
            captions(&document),
            before,
            "one undo removes the caption AND reverses the renumbering it caused"
        );
    }

    #[test]
    fn a_caption_survives_a_word_round_trip_as_the_field_word_writes() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert");

        let xml = exported_document_xml(&document);
        assert!(
            xml.contains(" SEQ Figure \\* ARABIC "),
            "the export must carry the SEQ instruction verbatim, not a rendered number"
        );
        // `w:pStyle w:val` carries the exported style ID token, not the style's
        // human name, so the assertion that matters to another reader is that
        // `styles.xml` defines a style NAMED Caption and that the caption
        // paragraph resolves to it after a round trip.
        let reopened = open_document(&document.export_docx().expect("export")).expect("reopen");
        let caption_node = reopened.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();
        assert_eq!(
            reopened.paragraph_style_at(&caption_node),
            "Caption",
            "the caption paragraph must resolve to the Caption style after a round trip, \
             or another reader renders it as body text"
        );
        assert_eq!(
            captions(&reopened),
            captions(&document),
            "a caption read back out of our own DOCX is the same caption"
        );
    }

    #[test]
    fn include_chapter_number_writes_a_styleref_and_restarts_the_sequence() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        document
            .insert_caption(
                &target,
                "Figure",
                ": with a chapter".to_owned(),
                "below",
                false,
                "arabic",
                1,
                "hyphen",
            )
            .expect("insert");

        let xml = exported_document_xml(&document);
        assert!(
            xml.contains(" STYLEREF 1 \\s "),
            "the chapter number is a STYLEREF field, which is how Word writes it"
        );
        assert!(
            xml.contains(" SEQ Figure \\* ARABIC \\s 1 "),
            "the SEQ must carry the restart switch, or the number never restarts"
        );
    }

    #[test]
    fn a_caption_is_refused_outside_the_body_with_a_reason_rather_than_silently_misplaced() {
        const HEADER_FOOTER_DOCX: &[u8] =
            include_bytes!("../../../fixtures/corpus/real-producer-header-footer.docx");
        let document = open_document(HEADER_FOOTER_DOCX).expect("open corpus docx");
        let header = document
            .document
            .definitions()
            .headers
            .iter()
            .flat_map(|(_, header)| header.blocks.iter())
            .find_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph.id),
                _ => None,
            })
            .expect("the fixture carries a header paragraph, or this guard proves nothing");
        let refusal = caption_target_refusal(&document.document, header)
            .expect("a caption in a header must be refused");
        assert!(
            refusal.contains("body"),
            "the refusal must say WHY, so the control can be disabled with a reason \
             rather than doing nothing: {refusal}"
        );

        // And the body is not refused, or the guard would pass by refusing
        // everything.
        let body = document
            .document
            .body()
            .iter()
            .find_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph.id),
                _ => None,
            })
            .expect("the fixture has a body paragraph");
        assert_eq!(
            caption_target_refusal(&document.document, body),
            None,
            "a body paragraph must NOT be refused"
        );
    }

    #[test]
    fn a_cross_reference_points_at_one_bookmark_however_many_references_use_it() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert caption");
        let caption = document.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();

        let before = document.document.definitions().bookmarks.iter().count();
        for _ in 0..3 {
            document
                .insert_cross_reference(&target, 0, &caption, "entireCaption", true, false)
                .expect("insert cross-reference");
        }
        let after = document.document.definitions().bookmarks.iter().count();
        assert_eq!(
            after - before,
            1,
            "three references to one caption must share one bookmark, not make three"
        );
    }

    #[test]
    fn every_reference_kind_caches_the_result_a_reader_will_see() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert caption");
        let caption = document.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();

        // The whole caption's text.
        document
            .insert_cross_reference(&target, 0, &caption, "entireCaption", true, false)
            .expect("entire caption");
        let xml = exported_document_xml(&document);
        assert!(
            xml.contains(" REF _Ref"),
            "a text cross-reference is a REF field: {}",
            &xml[..0]
        );

        // The page, which comes from layout and is the one hard coupling.
        document
            .insert_cross_reference(&target, 0, &caption, "pageNumber", true, false)
            .expect("page number");
        let xml = exported_document_xml(&document);
        assert!(
            xml.contains(" PAGEREF _Ref"),
            "a page cross-reference is a PAGEREF field"
        );

        // Above or below, which comes from document order.
        document
            .insert_cross_reference(&target, 0, &caption, "aboveBelow", false, false)
            .expect("above/below");
        let xml = exported_document_xml(&document);
        assert!(xml.contains("\\p "), "above/below is the REF \\p switch");
        assert!(
            xml.contains(">below<") || xml.contains(">above<"),
            "the cached result must be the word a reader sees, not an empty field"
        );
    }

    #[test]
    fn a_cross_reference_survives_a_round_trip_with_its_bookmark_intact() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert caption");
        let caption = document.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();
        document
            .insert_cross_reference(&target, 0, &caption, "entireCaption", true, false)
            .expect("insert cross-reference");

        let reopened = open_document(&document.export_docx().expect("export")).expect("reopen");
        let names: Vec<String> = reopened
            .document
            .definitions()
            .bookmarks
            .iter()
            .map(|(_, bookmark)| bookmark.name.clone())
            .collect();
        assert!(
            names.iter().any(|name| name.starts_with("_Ref")),
            "the target's bookmark must survive the round trip, or the REF dangles: {names:?}"
        );
        // And the field is still a REF field pointing at that name, not flattened
        // into its cached text.
        let referenced: Vec<String> = {
            let mut found = Vec::new();
            crate::visit_paragraphs_all_surfaces(&reopened.document, &mut |paragraph| {
                for inline in &paragraph.inlines {
                    if let InlineNode::Field(field) = inline
                        && let FieldKind::Ref { bookmark } = &field.kind
                    {
                        found.push(bookmark.clone());
                    }
                }
            });
            found
        };
        assert!(
            referenced.iter().any(|name| name.starts_with("_Ref")),
            "the reopened document must still hold a REF field: {referenced:?}"
        );
    }

    #[test]
    fn reference_targets_answer_per_reference_type_and_refuse_nothing_silently() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert caption");

        assert!(
            !document.reference_targets("heading").is_empty(),
            "the fixture has headings, so the heading type must offer targets"
        );
        assert_eq!(
            document.reference_targets("Figure").len(),
            1,
            "the Figure type offers the one caption that exists"
        );
        assert!(
            document.reference_targets("Table").is_empty(),
            "a label with no captions offers none — an empty list, not an error"
        );
        assert!(
            document.reference_targets("nonsense").is_empty(),
            "an unknown type yields an empty list the host can explain, not a throw"
        );
        assert!(
            document.caption_labels().contains(&"Figure".to_owned()),
            "the label list carries the built-ins"
        );
    }

    /// A label the document invented, not one of ours, must appear in the label
    /// list and number independently — the property that makes this work on a file
    /// someone else wrote.
    #[test]
    fn a_label_the_document_invented_gets_its_own_counter() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let body: Vec<String> = body_paragraph_ids(&document);
        insert_figure(&mut document, &body[0], ": a figure").expect("figure");
        document
            .insert_caption(
                &body[3],
                "Listing",
                ": a listing".to_owned(),
                "below",
                false,
                "arabic",
                0,
                "hyphen",
            )
            .expect("listing");

        assert!(
            document.caption_labels().contains(&"Listing".to_owned()),
            "a label the document uses joins the list"
        );
        let numbers: Vec<(String, String)> = captions(&document)
            .into_iter()
            .map(|(label, number, _)| (label, number))
            .collect();
        assert_eq!(
            numbers,
            vec![
                ("Figure".to_owned(), "1".to_owned()),
                ("Listing".to_owned(), "1".to_owned()),
            ],
            "two labels are two sequences; neither continues the other"
        );
    }

    /// A caption inserted AFTER an existing one of the same label continues its
    /// sequence.
    ///
    /// Separate from the renumbering guard on purpose: that one inserts backwards,
    /// so a build that numbered every caption `1` and then renumbered what follows
    /// still produced 1, 2 and passed. Inserting forwards is the case where the new
    /// caption's own number has to count what is already there.
    #[test]
    fn a_caption_after_an_existing_one_continues_its_sequence() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let body: Vec<String> = body_paragraph_ids(&document);
        insert_figure(&mut document, &body[0], ": the first").expect("first");
        insert_figure(&mut document, &body[3], ": the second").expect("second");
        insert_figure(&mut document, &body[5], ": the third").expect("third");

        let numbers: Vec<String> = captions(&document)
            .into_iter()
            .map(|(_, number, _)| number)
            .collect();
        assert_eq!(
            numbers,
            vec!["1".to_owned(), "2".to_owned(), "3".to_owned()],
            "captions inserted in document order must number 1, 2, 3"
        );
    }

    /// The caption paragraph carries the `Caption` style, and the style is created
    /// when the document has not got one.
    ///
    /// Its own guard rather than a line in the round-trip test, because the
    /// round-trip test passed with the styling removed: the caption's *text* is the
    /// same either way, and a caption that renders as body text is exactly the
    /// "built is not reachable" failure `docs/99` §9.4 records.
    #[test]
    fn a_caption_is_styled_as_a_caption_and_the_style_is_created_if_missing() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert");

        let caption = document.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();
        assert_eq!(
            document.paragraph_style_at(&caption),
            CAPTION_STYLE_NAME,
            "the caption paragraph must resolve to the Caption style, or it renders \
             as body text"
        );
        assert!(
            document
                .list_styles()
                .contains(&CAPTION_STYLE_NAME.to_owned()),
            "the Caption style must exist in the document, as Word creates it"
        );
    }

    /// Word's three caption reference kinds show three DIFFERENT things.
    ///
    /// They are all a plain `REF` field — what differs is the extent of the bookmark
    /// it names, either side of the `SEQ` field. A build that bookmarked the whole
    /// caption for each would put three rows in the dialog that do the same thing,
    /// and every instruction-level assertion would still pass, because the
    /// instructions ARE identical. So this guard reads the cached result, which is
    /// the only thing that differs.
    #[test]
    fn the_three_caption_reference_kinds_show_three_different_things() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert caption");
        let caption = document.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();

        let shown = |document: &mut WasmDocument, kind: &str| {
            document
                .insert_cross_reference(&target, 0, &caption, kind, false, false)
                .expect("insert cross-reference");
            // The field just inserted is the first `REF`/`PAGEREF` in the caret's
            // paragraph; read its cached result.
            let mut text = String::new();
            crate::visit_paragraphs_all_surfaces(&document.document, &mut |paragraph| {
                if paragraph.id.to_string() != target || !text.is_empty() {
                    return;
                }
                for inline in &paragraph.inlines {
                    if let InlineNode::Field(field) = inline
                        && matches!(
                            field.kind,
                            FieldKind::Ref { .. } | FieldKind::PageRef { .. }
                        )
                    {
                        text = node_plain_text(&field.inlines);
                        return;
                    }
                }
            });
            document.undo().expect("undo the reference");
            text
        };

        let entire = shown(&mut document, "entireCaption");
        let label_and_number = shown(&mut document, "labelAndNumber");
        let caption_text = shown(&mut document, "captionText");

        assert_eq!(entire, "Figure 1: Wiring diagram");
        assert_eq!(
            label_and_number, "Figure 1",
            "\"only label and number\" must stop at the end of the SEQ field"
        );
        assert_eq!(
            caption_text, ": Wiring diagram",
            "\"only caption text\" must start at the end of the SEQ field"
        );
        assert!(
            entire != label_and_number && entire != caption_text,
            "three reference kinds that show the same string are three dialog rows \
             that do the same thing"
        );
    }

    /// A whole-caption reference bookmarks the WHOLE caption, not the trimmed
    /// display string a picker shows.
    ///
    /// Using the trimmed length as the bookmark's end offset never errors — a
    /// trimmed length is never longer — so it passes every test that only checks the
    /// instruction, and another word processor then shows truncated text. The
    /// fixture's caption ends in a space so trimmed and untrimmed differ.
    #[test]
    fn a_reference_bookmark_covers_the_whole_target_not_its_trimmed_display_text() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram   ").expect("insert caption");
        let caption = document.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();
        let caption_id = caption.parse::<NodeId>().expect("a node id");
        document
            .insert_cross_reference(&target, 0, &caption, "entireCaption", false, false)
            .expect("insert cross-reference");

        // The bookmark's end marker must sit at the caption paragraph's real end.
        let mut length = 0u32;
        let mut end_offset = None;
        crate::visit_paragraphs_all_surfaces(&document.document, &mut |paragraph| {
            if paragraph.id != caption_id {
                return;
            }
            length = node_plain_text(&paragraph.inlines).len() as u32;
            for (index, inline) in paragraph.inlines.iter().enumerate() {
                if matches!(inline, InlineNode::BookmarkEnd(_)) {
                    end_offset = Some(node_plain_text(&paragraph.inlines[..index]).len() as u32);
                }
            }
        });
        assert!(
            length > 0,
            "the caption must have text, or this proves nothing"
        );
        assert_eq!(
            end_offset,
            Some(length),
            "the bookmark must end at the caption's real end ({length} bytes), not at \
             the end of its trimmed display text"
        );
    }

    /// *Include above/below* inserts the word as a SECOND field, in one action.
    ///
    /// Word writes two fields for that checkbox — the reference, a space, and a
    /// ` REF … \p ` — and the alternative to taking the argument was shipping the
    /// checkbox disabled, which is a dead control.
    #[test]
    fn include_above_below_adds_a_second_reference_field_in_the_same_action() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let body = body_paragraph_ids(&document);
        // Caption a LATER paragraph and reference it from an earlier one, so the
        // word is "below" — a guard that could only ever read "above" would pass
        // on a build that hard-coded it.
        insert_figure(&mut document, &body[4], ": Wiring diagram").expect("insert caption");
        let caption = document.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();

        document
            .insert_cross_reference(&body[0], 0, &caption, "entireCaption", true, true)
            .expect("insert with above/below");

        let mut instructions = Vec::new();
        let mut text = String::new();
        let anchor = body[0].parse::<NodeId>().expect("a node id");
        crate::visit_paragraphs_all_surfaces(&document.document, &mut |paragraph| {
            if paragraph.id != anchor {
                return;
            }
            text = node_plain_text(&paragraph.inlines);
            for inline in &paragraph.inlines {
                if let InlineNode::Field(field) = inline {
                    instructions.push(field.instruction.clone());
                }
            }
        });
        assert_eq!(
            instructions.len(),
            2,
            "two fields: the reference and the above/below word — got {instructions:?}"
        );
        assert!(
            instructions.iter().any(|i| i.contains("\\p")),
            "the second field must be the REF \\p Word writes: {instructions:?}"
        );
        assert!(
            text.starts_with("Figure 1: Wiring diagram below"),
            "the paragraph must read the reference, a space, then the word: {text:?}"
        );
        // ONE undo, not three. This is the assertion that the three ops are one
        // action; the undo LABEL cannot say so, because the caption insertion before
        // it is also a "Field change" and comparing labels would pass either way.
        document.undo().expect("undo");
        let mut after = String::new();
        crate::visit_paragraphs_all_surfaces(&document.document, &mut |paragraph| {
            if paragraph.id == anchor {
                after = node_plain_text(&paragraph.inlines);
            }
        });
        assert!(
            !after.contains("below") && !after.contains("Figure 1"),
            "ONE undo must remove both fields and the space: {after:?}"
        );
    }

    /// Inserting a field leaves the caret AFTER it, not before it.
    ///
    /// The engine rested it at the insertion point and a comment there asserted "a
    /// field is zero-width in the edit anchor space" — which is false: a field's
    /// cached result is part of the paragraph's plain text, because
    /// `node_plain_text` recurses into `Field::inlines`. The caret therefore landed
    /// immediately BEFORE the field a reader had just inserted.
    ///
    /// It was invisible for as long as a field's own glyphs carried no caret stops:
    /// the caret fell back to another position in the line and landed somewhere
    /// plausible, and the e2e assertion "the insert leaves the caret immediately
    /// after the field" passed while the code did the opposite. That is the
    /// green-for-the-wrong-reason failure `SKILL.md` §4 is about, and it surfaced
    /// only when a cached-result field started flowing as ordinary text (OO-005) and
    /// the layout began agreeing with the model.
    ///
    /// This guard asserts the MODEL OFFSET, not pixels: the geometry follows from
    /// it, and a pixel assertion is what let the defect hide in the first place.
    #[test]
    fn inserting_a_field_leaves_the_caret_after_it() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);

        // A cached-result field: the engine has no clock, so the host supplies the
        // already-formatted string — and its length is what the caret must clear.
        let result = document
            .insert_field(&target, 0, "author", Some("Ada Lovelace".to_owned()))
            .expect("insert an author field");
        assert_eq!(
            result.node(),
            target,
            "the caret stays in the same paragraph"
        );
        assert_eq!(
            result.offset(),
            "Ada Lovelace".len() as u32,
            "the caret must rest past the field's cached text, so the next character \
             typed lands after the field and not before it"
        );

        // And the same for a caption's SEQ field, which is the shape OO-005 inserts:
        // the caret lands at the start of the new caption paragraph, so this asserts
        // the sibling guarantee rather than repeating the one above.
        let caption = document
            .insert_caption(
                &target,
                "Figure",
                ": Wiring diagram".to_owned(),
                "below",
                false,
                "arabic",
                0,
                "hyphen",
            )
            .expect("insert a caption");
        assert_eq!(
            caption.offset(),
            0,
            "a caption's caret rests at the start of the caption paragraph it made"
        );
    }

    /// A caption's number can go stale, the engine says which, and a reader can fix
    /// it — the "dirty, not silently stale" contract.
    ///
    /// Staleness is created here the way a user creates it: by deleting a caption
    /// paragraph's text, which is ordinary editing and deliberately does NOT
    /// renumber (renumbering on a keystroke would make typing O(document)).
    #[test]
    fn a_stale_caption_number_is_reported_and_can_be_put_right() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let body = body_paragraph_ids(&document);
        insert_figure(&mut document, &body[0], ": the first").expect("first");
        insert_figure(&mut document, &body[3], ": the second").expect("second");
        assert!(
            document.stale_caption_numbers().is_empty(),
            "nothing this engine inserted is stale on arrival"
        );

        // Delete the FIRST caption's paragraph outright. The second still shows 2,
        // but the document now resolves it to 1. Applied straight through the edit
        // crate rather than through a binding, because that is what the two real
        // sources of staleness look like to this module: a change it did not make.
        let first = document.caption_entries()[0]
            .split('\t')
            .next()
            .and_then(|id| id.parse::<NodeId>().ok())
            .expect("the caption's node id");
        let index = document
            .document
            .body()
            .iter()
            .position(|block| matches!(block, BlockNode::Paragraph(p) if p.id == first))
            .expect("the caption is a top-level body paragraph");
        casual_doc_edit::apply(
            &mut document.document,
            casual_doc_edit::Mint::reserve(&mut document.edit_ids, 1).expect("an identity space"),
            &Operation::DeleteBlocks {
                container: None,
                index: index as u32,
                count: 1,
            },
        )
        .expect("delete the caption paragraph");

        let stale = document.stale_caption_numbers();
        assert_eq!(
            stale.len(),
            1,
            "the surviving caption must be reported as showing the wrong number: {stale:?}"
        );
        assert!(
            stale[0].ends_with("\t2\t1"),
            "the report must name what is shown and what is right: {}",
            stale[0]
        );

        document
            .update_caption_numbers()
            .expect("updating puts it right");
        assert!(
            document.stale_caption_numbers().is_empty(),
            "after an update nothing is stale"
        );
        assert_eq!(
            document
                .caption_entries()
                .iter()
                .map(|row| row.split('\t').nth(2).unwrap_or_default().to_owned())
                .collect::<Vec<_>>(),
            vec!["1".to_owned()],
            "and the caption now SHOWS the right number"
        );
        // A second update has nothing to do, so it builds no operations and the
        // binding refuses rather than pushing an empty undo step.
        assert!(
            document.caption_renumber_operations().0.is_empty(),
            "updating a correct document must produce no operations, so the command \
             can refuse with a reason instead of making an empty history entry"
        );
    }

    /// **Typing stays O(1) in document size.**
    ///
    /// A caption's number is resolved once, at insertion, and cached in the model
    /// as the field's result. So a keystroke must cost a bounded number of
    /// whole-document scans, that number must not change because the document holds
    /// captions, and it must not grow with the document.
    ///
    /// All three assertions are needed, and the third is why there is a budget
    /// rather than only a comparison: a renumbering pass bolted onto the keystroke
    /// path costs ONE extra whole-document scan, which is a real O(document)
    /// keystroke but is the same number at any document size and in a document with
    /// or without captions. A comparison alone passes it; the budget does not.
    /// `docs/107` §4 makes per-keystroke work an owner constraint, so this is a
    /// budget, not a measurement — raising it needs the same justification any
    /// other budget change needs.
    #[test]
    fn a_keystroke_costs_a_bounded_number_of_document_scans() {
        /// ONE. The editing path resolves the caret's surface once, and that is
        /// the whole per-keystroke document cost. This is a budget, not a
        /// measurement: `docs/107` §4 makes per-keystroke work O(1) in document
        /// size an owner constraint, so a second whole-document walk on the
        /// keystroke path is a defect to fix, not a number to raise. A renumbering
        /// pass bolted on here costs exactly one more, which is why the budget is
        /// tight enough to notice it.
        const BUDGET: usize = 1;

        let scans_for_a_keystroke = |lines: usize, with_caption: bool| {
            let mut document = open_document(&plain_text_of(lines)).expect("open");
            let target = body_paragraph_ids(&document)[0].clone();
            if with_caption {
                insert_figure(&mut document, &target, ": Wiring diagram").expect("insert");
            }
            casual_doc_edit::reset_document_scans();
            document
                .insert_text(&target, 0, "x".to_owned())
                .expect("type one character");
            casual_doc_edit::document_scans()
        };

        let plain = scans_for_a_keystroke(200, false);
        let captioned = scans_for_a_keystroke(200, true);
        let captioned_twice_as_long = scans_for_a_keystroke(400, true);
        assert!(
            captioned <= BUDGET,
            "a keystroke in a document with a caption scanned the whole document \
             {captioned} times, against a budget of {BUDGET}; per-keystroke work is \
             O(1) in document size (`docs/107` §4)"
        );
        assert_eq!(
            plain, captioned,
            "a document with a caption must not make a keystroke cost more than the \
             same document without one"
        );
        assert_eq!(
            captioned, captioned_twice_as_long,
            "a keystroke cost {captioned} scans at 200 paragraphs and \
             {captioned_twice_as_long} at 400 — the cost grows with the document"
        );
    }

    /// **Enumerating the captions is one walk, not one walk per caption.**
    ///
    /// The class guard in this crate's `tests` module
    /// (`document_wide_reads_do_not_scan_the_document_once_per_node`) covers the
    /// same rule for every document-wide read, but its fixture is plain text and
    /// therefore holds **no captions** — so a scan added inside the per-caption loop
    /// would never execute there, and the guard would pass while the defect shipped.
    /// That is the "green for the wrong reason" failure `SKILL.md` §4 warns about,
    /// found by mutating this exact code and watching the class guard stay green.
    /// This guard uses a caption-bearing document, and asserts both shapes: a budget
    /// (which catches per-caption cost) and a doubling (which catches per-paragraph
    /// cost).
    #[test]
    fn enumerating_captions_costs_one_walk_however_many_captions_there_are() {
        /// Enough captions that one scan each is unmistakable against the budget.
        const CAPTIONS: usize = 20;
        /// A handful of whole-document scans, independent of the caption count.
        const BUDGET: usize = 6;

        let scans = |lines: usize| {
            let mut document = open_document(&plain_text_of(lines)).expect("open");
            let ids = body_paragraph_ids(&document);
            for index in 0..CAPTIONS {
                insert_figure(&mut document, &ids[index * 2], ": a figure").expect("insert");
            }
            assert_eq!(
                document.caption_entries().len(),
                CAPTIONS,
                "the fixture must actually hold captions, or this guard proves nothing"
            );
            casual_doc_edit::reset_document_scans();
            let _ = document.caption_entries();
            let _ = document.caption_labels();
            let _ = document.reference_targets("Figure");
            casual_doc_edit::document_scans()
        };

        let one = scans(120);
        let two = scans(240);
        assert!(
            one <= BUDGET,
            "reading {CAPTIONS} captions scanned the whole document {one} times, \
             against a budget of {BUDGET}; a document-wide read resolves ids once, \
             not once per caption (`docs/116`)"
        );
        assert_eq!(
            one, two,
            "reading the captions scanned the document {one} times at 120 paragraphs \
             and {two} times at 240 — the cost grows with the document"
        );
    }

    // ---- A paragraph's inline containers are descended, on every reference path -
    // Four walks in this file stopped at a paragraph's top-level inlines or at
    // three of the four inline containers. The guards below assert the guarantee a
    // user would notice — a caption is listed, a bookmark is offered, a cross
    // reference resolves to text, Insert Caption is not refused — rather than the
    // shape of what a walk returned.

    /// Ids for a fixture, from one space so nothing collides with the sample.
    fn fixture_ids(space: u64) -> impl FnMut() -> NodeId {
        let mut next = 1u64;
        move || {
            next += 1;
            NodeId::from_parts(space, next).unwrap()
        }
    }

    /// A hyperlink target for a fixture whose point is the WRAPPER, not where it
    /// points.
    fn fixture_link_target() -> casual_doc_model::v1::HyperlinkTarget {
        casual_doc_model::v1::HyperlinkTarget::Internal(casual_doc_model::v1::InternalTarget {
            anchor: "Anywhere".to_owned(),
        })
    }

    fn fixture_run(id: NodeId, text: &str) -> InlineNode {
        InlineNode::Run(casual_doc_model::v1::Run {
            id,
            properties: casual_doc_model::v1::RunProperties::default().into(),
            text: text.to_owned(),
        })
    }

    fn fixture_paragraph(id: NodeId, inlines: Vec<InlineNode>) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id,
            properties: casual_doc_model::v1::ParagraphProperties::default().into(),
            inlines,
        })
    }

    /// A `SEQ Figure` field whose cached result reads `number`.
    fn fixture_seq(id: NodeId, result: NodeId, number: &str) -> InlineNode {
        InlineNode::Field(Box::new(casual_doc_model::v1::Field {
            id,
            instruction: "SEQ Figure \\* ARABIC".to_owned(),
            kind: FieldKind::Seq {
                name: "Figure".to_owned(),
            },
            inlines: vec![fixture_run(result, number)],
            form: None,
            update: casual_doc_model::v1::FieldUpdateState::default(),
        }))
    }

    fn fixture_text_box(id: NodeId, blocks: Vec<BlockNode>) -> InlineNode {
        InlineNode::TextBox(Box::new(casual_doc_model::v1::TextBox {
            hyperlink: None,
            id,
            anchor: None,
            relative_height: None,
            extent: Some(casual_doc_model::v1::Extent {
                width_emu: 2_743_200,
                height_emu: 1_828_800,
            }),
            fill: None,
            border: None,
            body_properties: casual_doc_model::v1::TextBoxBodyProperties::default(),
            blocks,
        }))
    }

    fn fixture_sdt(id: NodeId, inlines: Vec<InlineNode>) -> InlineNode {
        InlineNode::Sdt(Box::new(casual_doc_model::v1::InlineSdt {
            id,
            properties: casual_doc_model::v1::SdtProperties::default(),
            inlines,
        }))
    }

    /// A caption paragraph inside a text box that itself sits inside an inline
    /// content control — the box a figure caption is authored in, in a document
    /// whose fields a producer wrapped in a control.
    ///
    /// This is the case the whole caption feature could not see: the paragraph walk
    /// (`visit_text_box_paragraphs`) descended `TextBox`, `Hyperlink`, `Field` and
    /// `Group` and missed `Sdt` and `Revision`, so the caption paragraph inside the
    /// box was never visited and the caption was absent from `captionEntries`, from
    /// the cross-reference picker, and from renumbering.
    #[test]
    fn a_caption_in_a_box_inside_a_content_control_is_listed_and_numbered() {
        let mut id = fixture_ids(71);
        let plain_caption = id();
        let boxed_caption = id();
        let document = Document::new(
            NodeId::from_parts(71, 1).unwrap(),
            vec![
                // A plain body caption first, so the numbering has something to
                // count from and the guard can tell "not found" from "found but
                // mis-numbered".
                fixture_paragraph(
                    plain_caption,
                    vec![
                        fixture_run(id(), "Figure "),
                        fixture_seq(id(), id(), "1"),
                        fixture_run(id(), ": in the body"),
                    ],
                ),
                fixture_paragraph(
                    id(),
                    vec![fixture_sdt(
                        id(),
                        vec![fixture_text_box(
                            id(),
                            vec![fixture_paragraph(
                                boxed_caption,
                                vec![
                                    fixture_run(id(), "Figure "),
                                    fixture_seq(id(), id(), "1"),
                                    fixture_run(id(), ": in a box in a control"),
                                ],
                            )],
                        )],
                    )],
                ),
            ],
            casual_doc_model::v1::Definitions::default(),
        )
        .expect("a valid boxed-caption document");

        let d = crate::tests::wasm_document(document);
        let rows = captions(&d);
        assert!(
            rows.iter().any(|(_, _, text)| text.contains("in the body")),
            "the fixture's plain caption must be listed, or this guard cannot tell \
             a missing descent from a broken scan: {rows:?}"
        );
        assert!(
            rows.iter()
                .any(|(_, _, text)| text.contains("in a box in a control")),
            "a caption in a text box inside an inline content control is missing \
             from the caption list, so it is missing from the cross-reference \
             picker and from a Table of Figures: {rows:?}"
        );
        assert_eq!(
            rows.iter()
                .find(|(_, _, text)| text.contains("in a box in a control"))
                .map(|(_, number, _)| number.as_str()),
            Some("1"),
            "the boxed caption's CACHED number is what the document shows, and it \
             shows 1; renumbering is what changes it, and renumbering could not see \
             it either: {rows:?}"
        );
        // And renumbering can now reach it: two captions of one label cannot both
        // be 1, so an update must produce work.
        let (ops, first) = d.caption_renumber_operations();
        assert!(
            first.is_some() && !ops.is_empty(),
            "two captions of the same label both cached as 1 must be stale; a \
             renumber that produces nothing means one of them is invisible"
        );
    }

    /// A cross-reference to a caption whose `SEQ` field sits inside a hyperlink
    /// resolves to the caption's text, not to nothing.
    ///
    /// `paragraph_offsets` looked at top-level inlines only, so for such a caption
    /// `after_sequence` was `None` and every reference kind fell back to the whole
    /// paragraph — Word's "Only label and number" and "Only caption text" silently
    /// became "Entire caption", which is three rows in a dialog that do the same
    /// thing.
    #[test]
    fn a_seq_inside_a_hyperlink_still_splits_label_from_caption_text() {
        let mut id = fixture_ids(72);
        let caption = id();
        let caret = id();
        let document = Document::new(
            NodeId::from_parts(72, 1).unwrap(),
            vec![
                fixture_paragraph(
                    caption,
                    vec![InlineNode::Hyperlink(Box::new(
                        casual_doc_model::v1::Hyperlink {
                            id: id(),
                            target: fixture_link_target(),
                            tooltip: None,
                            inlines: vec![
                                fixture_run(id(), "Figure "),
                                fixture_seq(id(), id(), "1"),
                            ],
                            history: false,
                        },
                    ))],
                ),
                fixture_paragraph(caret, vec![fixture_run(id(), "see ")]),
            ],
            casual_doc_model::v1::Definitions::default(),
        )
        .expect("a valid hyperlinked-caption document");

        let mut d = crate::tests::wasm_document(document);
        // Append the caption's own text after the field, inside the link, so the
        // label/number boundary has text on both sides of it.
        {
            let BlockNode::Paragraph(paragraph) = &mut d.document.body_mut()[0] else {
                panic!("the caption paragraph");
            };
            let InlineNode::Hyperlink(link) = &mut paragraph.inlines[0] else {
                panic!("the hyperlink");
            };
            link.inlines
                .push(fixture_run(NodeId::from_parts(72, 90).unwrap(), ": Wiring"));
        }

        let scan = d.scan_references(ScanRequest {
            interest: &[caption],
            keep_inlines: false,
        });
        let found = scan.found(caption).expect("the caption is in the scan");
        assert_eq!(
            found.length,
            "Figure 1: Wiring".len() as u32,
            "the paragraph's length must count text nested in the hyperlink, or the \
             bookmark a reference creates stops short of what it names"
        );
        assert_eq!(
            found.after_sequence,
            Some("Figure 1".len() as u32),
            "the boundary between the caption's label-and-number and its own text \
             is the end of the `SEQ` field, and a `SEQ` inside a hyperlink is still \
             the caption's `SEQ`"
        );

        // And a caption this engine LISTS is one it can RENUMBER. `renumbered`
        // rewrote a top-level `SEQ` only, which agreed with `sequence_of` while
        // that was also top-level and became a half-fix the moment it was not: the
        // caption would be listed, counted, and then silently skipped by every
        // renumber, so Update Caption Numbers reported nothing stale while the
        // document read 1, 1, 2.
        let BlockNode::Paragraph(caption_paragraph) = &d.document.body()[0] else {
            panic!("the caption paragraph");
        };
        let mut allocate = || Some(NodeId::from_parts(72, 95).unwrap());
        let rewritten = renumbered(&caption_paragraph.inlines, "4", &mut allocate)
            .expect("a caption whose SEQ sits inside a hyperlink can be renumbered");
        assert_eq!(
            node_plain_text(&rewritten),
            "Figure 4: Wiring",
            "the renumber must reach the nested `SEQ`'s cached result"
        );
    }

    /// A `_Ref` bookmark nested inside a hyperlink is found and reused, so ten
    /// references to one figure carry one bookmark rather than ten.
    ///
    /// The extent scan was top-level only, so a bookmark Word itself had written
    /// inside the link of a hyperlinked cross-reference was invisible and every new
    /// reference minted another `_Ref…` over the same range.
    #[test]
    fn a_reference_bookmark_nested_in_a_hyperlink_is_reused() {
        use casual_doc_model::v1::{Bookmark, BookmarkEnd, BookmarkStart};

        let mut id = fixture_ids(73);
        let bookmark = casual_doc_model::v1::BookmarkId::new(NodeId::from_parts(73, 800).unwrap());
        let mut definitions = casual_doc_model::v1::Definitions::default();
        definitions.bookmarks.insert(
            bookmark,
            Bookmark {
                name: format!("{REFERENCE_BOOKMARK_PREFIX}123456789"),
            },
        );
        let caption = id();
        let document = Document::new(
            NodeId::from_parts(73, 1).unwrap(),
            vec![fixture_paragraph(
                caption,
                vec![InlineNode::Hyperlink(Box::new(
                    casual_doc_model::v1::Hyperlink {
                        id: id(),
                        target: fixture_link_target(),
                        tooltip: None,
                        inlines: vec![
                            InlineNode::BookmarkStart(BookmarkStart { id: id(), bookmark }),
                            fixture_run(id(), "Figure 1"),
                            InlineNode::BookmarkEnd(BookmarkEnd { id: id(), bookmark }),
                        ],
                        history: false,
                    },
                ))],
            )],
            definitions,
        )
        .expect("a valid nested-bookmark document");

        let d = crate::tests::wasm_document(document);
        let scan = d.scan_references(ScanRequest {
            interest: &[caption],
            keep_inlines: false,
        });
        assert_eq!(
            scan.reference_bookmark(caption, 0, "Figure 1".len() as u32),
            Some(bookmark),
            "a `_Ref` bookmark inside a hyperlink covers the same characters as one \
             beside it, and must be reused for a reference over that range rather \
             than duplicated"
        );
    }

    /// A picture selected inside a text box resolves to the body block that
    /// anchors the box, so a caption has somewhere to go.
    ///
    /// `inlines_hold` searched the four inline-in-inline containers only, so a node
    /// inside a text box's own block content was not "held" by any paragraph and
    /// `locate_block` returned `None` — the insertion point for Word's most
    /// ordinary floated figure, a picture in a box, could not be computed at all.
    ///
    /// **Insert Caption on such a picture is still refused, one layer above this,
    /// and the guard says so rather than pretending otherwise.**
    /// `caption_target_refusal` asks `casual_doc_edit::surface_of`, whose
    /// `blocks_contain` matches paragraph, table, row and cell ids and does not
    /// descend a paragraph's inline text boxes, so it answers "not in the document"
    /// and the command refuses before `locate_block` is consulted. That function is
    /// in another lane's crate, so it is reported as a row instead of edited here —
    /// and the assertion below pins the refusal that is CURRENTLY correct behaviour
    /// for this engine, so the day `surface_of` learns text boxes this guard fails
    /// and is updated deliberately rather than silently passing on a changed
    /// meaning.
    #[test]
    fn a_picture_inside_a_text_box_resolves_to_the_block_that_anchors_it() {
        use casual_doc_model::v1::{Definitions, Drawing, MediaId, MediaReference};

        let mut id = fixture_ids(74);
        let media = MediaId::new(NodeId::from_parts(74, 800).unwrap());
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rId74".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/image1.png".to_owned(),
            },
        );
        let picture = id();
        let document = Document::new(
            NodeId::from_parts(74, 1).unwrap(),
            vec![fixture_paragraph(
                id(),
                vec![fixture_text_box(
                    id(),
                    vec![fixture_paragraph(
                        id(),
                        vec![InlineNode::Drawing(Box::new(Drawing {
                            hyperlink: None,
                            opacity: None,
                            id: picture,
                            media,
                            extent: Some(casual_doc_model::v1::Extent {
                                width_emu: 914_400,
                                height_emu: 914_400,
                            }),
                            crop: None,
                            descr: None,
                            border: None,
                            flip_h: false,
                            flip_v: false,
                            rotation: None,
                        }))],
                    )],
                )],
            )],
            definitions,
        )
        .expect("a valid boxed-picture document");

        let d = crate::tests::wasm_document(document);
        assert_eq!(
            locate_block(&d.document, picture),
            Some((None, 0)),
            "a picture selected inside a text box must resolve to the body block \
             that anchors the box — index 0 of the body, with no `InsertBlocks` \
             container, because the engine cannot address a text box as a container"
        );
        assert_eq!(
            caption_target_refusal(&d.document, picture),
            Some("the caption's target is not in the document"),
            "Insert Caption is still refused for a boxed picture, by \
             `casual_doc_edit::surface_of`, which does not descend a paragraph's \
             inline text boxes. Reported as a row; when it does, update this guard \
             deliberately"
        );
    }

    /// An authored bookmark inside a hyperlink is offered as a cross-reference
    /// target.
    ///
    /// `bookmark_targets` looked at a paragraph's top-level inlines, so a bookmark
    /// an author had placed over linked text simply had no row in the picker — a
    /// capability present in the model and unreachable from the product.
    #[test]
    fn a_bookmark_inside_a_hyperlink_is_offered_as_a_target() {
        use casual_doc_model::v1::{Bookmark, BookmarkEnd, BookmarkStart};

        let mut id = fixture_ids(75);
        let nested = casual_doc_model::v1::BookmarkId::new(NodeId::from_parts(75, 800).unwrap());
        let top_level = casual_doc_model::v1::BookmarkId::new(NodeId::from_parts(75, 801).unwrap());
        let mut definitions = casual_doc_model::v1::Definitions::default();
        for (bookmark, name) in [(nested, "InsideTheLink"), (top_level, "BesideTheLink")] {
            definitions.bookmarks.insert(
                bookmark,
                Bookmark {
                    name: name.to_owned(),
                },
            );
        }
        let document = Document::new(
            NodeId::from_parts(75, 1).unwrap(),
            vec![fixture_paragraph(
                id(),
                vec![
                    InlineNode::BookmarkStart(BookmarkStart {
                        id: id(),
                        bookmark: top_level,
                    }),
                    fixture_run(id(), "plain"),
                    InlineNode::BookmarkEnd(BookmarkEnd {
                        id: id(),
                        bookmark: top_level,
                    }),
                    InlineNode::Hyperlink(Box::new(casual_doc_model::v1::Hyperlink {
                        id: id(),
                        target: fixture_link_target(),
                        tooltip: None,
                        inlines: vec![
                            InlineNode::BookmarkStart(BookmarkStart {
                                id: id(),
                                bookmark: nested,
                            }),
                            fixture_run(id(), "linked"),
                            InlineNode::BookmarkEnd(BookmarkEnd {
                                id: id(),
                                bookmark: nested,
                            }),
                        ],
                        history: false,
                    })),
                ],
            )],
            definitions,
        )
        .expect("a valid bookmark document");

        let d = crate::tests::wasm_document(document);
        let rows = d.reference_targets("bookmark");
        assert!(
            rows.iter().any(|row| row.contains("BesideTheLink")),
            "the fixture's top-level bookmark must be offered, or this guard cannot \
             tell a missing descent from a broken picker: {rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("InsideTheLink")),
            "a bookmark an author placed over linked text is not offered as a \
             cross-reference target, so it is in the document and unreachable from \
             the product: {rows:?}"
        );
    }
}
