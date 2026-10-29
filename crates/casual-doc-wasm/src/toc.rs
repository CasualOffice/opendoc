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
// Their own lines (rustfmt is set to Preserve; a shared `use` list is where
// parallel lanes collide).
use casual_doc_command::references::{TocOptions, TocSource, TocStyle};
use casual_doc_layout::cascade::StyleCascade;
use casual_doc_layout::paginate::first_page_labels;
use casual_doc_model::NodeId;
use casual_doc_model::v1::TabLeader;
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

// ---- Settings, removal, Add text, table of figures (`docs/153` RM-25) -------

/// The settings dialog's answers, as the host sends them. Every field is
/// optional and defaults to what the editor generated before settings existed,
/// so `{}` is the old table.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TocSettings {
    /// Levels shown, 1..=9 (outline-level tables).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) levels: Option<u32>,
    /// Show page numbers (default true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) page_numbers: Option<bool>,
    /// Right-align page numbers (default true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) right_align: Option<bool>,
    /// `none`, `dot`, `hyphen` or `underscore` (default `dot`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) leader: Option<String>,
    /// Link each entry to its heading (default false).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) hyperlinks: Option<bool>,
    /// Build from these `[style name, level]` pairs instead of outline levels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) styles: Option<Vec<(String, u8)>>,
    /// Build a table of FIGURES from the captions of this label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) caption: Option<String>,
    /// Keep "Figure 3" in front of each caption's text (default true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) include_label: Option<bool>,
    /// The gallery format: `current`, `simple`, `standard`, `modern`,
    /// `classic` or `online` (default `current`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) format: Option<String>,
}

/// The wire token of a leader. O(1).
fn leader_token(leader: Option<TabLeader>) -> &'static str {
    match leader {
        None => "none",
        Some(TabLeader::Hyphen) => "hyphen",
        Some(TabLeader::Underscore) => "underscore",
        // Middle dot and heavy are Word leaders its dialog does not offer; the
        // dialog shows them as dots, the nearest it has.
        Some(TabLeader::Dot | TabLeader::MiddleDot | TabLeader::Heavy) => "dot",
    }
}

impl TocSettings {
    /// The options these settings ask for. O(styles named).
    ///
    /// # Errors
    /// An unknown leader or format token, a style level outside 1..=9, or an
    /// empty caption label or style list.
    pub(crate) fn options(&self) -> Result<TocOptions, String> {
        let mut options = TocOptions {
            max_level: references::toc_level(self.levels.unwrap_or(0)),
            ..TocOptions::default()
        };
        options.page_numbers = self.page_numbers.unwrap_or(true);
        options.right_align = self.right_align.unwrap_or(true);
        options.hyperlinks = self.hyperlinks.unwrap_or(false);
        options.leader = match self.leader.as_deref().unwrap_or("dot") {
            "none" => None,
            "dot" => Some(TabLeader::Dot),
            "hyphen" => Some(TabLeader::Hyphen),
            "underscore" => Some(TabLeader::Underscore),
            other => return Err(format!("unknown tab leader {other:?}")),
        };
        if let Some(format) = &self.format {
            options.style = TocStyle::parse(format)
                .ok_or_else(|| format!("unknown contents format {format:?}"))?;
        }
        if let Some(label) = &self.caption {
            let label = label.trim();
            if label.is_empty() || label.contains('"') {
                return Err("a table of figures needs a caption label".to_owned());
            }
            options.source = TocSource::Captions {
                label: label.to_owned(),
                include_label: self.include_label.unwrap_or(true),
            };
        } else if let Some(styles) = &self.styles {
            if styles.is_empty() {
                return Err("choose at least one style to build the table from".to_owned());
            }
            for (name, level) in styles {
                if name.trim().is_empty() || name.contains(['"', ',']) {
                    return Err(format!("{name:?} cannot be named in a contents field"));
                }
                if !(1..=references::MAX_TOC_LEVEL).contains(level) {
                    return Err("a contents level is 1 to 9".to_owned());
                }
            }
            options.source = TocSource::Styles(styles.clone());
        }
        Ok(options)
    }

    /// The settings that describe `options` — what the dialog opens on.
    /// O(styles named).
    pub(crate) fn of(options: &TocOptions) -> Self {
        let mut settings = Self {
            levels: Some(u32::from(options.max_level)),
            page_numbers: Some(options.page_numbers),
            right_align: Some(options.right_align),
            leader: Some(leader_token(options.leader).to_owned()),
            hyperlinks: Some(options.hyperlinks),
            format: Some(options.style.token().to_owned()),
            ..Self::default()
        };
        match &options.source {
            TocSource::OutlineLevels => {}
            TocSource::Styles(styles) => settings.styles = Some(styles.clone()),
            TocSource::Captions {
                label,
                include_label,
            } => {
                settings.caption = Some(label.clone());
                settings.include_label = Some(*include_label);
            }
        }
        settings
    }
}

#[wasm_bindgen]
impl WasmDocument {
    /// Inserts a table of contents — or, with `caption` set, a table of
    /// figures — with the settings dialog's answers, a JSON object of
    /// `TocSettings` (`levels`, `pageNumbers`, `rightAlign`, `leader`,
    /// `hyperlinks`, `styles`, `caption`, `includeLabel`, `format`). With the
    /// caret inside an existing table it REPLACES that table, which is how
    /// Word's dialog changes a table's settings.
    ///
    /// One undoable action. **O(document)**, plus O(new bookmarks × document)
    /// the first time a hyperlinked table is generated
    /// (`casual_doc_command::references` contents note).
    ///
    /// # Errors
    /// Unreadable settings, and every refusal of
    /// [`insertTableOfContents`](Self::insert_table_of_contents).
    #[wasm_bindgen(js_name = insertTableOfContentsWith)]
    pub fn insert_table_of_contents_with(
        &mut self,
        node: &str,
        settings: &str,
    ) -> Result<EditResult, JsValue> {
        self.insert_table_of_contents_with_inner(node, settings)
            .map_err(to_js)
    }

    /// The settings of a table of contents or figures, as the JSON
    /// `TocSettings` the dialog opens on: its instruction's switches plus its
    /// entries' tab format. `id` as for
    /// [`updateTableOfContents`](Self::update_table_of_contents); empty means
    /// the only one. **O(body blocks).**
    ///
    /// # Errors
    /// No such table, or several and `id` was empty.
    #[wasm_bindgen(js_name = tableOfContentsSettings)]
    pub fn table_of_contents_settings(&self, id: &str) -> Result<String, JsValue> {
        self.table_of_contents_settings_inner(id).map_err(to_js)
    }

    /// Removes a table of contents or figures as a unit: its field definition
    /// and every entry paragraph, one undoable action. `id` as for
    /// [`updateTableOfContents`](Self::update_table_of_contents).
    ///
    /// # Errors
    /// No such table; several and `id` was empty.
    #[wasm_bindgen(js_name = removeTableOfContents)]
    pub fn remove_table_of_contents(&mut self, id: &str) -> Result<EditResult, JsValue> {
        self.remove_table_of_contents_inner(id).map_err(to_js)
    }

    /// Marks every paragraph the selection `start..end` touches at contents
    /// level `level` (1..=9) without restyling it, or — with `-1` — as "Do not
    /// show in table of contents" (ONLYOFFICE's References ▸ Add text). One
    /// undoable action. **O(document)** for the walks that find them.
    ///
    /// # Errors
    /// A level outside -1 and 1..=9, a paragraph outside the body, or nothing
    /// to change.
    #[wasm_bindgen(js_name = addParagraphsToContents)]
    pub fn add_paragraphs_to_contents(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        level: i32,
    ) -> Result<EditResult, JsValue> {
        self.add_paragraphs_to_contents_inner(start_node, start_offset, end_node, end_offset, level)
            .map_err(to_js)
    }

    /// The contents level the paragraph `node` is listed at (1..=9), or `-1`
    /// when a table of contents would not list it — what checks the Add text
    /// menu's current row. **O(document)** for the paragraph lookup plus
    /// O(style chain); called when the menu opens, never per keystroke.
    #[wasm_bindgen(js_name = contentsLevelAt)]
    #[must_use]
    pub fn contents_level_at(&self, node: &str) -> i32 {
        let Ok(id) = NodeId::from_str(node) else {
            return -1;
        };
        let Some(direct) = casual_doc_edit::paragraph_properties(&self.document, id) else {
            return -1;
        };
        let definitions = self.document.definitions();
        let cascade = StyleCascade::new(definitions);
        casual_doc_layout::fold::heading_level(&direct, definitions, &cascade).map_or(-1, i32::from)
    }
}

impl WasmDocument {
    /// See [`WasmDocument::insert_table_of_contents_with`].
    pub(crate) fn insert_table_of_contents_with_inner(
        &mut self,
        node: &str,
        settings: &str,
    ) -> Result<EditResult, String> {
        let settings = if settings.trim().is_empty() {
            "{}"
        } else {
            settings
        };
        let settings: TocSettings = serde_json::from_str(settings)
            .map_err(|error| format!("unreadable contents settings: {error}"))?;
        let options = settings.options()?;
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
        let placed = references::insert_table_of_contents_with(
            &self.document,
            &facts,
            before,
            &options,
            tab,
            &mut || ids.next_id().ok(),
        )?;
        drop(facts);
        self.apply_action_caret_as(placed.operations, placed.caret, HistoryKind::FieldChange)
    }

    /// The field-range id `id` names, or `None` for "the only one". O(1).
    fn toc_field_id(id: &str) -> Result<Option<FieldRangeId>, String> {
        if id.is_empty() {
            return Ok(None);
        }
        let node = NodeId::from_str(id).map_err(|_| "invalid field range id".to_owned())?;
        Ok(Some(FieldRangeId::new(node)))
    }

    /// See [`WasmDocument::table_of_contents_settings`].
    pub(crate) fn table_of_contents_settings_inner(&self, id: &str) -> Result<String, String> {
        let options = references::toc_options(&self.document, Self::toc_field_id(id)?)?;
        serde_json::to_string(&TocSettings::of(&options)).map_err(|error| error.to_string())
    }

    /// See [`WasmDocument::remove_table_of_contents`].
    pub(crate) fn remove_table_of_contents_inner(
        &mut self,
        id: &str,
    ) -> Result<EditResult, String> {
        let operations =
            references::remove_table_of_contents(&self.document, Self::toc_field_id(id)?)?;
        self.apply_action_as(operations, HistoryKind::FieldChange)
    }

    /// See [`WasmDocument::add_paragraphs_to_contents`].
    pub(crate) fn add_paragraphs_to_contents_inner(
        &mut self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
        level: i32,
    ) -> Result<EditResult, String> {
        let level = match level {
            -1 => None,
            1..=9 => Some(level as u8),
            _ => return Err("a contents level is 1 to 9, or -1 for do not show".to_owned()),
        };
        let (start, end) = self.order_endpoints(start_node, start_offset, end_node, end_offset)?;
        let paragraphs = self.paragraphs_in_selection(start, end);
        let facts = EditorFacts::with_pages(self.document.definitions(), HashMap::new());
        let operations =
            references::add_paragraphs_to_contents(&self.document, &facts, &paragraphs, level)?;
        drop(facts);
        if operations.is_empty() {
            return Err(casual_doc_edit::refused!(
                "toc.add-text-unchanged",
                "This paragraph is already listed that way."
            )
            .to_owned());
        }
        self.apply_action_as(operations, HistoryKind::ParagraphFormatting)
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

    // ---- Settings, removal, Add text, table of figures (RM-25) -------------

    /// Fixture ids in a namespace the editor's own minting never reaches.
    fn fixture_id(n: u64) -> NodeId {
        NodeId::from_parts(91, n).expect("non-zero")
    }

    fn fixture_run(id: u64, text: &str) -> InlineNode {
        InlineNode::Run(casual_doc_model::v1::Run {
            id: fixture_id(id),
            properties: casual_doc_model::v1::RunProperties::default().into(),
            text: text.to_owned(),
        })
    }

    fn fixture_paragraph(
        id: u64,
        style: Option<casual_doc_model::v1::StyleId>,
        inlines: Vec<InlineNode>,
    ) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id: fixture_id(id),
            properties: casual_doc_model::v1::ParagraphProperties {
                style_ref: style,
                ..casual_doc_model::v1::ParagraphProperties::default()
            }
            .into(),
            inlines,
        })
    }

    /// A caption paragraph: `"{label} "`, a `SEQ {label}` field showing
    /// `number`, then `": {text}"` — the shape Insert Caption writes.
    fn fixture_caption(id: u64, label: &str, number: &str, text: &str) -> BlockNode {
        fixture_paragraph(
            id,
            None,
            vec![
                fixture_run(id + 1, &format!("{label} ")),
                InlineNode::Field(Box::new(casual_doc_model::v1::Field {
                    id: fixture_id(id + 2),
                    instruction: format!("SEQ {label} \\* ARABIC"),
                    kind: casual_doc_model::v1::FieldKind::Seq {
                        name: label.to_owned(),
                    },
                    inlines: vec![fixture_run(id + 3, number)],
                    form: None,
                    update: casual_doc_model::v1::FieldUpdateState::default(),
                })),
                fixture_run(id + 4, &format!(": {text}")),
            ],
        )
    }

    /// A plain paragraph style named `name`.
    fn fixture_style(name: &str) -> casual_doc_model::v1::Style {
        casual_doc_model::v1::Style {
            kind: casual_doc_model::v1::StyleKind::Paragraph,
            is_default: false,
            name: Some(name.to_owned()),
            aliases: None,
            based_on: None,
            next: None,
            link: None,
            hidden: false,
            ui_priority: None,
            semi_hidden: false,
            unhide_when_used: false,
            q_format: false,
            locked: false,
            paragraph: None,
            run: None,
            table: None,
            table_row: None,
            table_cell: None,
            conditional: Vec::new(),
            custom_style: false,
        }
    }

    /// The paragraph style named `name`, as the document now defines it.
    fn style_named(d: &WasmDocument, name: &str) -> casual_doc_model::v1::Style {
        d.document
            .definitions()
            .styles
            .iter()
            .find(|(_, style)| style.name.as_deref() == Some(name))
            .map(|(_, style)| style.clone())
            .unwrap_or_else(|| panic!("no style named {name:?}"))
    }

    /// The first body block's paragraph.
    fn first_paragraph(d: &WasmDocument) -> &Paragraph {
        match d.document.body().first() {
            Some(BlockNode::Paragraph(paragraph)) => paragraph,
            _ => panic!("the body starts with a paragraph"),
        }
    }

    /// The contents field's instruction, from the facade's own accessor.
    fn instruction(d: &WasmDocument) -> String {
        d.field_range_spans()[0]
            .split('\t')
            .nth(1)
            .expect("an instruction")
            .to_owned()
    }

    /// Mutation: make `references::toc_options` skip `entry_format_of`, and the
    /// rebuilt entries carry the default dot leader — the update forgets the
    /// leader the author chose.
    #[test]
    fn the_settings_dialog_choices_survive_an_update_and_read_back() {
        let mut d = wasm_document(heading_document(2, 2));
        let anchor = first_body_paragraph(&d);
        d.insert_table_of_contents_with_inner(
            &anchor,
            r#"{"leader":"hyphen","levels":2,"format":"standard"}"#,
        )
        .expect("insert with settings");
        let leader = |d: &WasmDocument| {
            first_paragraph(d)
                .properties
                .get()
                .tabs
                .first()
                .map(|stop| (stop.alignment, stop.leader))
        };
        assert_eq!(
            leader(&d),
            Some((
                casual_doc_model::v1::TabAlignment::End,
                Some(TabLeader::Hyphen)
            )),
            "the entry's page number sits on a right tab with the chosen leader"
        );
        assert!(
            instruction(&d).contains("\\o \"1-2\""),
            "{}",
            instruction(&d)
        );
        let read: TocSettings =
            serde_json::from_str(&d.table_of_contents_settings_inner("").expect("settings"))
                .expect("json");
        assert_eq!(read.leader.as_deref(), Some("hyphen"));
        assert_eq!(read.levels, Some(2));
        assert_eq!(read.right_align, Some(true));

        d.update_table_of_contents_inner("", "entire")
            .expect("update entire");
        assert_eq!(
            leader(&d),
            Some((
                casual_doc_model::v1::TabAlignment::End,
                Some(TabLeader::Hyphen)
            )),
            "Update entire keeps the leader, read back from the entries"
        );

        // The Standard format: the top level is bold 14 pt, the second 13 pt.
        let top = style_named(&d, "TOC 1").run.expect("a run format");
        assert_eq!((top.bold, top.size_half_points), (Some(true), Some(28)));
        let second = style_named(&d, "TOC 2").run.expect("a run format");
        assert_eq!(second.size_half_points, Some(26));
    }

    /// Mutation: always push the Tab in `toc_blocks` — the entry then reads
    /// "Chapter 0\t" and the first assertion goes red.
    #[test]
    fn page_numbers_off_and_left_aligned_numbers_are_honoured() {
        let mut d = wasm_document(heading_document(2, 1));
        let anchor = first_body_paragraph(&d);
        d.insert_table_of_contents_with_inner(&anchor, r#"{"pageNumbers":false}"#)
            .expect("insert");
        assert_eq!(
            &body_lines(&d)[..2],
            ["Chapter 0", "Chapter 1"],
            "no tab and no number"
        );
        assert!(instruction(&d).contains("\\n"), "{}", instruction(&d));
        d.update_table_of_contents_inner("", "pageNumbers")
            .expect("a table with no numbers rebuilds rather than refusing");
        assert_eq!(&body_lines(&d)[..2], ["Chapter 0", "Chapter 1"]);

        let mut d = wasm_document(heading_document(1, 1));
        let anchor = first_body_paragraph(&d);
        d.insert_table_of_contents_with_inner(&anchor, r#"{"rightAlign":false}"#)
            .expect("insert");
        assert!(body_lines(&d)[0].starts_with("Chapter 0\t"));
        assert!(
            first_paragraph(&d).properties.get().tabs.is_empty(),
            "no right-aligned stop: the number follows the text"
        );
    }

    /// Mutation: in `toc_entries`, skip `toc_bookmark_in` so every entry looks
    /// unbookmarked — the update then plants a second bookmark per heading and
    /// the count assertion goes red.
    #[test]
    fn hyperlinked_entries_point_at_a_bookmark_on_their_heading_and_an_update_reuses_it() {
        let mut d = wasm_document(heading_document(2, 1));
        let anchor = first_body_paragraph(&d);
        d.insert_table_of_contents_with_inner(&anchor, r#"{"hyperlinks":true}"#)
            .expect("insert");
        assert!(instruction(&d).contains("\\h"), "{}", instruction(&d));
        let link = first_paragraph(&d)
            .inlines
            .iter()
            .find_map(|inline| match inline {
                InlineNode::Hyperlink(link) => Some(link.clone()),
                _ => None,
            })
            .expect("the entry is a hyperlink");
        let casual_doc_model::v1::HyperlinkTarget::Internal(target) = &link.target else {
            panic!("an internal link");
        };
        assert!(target.anchor.starts_with("_Toc"), "{}", target.anchor);
        let heading = NodeId::from_str(&paragraph_with_text(&d, "Chapter 0")).expect("id");
        let marked = d.document.body().iter().any(|block| match block {
            BlockNode::Paragraph(paragraph) if paragraph.id == heading => {
                paragraph.inlines.iter().any(|inline| match inline {
                    InlineNode::BookmarkStart(marker) => d
                        .document
                        .definitions()
                        .bookmarks
                        .get(&marker.bookmark)
                        .is_some_and(|bookmark| bookmark.name == target.anchor),
                    _ => false,
                })
            }
            _ => false,
        });
        assert!(marked, "the heading carries the bookmark the entry names");
        assert!(body_lines(&d)[0].starts_with("Chapter 0\t"));

        let planted = d.document.definitions().bookmarks.len();
        assert_eq!(planted, 2, "one bookmark per heading");
        d.update_table_of_contents_inner("", "entire")
            .expect("update");
        assert_eq!(
            d.document.definitions().bookmarks.len(),
            planted,
            "an update reuses each heading's bookmark"
        );
        d.update_table_of_contents_inner("", "pageNumbers")
            .expect("page numbers inside a link are renumbered");
        assert_eq!(covered(&d).len(), 2);
    }

    /// Mutation: make `remove_table_of_contents` return `DeleteBlocks` of the
    /// first entry only — the field definition stays and the first assertion
    /// goes red.
    #[test]
    fn removing_a_table_of_contents_takes_the_field_and_every_entry_as_one_step() {
        let mut d = wasm_document(heading_document(3, 1));
        let original = body_lines(&d);
        let anchor = first_body_paragraph(&d);
        d.insert_table_of_contents_inner(&anchor, 3)
            .expect("insert");
        d.remove_table_of_contents_inner("").expect("remove");
        assert!(d.field_range_entries().is_empty(), "the field is gone");
        assert_eq!(body_lines(&d), original, "and so is every entry");
        d.undo().expect("undo");
        assert_eq!(covered(&d).len(), 3, "removal is one undoable step");
        let none = wasm_document(heading_document(1, 1))
            .remove_table_of_contents_inner("")
            .expect_err("nothing to remove");
        assert_eq!(
            casual_doc_edit::refusal::split(&none).1,
            Some("toc.none-to-remove")
        );
    }

    /// Mutation: in `add_paragraphs_to_contents`, return `None` instead of the
    /// body-text level 9 for "Do not show" — the heading-styled paragraph keeps
    /// listing at level 1 and the last assertion goes red.
    #[test]
    fn add_text_lists_a_paragraph_without_restyling_it_and_do_not_show_overrides_a_style() {
        let mut d = wasm_document(heading_document(1, 2));
        let body = paragraph_with_text(&d, "Body 0.0");
        assert_eq!(d.contents_level_at(&body), -1);
        d.add_paragraphs_to_contents_inner(&body, 0, &body, 0, 2)
            .expect("add at level 2");
        assert_eq!(d.contents_level_at(&body), 2);
        let node = NodeId::from_str(&body).expect("id");
        assert_eq!(
            casual_doc_edit::paragraph_properties(&d.document, node).and_then(|p| p.style_ref),
            None,
            "the paragraph is not restyled"
        );
        let anchor = first_body_paragraph(&d);
        d.insert_table_of_contents_inner(&anchor, 3)
            .expect("insert");
        assert!(
            body_lines(&d)[1].starts_with("Body 0.0\t"),
            "the table lists it: {:?}",
            body_lines(&d)
        );
        assert!(
            d.add_paragraphs_to_contents_inner(&body, 0, &body, 0, 2)
                .expect_err("unchanged")
                .contains("already")
        );

        // A paragraph whose STYLE makes it a heading is kept out by body text.
        let heading_style = casual_doc_model::v1::StyleId::new(fixture_id(50));
        let mut definitions = casual_doc_model::v1::Definitions::default();
        definitions
            .styles
            .insert(heading_style, fixture_style("Heading 1"));
        let document = Document::new(
            fixture_id(1),
            vec![
                fixture_paragraph(2, Some(heading_style), vec![fixture_run(3, "Styled")]),
                fixture_paragraph(4, None, vec![fixture_run(5, "Text")]),
            ],
            definitions,
        )
        .expect("a valid document");
        let mut d = wasm_document(document);
        let styled = fixture_id(2).to_string();
        assert_eq!(d.contents_level_at(&styled), 1);
        d.add_paragraphs_to_contents_inner(&styled, 0, &styled, 0, -1)
            .expect("do not show");
        assert_eq!(
            casual_doc_edit::paragraph_properties(&d.document, fixture_id(2))
                .and_then(|p| p.outline_level),
            Some(9),
            "body text, written explicitly because the style says heading"
        );
        assert_eq!(d.contents_level_at(&styled), -1);
    }

    /// Mutation: make `TocOptions::from_instruction` ignore `\c` — the update
    /// then reads the table as an outline table, finds no headings and refuses.
    #[test]
    fn a_table_of_figures_lists_the_captions_of_its_label_and_updates_from_them() {
        let document = Document::new(
            fixture_id(1),
            vec![
                fixture_paragraph(2, None, vec![fixture_run(3, "Intro")]),
                fixture_caption(10, "Figure", "1", "Revenue"),
                fixture_caption(20, "Table", "1", "Rates"),
                fixture_caption(30, "Figure", "2", "Costs"),
            ],
            casual_doc_model::v1::Definitions::default(),
        )
        .expect("a valid document");
        let mut d = wasm_document(document);
        let intro = fixture_id(2).to_string();
        d.insert_table_of_contents_with_inner(&intro, r#"{"caption":"Figure"}"#)
            .expect("insert a table of figures");
        let lines = body_lines(&d);
        assert!(lines[0].starts_with("Figure 1: Revenue\t"), "{lines:?}");
        assert!(lines[1].starts_with("Figure 2: Costs\t"), "{lines:?}");
        assert_eq!(lines[2], "Intro", "the Table caption is not listed");
        assert!(
            instruction(&d).contains("\\c \"Figure\""),
            "{}",
            instruction(&d)
        );
        assert!(
            first_paragraph(&d)
                .properties
                .get()
                .style_ref
                .is_some_and(|id| d
                    .document
                    .definitions()
                    .styles
                    .get(&id)
                    .and_then(|s| s.name.as_deref())
                    == Some("table of figures")),
            "entries carry Word's table-of-figures style"
        );
        d.update_table_of_contents_inner("", "entire")
            .expect("an update rebuilds from the captions");
        assert!(body_lines(&d)[1].starts_with("Figure 2: Costs\t"));

        d.remove_table_of_contents_inner("").expect("remove");
        d.insert_table_of_contents_with_inner(
            &intro,
            r#"{"caption":"Figure","includeLabel":false}"#,
        )
        .expect("caption text only");
        let lines = body_lines(&d);
        assert!(lines[0].starts_with("Revenue\t"), "{lines:?}");
        assert!(lines[1].starts_with("Costs\t"), "{lines:?}");
    }

    /// Mutation: drop the `\t` arm in `TocOptions::from_instruction` — the
    /// update reads an outline table, finds no headings and refuses.
    #[test]
    fn a_table_built_from_chosen_styles_lists_them_at_their_levels_and_keeps_them() {
        let callout = casual_doc_model::v1::StyleId::new(fixture_id(60));
        let mut definitions = casual_doc_model::v1::Definitions::default();
        definitions.styles.insert(callout, fixture_style("Callout"));
        let document = Document::new(
            fixture_id(1),
            vec![
                fixture_paragraph(2, None, vec![fixture_run(3, "Intro")]),
                fixture_paragraph(4, Some(callout), vec![fixture_run(5, "Key point")]),
            ],
            definitions,
        )
        .expect("a valid document");
        let mut d = wasm_document(document);
        let intro = fixture_id(2).to_string();
        d.insert_table_of_contents_with_inner(&intro, r#"{"styles":[["Callout",2]]}"#)
            .expect("insert from styles");
        assert!(body_lines(&d)[0].starts_with("Key point\t"));
        assert!(
            first_paragraph(&d)
                .properties
                .get()
                .style_ref
                .is_some_and(|id| d
                    .document
                    .definitions()
                    .styles
                    .get(&id)
                    .and_then(|s| s.name.as_deref())
                    == Some("TOC 2")),
            "listed at the level its style is paired with"
        );
        assert!(
            instruction(&d).contains("\\t \"Callout,2\""),
            "{}",
            instruction(&d)
        );
        d.update_table_of_contents_inner("", "entire")
            .expect("the update keeps building from the styles");
        assert!(body_lines(&d)[0].starts_with("Key point\t"));
        assert!(
            d.insert_table_of_contents_with_inner(&intro, r#"{"styles":[]}"#)
                .is_err(),
            "an empty style list is refused rather than read as outline levels"
        );
    }

    /// The settings dialog run with the caret IN a table replaces it rather
    /// than nesting a second field inside the first. Mutation: drop the
    /// `contents_field_covering` branch — the insert lands inside the field and
    /// is refused (or nests), and the one-field assertion goes red.
    #[test]
    fn the_settings_dialog_on_an_existing_table_replaces_it() {
        let mut d = wasm_document(heading_document(2, 1));
        let anchor = first_body_paragraph(&d);
        d.insert_table_of_contents_inner(&anchor, 3)
            .expect("insert");
        let entry = first_body_paragraph(&d);
        d.insert_table_of_contents_with_inner(&entry, r#"{"leader":"none"}"#)
            .expect("replace");
        assert_eq!(d.field_range_entries().len(), 1, "still one table");
        assert_eq!(covered(&d).len(), 2);
        assert_eq!(
            first_paragraph(&d)
                .properties
                .get()
                .tabs
                .first()
                .map(|stop| stop.leader),
            Some(None)
        );
    }
}
