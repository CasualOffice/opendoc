//! Table of contents — generating one, and the two updates Word offers.
//!
//! Word and ONLYOFFICE both split updating a table of contents exactly one way:
//! **update page numbers only**, which keeps the entries and re-reads the
//! pagination, and **update the entire table**, which rebuilds the entries from
//! the heading outline. The lowering of all three gestures is
//! `casual_doc_command::references` (`docs/181` §12, lane references), shared
//! with agents; this module is the editor's glue over it: it resolves the caret,
//! lends the command what the editor's layout knows ([`EditorFacts`]: the heading
//! cascade and the page each heading is laid out on), and applies the result as
//! one undoable action.
//!
//! # What makes it a field rather than frozen text
//!
//! A generated table of contents is a real paragraph-spanning complex field
//! (`docs/128`): a `FieldRangeStart`/`FieldRangeEnd` marker pair delimiting the
//! entry paragraphs, with the instruction in `Definitions::field_ranges`. That is
//! what lets [`WasmDocument::field_range_spans`](crate::WasmDocument::field_range_spans)
//! answer "is this paragraph inside the contents field", what survives a DOCX
//! round trip as a field Word will itself offer to update, and what makes
//! "update this table" a question with an answer. Entry text, level, page number
//! and the outline it is derived from all come from the document — the heading
//! cascade and the real pagination — never from what the caller typed. Where it
//! deliberately differs from Word (no `\h`, no `_Toc` bookmarks) is recorded on
//! the command module.
//!
//! # Complexity
//!
//! Generation and both updates are **O(document)**: one walk for the headings,
//! one pass over the paginated layout for the page labels. That is legitimate —
//! a table of contents is a statement about the whole document — and it is why
//! none of this is on a keystroke path and why
//! `generating_a_table_of_contents_costs_work_proportional_to_the_document`
//! guards it by DOUBLING the heading count rather than by timing.

use casual_doc_command::references::{self, LayoutFacts, TocUpdate};
use casual_doc_layout::cascade::StyleCascade;
use casual_doc_layout::paginate::first_page_labels;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{Definitions, Document, FieldRangeId, Paragraph};
use std::collections::HashMap;
use std::str::FromStr;
use wasm_bindgen::prelude::*;

use crate::window::BodyLayout;
use crate::{EditResult, HistoryKind, WasmDocument, to_js};

/// What the editor's layout knows, lent to the shared lowerings in
/// `casual_doc_command::references`: heading levels through the style cascade,
/// and page labels for the nodes a gesture asks about.
///
/// It borrows only the document's definitions, so the gesture can still mint
/// ids while the facts are alive.
pub(crate) struct EditorFacts<'a> {
    definitions: &'a Definitions,
    cascade: StyleCascade<'a>,
    /// The page label of each node the gesture may ask about, or the reason
    /// the layout cannot answer for the whole document.
    pages: Result<HashMap<NodeId, String>, String>,
}

impl<'a> EditorFacts<'a> {
    /// Heading levels and the given page labels. O(1); the cascade resolves
    /// lazily.
    pub(crate) fn with_pages(definitions: &'a Definitions, pages: HashMap<NodeId, String>) -> Self {
        Self {
            definitions,
            cascade: StyleCascade::new(definitions),
            pages: Ok(pages),
        }
    }
}

impl LayoutFacts for EditorFacts<'_> {
    fn heading_level(&self, paragraph: &Paragraph) -> Option<u8> {
        casual_doc_layout::fold::heading_level(
            paragraph.properties.get(),
            self.definitions,
            &self.cascade,
        )
    }

    fn page_label(&self, node: NodeId) -> Option<String> {
        self.pages
            .as_ref()
            .ok()
            .and_then(|pages| pages.get(&node).cloned())
    }

    fn whole_pagination(&self) -> Result<(), String> {
        self.pages.as_ref().map(|_| ()).map_err(Clone::clone)
    }
}

impl WasmDocument {
    /// The facts a table of contents needs: the page label of every node the
    /// paginated layout placed (a heading that splits across a break is listed
    /// on the page it starts on, as Word lists it), or — when the body is laid
    /// out one window at a time — the reason the pages a table of contents must
    /// read are not all resident. Inventing numbers for the ones that are not is
    /// worse than saying so.
    ///
    /// An associated function over the two fields it reads, so the facts borrow
    /// the document alone and the gesture can mint ids while they are alive.
    ///
    /// **O(placed fragments)**, one pass over the layout.
    fn toc_facts<'a>(document: &'a Document, layout: &BodyLayout) -> EditorFacts<'a> {
        let definitions = document.definitions();
        if layout.is_windowed() {
            return EditorFacts {
                definitions,
                cascade: StyleCascade::new(definitions),
                pages: Err(
                    "a table of contents needs the whole document paginated, and this one is \
                     laid out a window at a time"
                        .to_owned(),
                ),
            };
        }
        let pages = first_page_labels(layout.resident(), &definitions.sections);
        EditorFacts::with_pages(definitions, pages)
    }

    /// The right-aligned page-number tab's position: the content width of the
    /// document's default page. O(1).
    fn toc_tab_position(&self) -> i32 {
        let config = &self.default_config;
        config.page_size.width.raw() - config.margin_start.raw() - config.margin_end.raw()
    }
}

#[wasm_bindgen]
impl WasmDocument {
    /// Inserts a table of contents at the caret's top-level block, listing every
    /// heading down to `max_level` (1..=9; Word's "Show levels", defaulting to 3
    /// when `0` is passed).
    ///
    /// Each entry is a `TOC n`-styled paragraph holding the heading's own text, a
    /// right-aligned dot-leader tab, and the page label that page actually
    /// prints. The whole run of entries is one paragraph-spanning complex field,
    /// so it is a live field an update can find again — not frozen text.
    ///
    /// The caret lands at the start of the first entry. One undoable action under
    /// `HistoryKind::FieldChange`.
    ///
    /// **Complexity: O(document)** — see the module note. Never call this from a
    /// keystroke path.
    ///
    /// # Errors
    /// When the caret is not in the document body, when the document has no
    /// headings (there is nothing to list, and an empty table is not an answer),
    /// or when the body is laid out one window at a time.
    #[wasm_bindgen(js_name = insertTableOfContents)]
    pub fn insert_table_of_contents(
        &mut self,
        node: &str,
        max_level: u32,
    ) -> Result<EditResult, JsValue> {
        self.insert_table_of_contents_inner(node, max_level)
            .map_err(to_js)
    }

    /// Updates a table of contents. `mode` is `"pageNumbers"` (re-read the
    /// pagination, keep the entries) or `"entire"` (rebuild the entries from the
    /// heading outline) — the split Word and ONLYOFFICE both offer.
    ///
    /// `id` is the 32-hex field-range id from
    /// [`fieldRangeSpans`](Self::field_range_spans); passing an empty string
    /// means "the document's only table of contents", which is refused rather
    /// than guessed when there is not exactly one.
    ///
    /// One undoable action under `HistoryKind::FieldChange`; the caret does not
    /// move, because an update is not a place the user asked to go.
    ///
    /// **Complexity: O(document)** — see the module note.
    ///
    /// # Errors
    /// When `id` names no field range, when no (or more than one) contents field
    /// exists and `id` was empty, when `mode` is neither of the two, when the
    /// range's markers are not at body top level, or — for `"pageNumbers"` — when
    /// the number of entries no longer matches the number of headings, which is
    /// the case that needs a whole rebuild rather than new numbers on the wrong
    /// rows.
    #[wasm_bindgen(js_name = updateTableOfContents)]
    pub fn update_table_of_contents(
        &mut self,
        id: &str,
        mode: &str,
    ) -> Result<EditResult, JsValue> {
        self.update_table_of_contents_inner(id, mode).map_err(to_js)
    }
}

impl WasmDocument {
    /// See [`WasmDocument::insert_table_of_contents`]. Split out so native guards
    /// read the refusal text on the same path as the `#[wasm_bindgen]` boundary
    /// (a `JsValue` cannot be built off-wasm).
    pub(crate) fn insert_table_of_contents_inner(
        &mut self,
        node: &str,
        max_level: u32,
    ) -> Result<EditResult, String> {
        // An id that does not parse names no body block, which is the command's
        // own body-only refusal (as `block_index_of`'s `-1` was).
        let Ok(before) = NodeId::from_str(node) else {
            return Err(casual_doc_edit::refused!(
                "toc.body-only",
                "A table of contents goes in the document body, not in a header, footer or note."
            )
            .to_owned());
        };
        let tab = self.toc_tab_position();
        let facts = Self::toc_facts(&self.document, &self.layout);
        let ids = &mut self.edit_ids;
        let placed = references::insert_table_of_contents(
            &self.document,
            &facts,
            before,
            references::toc_level(max_level),
            tab,
            &mut || ids.next_id().ok(),
        )?;
        drop(facts);
        self.apply_action_caret_as(placed.operations, placed.caret, HistoryKind::FieldChange)
    }

    /// See [`WasmDocument::update_table_of_contents`].
    pub(crate) fn update_table_of_contents_inner(
        &mut self,
        id: &str,
        mode: &str,
    ) -> Result<EditResult, String> {
        let mode = match mode {
            "pageNumbers" => TocUpdate::PageNumbers,
            "entire" => TocUpdate::Entire,
            _ => return Err("update mode must be pageNumbers or entire".to_owned()),
        };
        let field = if id.is_empty() {
            None
        } else {
            let node = NodeId::from_str(id).map_err(|_| "invalid field range id".to_owned())?;
            Some(FieldRangeId::new(node))
        };
        let tab = self.toc_tab_position();
        let facts = Self::toc_facts(&self.document, &self.layout);
        let ids = &mut self.edit_ids;
        let operations = references::update_table_of_contents(
            &self.document,
            &facts,
            field,
            mode,
            tab,
            &mut || ids.next_id().ok(),
        )?;
        drop(facts);
        self.apply_action_as(operations, HistoryKind::FieldChange)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_plain_text;
    use crate::tests::wasm_document;
    use casual_doc_layout::paginate::page_number_labels;
    use casual_doc_model::v1::{BlockNode, InlineNode};

    /// A document of `chapters` level-1 headings, each followed by `filler` body
    /// paragraphs, built by importing real markup so the headings carry a real
    /// `w:outlineLvl` the cascade has to resolve.
    fn heading_document(chapters: usize, filler: usize) -> Document {
        let mut xml = String::from(r#"<w:document xmlns:w="urn:w"><w:body>"#);
        for chapter in 0..chapters {
            xml.push_str(&format!(
                r#"<w:p><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:r><w:t>Chapter {chapter}</w:t></w:r></w:p>"#
            ));
            for line in 0..filler {
                xml.push_str(&format!(
                    r#"<w:p><w:r><w:t>Body {chapter}.{line}</w:t></w:r></w:p>"#
                ));
            }
        }
        xml.push_str("</w:body></w:document>");
        casual_doc_import::import_main_document_xml(
            xml.as_bytes(),
            casual_doc_import::ImportConfig::default(),
        )
        .expect("import")
        .document
    }

    /// The plain text of every body paragraph, in document order.
    fn body_lines(document: &WasmDocument) -> Vec<String> {
        document
            .document
            .body()
            .iter()
            .filter_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(node_plain_text(&paragraph.inlines)),
                _ => None,
            })
            .collect()
    }

    /// The id of the body paragraph whose text is exactly `text`.
    fn paragraph_with_text(document: &WasmDocument, text: &str) -> String {
        document
            .document
            .body()
            .iter()
            .find_map(|block| match block {
                BlockNode::Paragraph(paragraph) if node_plain_text(&paragraph.inlines) == text => {
                    Some(paragraph.id.to_string())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no paragraph reading {text:?}"))
    }

    /// The page label the LAYOUT prints for the page the paragraph reading `text`
    /// is laid out on — read independently of the table of contents, so the guard
    /// cannot be satisfied by the generator agreeing with itself.
    fn page_label_of(document: &WasmDocument, text: &str) -> String {
        let node = NodeId::from_str(&paragraph_with_text(document, text)).expect("node id");
        let layout = document.layout.resident();
        let labels = page_number_labels(layout, &document.document.definitions().sections);
        layout
            .pages
            .iter()
            .position(|page| {
                page.placed
                    .iter()
                    .any(|placed| placed.fragment.node_id() == node)
            })
            .and_then(|index| labels.get(index).cloned())
            .unwrap_or_else(|| panic!("{text:?} is not laid out on any page"))
    }

    /// The id of the first body paragraph.
    fn first_body_paragraph(document: &WasmDocument) -> String {
        match document.document.body().first().expect("a block") {
            BlockNode::Paragraph(paragraph) => paragraph.id.to_string(),
            _ => panic!("paragraph"),
        }
    }

    /// The node ids a field range covers, from the facade's own span accessor.
    fn covered(document: &WasmDocument) -> Vec<String> {
        let spans = document.field_range_spans();
        assert_eq!(spans.len(), 1, "one contents field: {spans:?}");
        spans[0]
            .split('\t')
            .nth(2)
            .expect("the span column")
            .split(' ')
            .map(ToOwned::to_owned)
            .collect()
    }

    #[test]
    fn a_generated_table_of_contents_lists_the_real_headings_and_their_real_pages() {
        // A document tall enough that its headings do NOT all land on page 1, so
        // "the page number came from the pagination" is a claim with content.
        let mut d = wasm_document(heading_document(3, 60));
        let expected: Vec<String> = (0..3)
            .map(|chapter| {
                let text = format!("Chapter {chapter}");
                format!("{text}\t{}", page_label_of(&d, &text))
            })
            .collect();
        assert!(
            expected.iter().any(|line| !line.ends_with("\t1")),
            "the fixture must span more than one page, or the page column proves \
             nothing: {expected:?}"
        );

        let anchor = first_body_paragraph(&d);
        d.insert_table_of_contents_inner(&anchor, 3)
            .expect("insert a table of contents");

        // Read the DOCUMENT back, not the accessor that built it.
        let lines = body_lines(&d);
        assert_eq!(
            &lines[..3],
            &expected[..],
            "one entry per heading: heading text, a tab, then the page label the \
             layout prints for that heading"
        );
        assert_eq!(
            lines[3], "Chapter 0",
            "the headings themselves are untouched and still follow the table"
        );

        // Each entry carries its `TOC n` style, which is the marking the shipped
        // navigation path keys on.
        let styles: Vec<String> = (0..3)
            .map(|index| match &d.document.body()[index] {
                BlockNode::Paragraph(paragraph) => paragraph
                    .properties
                    .get()
                    .style_ref
                    .and_then(|id| d.document.definitions().styles.get(&id))
                    .and_then(|style| style.name.clone())
                    .unwrap_or_default(),
                _ => panic!("paragraph"),
            })
            .collect();
        assert_eq!(styles, ["TOC 1", "TOC 1", "TOC 1"]);

        // It is a FIELD, not frozen text: the facade can find it and say which
        // paragraphs it covers.
        let spans = d.field_range_spans();
        let parts: Vec<&str> = spans[0].split('\t').collect();
        assert!(
            parts[1].contains("TOC") && parts[1].contains("1-3"),
            "the instruction says what it is: {:?}",
            parts[1]
        );
        assert_eq!(
            covered(&d).len(),
            3,
            "the span covers exactly the three entries"
        );

        d.undo().expect("undo");
        assert_eq!(
            body_lines(&d)[0],
            "Chapter 0",
            "generating a table of contents is one undoable action"
        );
        assert!(
            d.field_range_entries().is_empty(),
            "and undo takes the field definition with it"
        );
    }

    #[test]
    fn a_document_with_no_headings_is_refused_rather_than_given_an_empty_table() {
        let mut d = wasm_document(heading_document(0, 3));
        let anchor = first_body_paragraph(&d);
        let err = d
            .insert_table_of_contents_inner(&anchor, 3)
            .expect_err("nothing to list");
        assert!(err.contains("no headings"), "the refusal says why: {err}");
        assert!(
            d.field_range_entries().is_empty(),
            "a refused insert leaves no field behind"
        );
    }

    /// A heading inside a table cell is listed with the page it is on.
    ///
    /// The page map read top-level fragments only, so a heading in a cell had
    /// no page, the entry carried an EMPTY page run, the model refused it as an
    /// invalid field, and the whole table of contents failed to insert
    /// (`InvalidField`) for any document with a heading in a table — found
    /// while moving this lowering into `casual-doc-command` (`docs/181` §12,
    /// lane references).
    #[test]
    fn a_heading_inside_a_table_cell_is_listed_with_its_page() {
        let document = casual_doc_import::import_main_document_xml(
            br#"<w:document xmlns:w="urn:w"><w:body>
                <w:p><w:r><w:t>Preface</w:t></w:r></w:p>
                <w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc>
                  <w:p><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:r><w:t>Boxed heading</w:t></w:r></w:p>
                </w:tc></w:tr></w:tbl>
                <w:p><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:r><w:t>Body heading</w:t></w:r></w:p>
            </w:body></w:document>"#,
            casual_doc_import::ImportConfig::default(),
        )
        .expect("import")
        .document;
        let mut d = wasm_document(document);
        let anchor = first_body_paragraph(&d);
        d.insert_table_of_contents_inner(&anchor, 3)
            .expect("a heading in a table does not stop the table of contents");
        let lines = body_lines(&d);
        assert_eq!(
            &lines[..2],
            ["Boxed heading\t1", "Body heading\t1"],
            "both headings are listed, each with its page"
        );
    }

    #[test]
    fn show_levels_limits_the_table_to_the_levels_asked_for() {
        let document = casual_doc_import::import_main_document_xml(
            br#"<w:document xmlns:w="urn:w"><w:body>
                <w:p><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:r><w:t>Top</w:t></w:r></w:p>
                <w:p><w:pPr><w:outlineLvl w:val="1"/></w:pPr><w:r><w:t>Second</w:t></w:r></w:p>
                <w:p><w:pPr><w:outlineLvl w:val="2"/></w:pPr><w:r><w:t>Third</w:t></w:r></w:p>
            </w:body></w:document>"#,
            casual_doc_import::ImportConfig::default(),
        )
        .expect("import")
        .document;
        let mut d = wasm_document(document);
        let anchor = first_body_paragraph(&d);
        d.insert_table_of_contents_inner(&anchor, 2)
            .expect("insert two levels");
        let lines = body_lines(&d);
        assert!(
            lines[0].starts_with("Top\t") && lines[1].starts_with("Second\t"),
            "levels 1 and 2 are listed: {lines:?}"
        );
        assert_eq!(
            lines[2], "Top",
            "the level-3 heading was NOT listed; the body starts right after two \
             entries: {lines:?}"
        );
        assert_eq!(covered(&d).len(), 2);
    }

    #[test]
    fn updating_page_numbers_rereads_the_pagination_and_keeps_the_entries() {
        let mut d = wasm_document(heading_document(2, 60));
        let anchor = first_body_paragraph(&d);
        d.insert_table_of_contents_inner(&anchor, 3)
            .expect("insert");

        // Break the numbers in the DOCUMENT, so a passing update has to have
        // rewritten them rather than left correct ones alone.
        for index in 0..2 {
            if let BlockNode::Paragraph(paragraph) = &mut d.document.body_mut()[index]
                && let Some(InlineNode::Run(run)) = paragraph
                    .inlines
                    .iter_mut()
                    .filter_map(|inline| match inline {
                        InlineNode::Run(_) => Some(inline),
                        _ => None,
                    })
                    .next_back()
            {
                run.text = "999".to_owned();
            }
        }
        assert_eq!(
            body_lines(&d)[0],
            "Chapter 0\t999",
            "the fixture now carries a wrong number"
        );

        let expected = format!("Chapter 1\t{}", page_label_of(&d, "Chapter 1"));
        d.update_table_of_contents_inner("", "pageNumbers")
            .expect("update page numbers");
        let lines = body_lines(&d);
        assert_eq!(
            lines[1], expected,
            "the page number was re-read from the pagination: {lines:?}"
        );
        assert!(
            lines[0].starts_with("Chapter 0\t") && !lines[0].ends_with("999"),
            "the entry TEXT is untouched and the number is not: {lines:?}"
        );
        assert_eq!(
            covered(&d).len(),
            2,
            "the field survived its own update, markers and all"
        );
    }

    #[test]
    fn updating_the_entire_table_rebuilds_the_entries_from_the_headings() {
        let mut d = wasm_document(heading_document(2, 2));
        let anchor = first_body_paragraph(&d);
        d.insert_table_of_contents_inner(&anchor, 3)
            .expect("insert");

        // The author renames a heading through the ordinary editing path.
        let heading = paragraph_with_text(&d, "Chapter 1");
        d.insert_text(&heading, 0, "Revised ".to_owned())
            .expect("rename the heading");
        assert!(
            body_lines(&d)[1].starts_with("Chapter 1\t"),
            "the table is now stale, which is the case under test"
        );

        d.update_table_of_contents_inner("", "entire")
            .expect("update the whole table");
        let lines = body_lines(&d);
        assert!(
            lines[1].starts_with("Revised Chapter 1\t"),
            "the rebuilt table carries the heading's current text: {lines:?}"
        );
        assert_eq!(covered(&d).len(), 2, "and is still one field range");

        d.undo().expect("undo the update");
        assert!(
            body_lines(&d)[1].starts_with("Chapter 1\t"),
            "an update is one undoable action"
        );
    }

    #[test]
    fn page_numbers_only_refuses_when_the_outline_no_longer_matches() {
        let mut d = wasm_document(heading_document(2, 1));
        let anchor = first_body_paragraph(&d);
        d.insert_table_of_contents_inner(&anchor, 3)
            .expect("insert");
        // One heading stops being a heading: two entries, one heading.
        let heading = d.document.body().len() - 2;
        if let BlockNode::Paragraph(paragraph) = &mut d.document.body_mut()[heading] {
            let mut props = paragraph.properties.get().clone();
            props.outline_level = None;
            paragraph.properties = props.into();
        }
        let before = body_lines(&d);
        let err = d
            .update_table_of_contents_inner("", "pageNumbers")
            .expect_err("the rows no longer line up");
        assert!(
            err.contains("update the entire table"),
            "the refusal names the fix: {err}"
        );
        assert_eq!(body_lines(&d), before, "and changes nothing");
    }

    #[test]
    fn an_unknown_mode_and_an_absent_table_are_refused_by_name() {
        let mut d = wasm_document(heading_document(1, 1));
        assert!(
            d.update_table_of_contents_inner("", "sideways")
                .expect_err("unknown mode")
                .contains("pageNumbers or entire")
        );
        let absent = d
            .update_table_of_contents_inner("", "entire")
            .expect_err("no table");
        assert_eq!(
            casual_doc_edit::refusal::split(&absent).1,
            Some("toc.none"),
            "the refusal carries a stable code a host can translate: {absent}"
        );
        assert!(
            absent.contains("no table of contents"),
            "and an English fallback that says which document state it means: {absent}"
        );
    }

    /// Generating a table of contents is legitimately O(document) — it is a
    /// statement about the whole document. It must not be O(document squared),
    /// which is what a by-id lookup per heading makes it (`docs/116`).
    ///
    /// A ratio, not a clock: doubling the headings must roughly double the blocks
    /// the paragraph lookups examine, not quadruple them. Zero at both sizes is
    /// the strongest outcome, not a hole — the walk carries each paragraph, so it
    /// resolves no id at all; reintroducing a per-heading `paragraph_properties`
    /// makes the count both nonzero and quadratic, which fails the ratio arm.
    #[test]
    fn generating_a_table_of_contents_costs_work_proportional_to_the_document() {
        let visits = |chapters: usize| {
            let mut d = wasm_document(heading_document(chapters, 2));
            let anchor = first_body_paragraph(&d);
            casual_doc_edit::reset_block_visits();
            d.insert_table_of_contents_inner(&anchor, 3)
                .expect("insert a table of contents");
            casual_doc_edit::block_visits()
        };
        let small_n = 60;
        let small = visits(small_n);
        let large = visits(small_n * 2);
        assert!(
            large < small * 3 || (small == 0 && large == 0),
            "contents generation must roughly double, not quadruple: {small} block \
             visits at {small_n} headings and {large} at {}",
            small_n * 2
        );
    }
}
