//! Table of contents — generating one, and the two updates Word offers.
//!
//! Word and ONLYOFFICE both split updating a table of contents exactly one way:
//! **update page numbers only**, which keeps the entries and re-reads the
//! pagination, and **update the entire table**, which rebuilds the entries from
//! the heading outline. This module is that split, plus the insertion that makes
//! a table exist in the first place.
//!
//! # What makes it a field rather than frozen text
//!
//! A generated table of contents here is a real paragraph-spanning complex field
//! (`docs/128`): a `FieldRangeStart`/`FieldRangeEnd` marker pair delimiting the
//! entry paragraphs, with the instruction in `Definitions::field_ranges`. That is
//! what lets [`WasmDocument::field_range_spans`](crate::WasmDocument::field_range_spans)
//! answer "is this paragraph inside the contents field", what survives a DOCX
//! round trip as a field Word will itself offer to update, and what makes
//! "update this table" a question with an answer. Entry text, level, page number
//! and the outline it is derived from all come from the document — the heading
//! cascade and the real pagination — never from what the caller typed.
//!
//! # Where this deliberately differs from Word
//!
//! Word writes `\h` and wraps every entry in a hyperlink to a `_TocNNNN`
//! bookmark it plants in each heading. **We do not**, and the instruction we
//! write says so: it has no `\h`. Planting a bookmark per heading means one
//! `CreateBookmark` per heading, and each of those re-validates the whole
//! document — O(headings x document), which is precisely the quadratic shape the
//! complexity gate here exists to keep out (`docs/116`). Navigation is not lost:
//! `webapp/src/toc_navigation.mjs` already follows an entry by its heading text
//! for the (very common) documents whose author's field carried no `\h` either,
//! and a generated table is marked with the same `TOC 1`..`TOC 9` styles that
//! path keys on.
//!
//! # Complexity
//!
//! Generation and both updates are **O(document)**: one walk for the headings,
//! one pass over the paginated layout for the page labels. That is legitimate —
//! a table of contents is a statement about the whole document — and it is why
//! none of this is on a keystroke path and why
//! `generating_a_table_of_contents_costs_work_proportional_to_the_document`
//! guards it by DOUBLING the heading count rather than by timing.

use casual_doc_edit::{Operation, Pos, body_field_range_blocks};
// Its own line, not folded into a sorted block: a shared `use` list is where
// parallel lanes collide (rustfmt is set to Preserve).
use casual_doc_edit::refused;
use casual_doc_layout::cascade::StyleCascade;
use casual_doc_layout::paginate::page_number_labels;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, FieldKind, FieldRange, FieldRangeEnd, FieldRangeId, FieldRangeStart, Indentation,
    InlineNode, Paragraph, ParagraphProperties, Run, RunProperties, Style, StyleId, StyleKind, Tab,
    TabAlignment, TabLeader, TabStop,
};
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use wasm_bindgen::prelude::*;

use crate::{EditResult, HistoryKind, WasmDocument, node_plain_text, to_js, visit_paragraphs};

/// The deepest heading level a table of contents can show, as Word's dialog
/// offers.
const MAX_TOC_LEVEL: u8 = 9;

/// The default when the caller names no level — Word's own default.
const DEFAULT_TOC_LEVEL: u8 = 3;

/// The indent one contents level adds, in twips (0.15 in), matching the built-in
/// `TOC n` styles Word ships.
const TOC_LEVEL_INDENT_TWIPS: i32 = 220;

/// The name of the paragraph style for contents level `level` (1-based).
///
/// `TOC 1`..`TOC 9` is Word's marking, and it is the marking
/// `webapp/src/toc_navigation.mjs` recognises, so a generated table is navigable
/// by the path that was already shipped for authored ones.
///
/// **Complexity: O(1).**
#[must_use]
fn toc_style_name(level: u8) -> String {
    format!("TOC {level}")
}

/// One heading the contents names.
struct Heading {
    /// 1-based outline level (1 = top).
    level: u8,
    /// The heading's own text, tabs flattened.
    text: String,
    /// The page label the reader sees printed on the page the heading is laid
    /// out on — the same label a `PAGE` field would print there. Empty when the
    /// pagination placed no fragment for the heading, which a whole layout does
    /// not do; it is not silently invented.
    page: String,
}

impl WasmDocument {
    /// Every heading at or above `max_level`, in document order, with its page
    /// label.
    ///
    /// **Complexity: O(document)** — one walk of the body carrying each
    /// paragraph (never a by-id lookup per heading, `docs/116`), plus one pass
    /// over the paginated layout's fragments.
    ///
    /// # Errors
    /// When the body is laid out one window at a time: the pages a table of
    /// contents must read are not all resident, and inventing numbers for the
    /// ones that are not is worse than saying so.
    fn toc_headings(&self, max_level: u8) -> Result<Vec<Heading>, String> {
        if self.layout.is_windowed() {
            return Err(
                "a table of contents needs the whole document paginated, and this one is \
                 laid out a window at a time"
                    .to_owned(),
            );
        }
        let layout = self.layout.resident();
        let labels = page_number_labels(layout, &self.document.definitions().sections);
        // node -> 0-based page index, first placement wins (a heading that splits
        // across a break is listed on the page it starts on, as Word lists it).
        let mut page_of: HashMap<NodeId, usize> = HashMap::new();
        for (index, page) in layout.pages.iter().enumerate() {
            for placed in &page.placed {
                page_of.entry(placed.fragment.node_id()).or_insert(index);
            }
        }

        let cascade = StyleCascade::new(self.document.definitions());
        let mut headings = Vec::new();
        visit_paragraphs(self.document.body(), &mut |paragraph| {
            let Some(level) = self.heading_level_of(paragraph.properties.get(), &cascade) else {
                return;
            };
            if level > max_level {
                return;
            }
            let text = node_plain_text(&paragraph.inlines);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return;
            }
            headings.push(Heading {
                level,
                text: trimmed.replace('\t', " "),
                page: page_of
                    .get(&paragraph.id)
                    .and_then(|index| labels.get(*index))
                    .cloned()
                    .unwrap_or_default(),
            });
        });
        Ok(headings)
    }

    /// The style id for contents level `level`, plus the operation that creates
    /// the style when the document does not already define it.
    ///
    /// **Complexity: O(styles)** per level, and at most nine levels exist, so the
    /// whole pass is O(styles) — bounded by the style registry, not the document.
    fn toc_style(&mut self, level: u8) -> Result<(StyleId, Option<Operation>), String> {
        let name = toc_style_name(level);
        if let Some(id) = self
            .document
            .definitions()
            .styles
            .iter()
            .find(|(_, style)| {
                style.kind == StyleKind::Paragraph && style.name.as_deref() == Some(name.as_str())
            })
            .map(|(id, _)| *id)
        {
            return Ok((id, None));
        }
        let id = StyleId::new(
            self.edit_ids
                .next_id()
                .map_err(|_| "id space exhausted".to_owned())?,
        );
        let style = Style {
            kind: StyleKind::Paragraph,
            is_default: false,
            name: Some(name),
            aliases: None,
            based_on: None,
            next: None,
            link: None,
            hidden: false,
            ui_priority: Some(39),
            semi_hidden: false,
            unhide_when_used: true,
            q_format: false,
            locked: false,
            paragraph: Some(ParagraphProperties {
                indentation: Some(Indentation {
                    start_twips: Some(i32::from(level - 1) * TOC_LEVEL_INDENT_TWIPS),
                    ..Indentation::default()
                }),
                ..ParagraphProperties::default()
            }),
            run: None,
            table: None,
            table_row: None,
            table_cell: None,
            conditional: Vec::new(),
        };
        Ok((
            id,
            Some(Operation::SetStyleDefinition {
                id,
                style: Some(Box::new(style)),
            }),
        ))
    }

    /// The right-aligned dot-leader tab stop an entry's page number sits on, at
    /// the content width of the document's first section — Word's shape for a
    /// contents entry.
    ///
    /// **Complexity: O(1).**
    fn toc_tab_stop(&self) -> TabStop {
        let config = &self.default_config;
        let width =
            (config.page_size.width.raw() - config.margin_start.raw() - config.margin_end.raw())
                .max(0);
        TabStop {
            position_twips: width,
            alignment: TabAlignment::End,
            leader: Some(TabLeader::Dot),
        }
    }

    /// Builds the entry paragraphs for `headings`, carrying the field range's two
    /// markers on the first and last block, and the `SetStyleDefinition`
    /// operations for any `TOC n` style the document does not yet define.
    ///
    /// **Complexity: O(headings)** plus the bounded style pass.
    fn toc_blocks(
        &mut self,
        headings: &[Heading],
        field: FieldRangeId,
    ) -> Result<(Vec<Operation>, Vec<BlockNode>), String> {
        let exhausted = || "id space exhausted".to_owned();
        let mut style_ops = Vec::new();
        let mut styles: BTreeMap<u8, StyleId> = BTreeMap::new();
        for heading in headings {
            if let std::collections::btree_map::Entry::Vacant(slot) = styles.entry(heading.level) {
                let (id, op) = self.toc_style(heading.level)?;
                slot.insert(id);
                style_ops.extend(op);
            }
        }
        let tab_stop = self.toc_tab_stop();

        let mut blocks = Vec::with_capacity(headings.len());
        for (index, heading) in headings.iter().enumerate() {
            let mut inlines = Vec::with_capacity(5);
            if index == 0 {
                inlines.push(InlineNode::FieldRangeStart(FieldRangeStart {
                    id: self.edit_ids.next_id().map_err(|_| exhausted())?,
                    field,
                }));
            }
            inlines.push(InlineNode::Run(Run {
                id: self.edit_ids.next_id().map_err(|_| exhausted())?,
                properties: RunProperties::default().into(),
                text: heading.text.clone(),
            }));
            inlines.push(InlineNode::Tab(Tab {
                id: self.edit_ids.next_id().map_err(|_| exhausted())?,
            }));
            inlines.push(InlineNode::Run(Run {
                id: self.edit_ids.next_id().map_err(|_| exhausted())?,
                properties: RunProperties::default().into(),
                text: heading.page.clone(),
            }));
            if index + 1 == headings.len() {
                inlines.push(InlineNode::FieldRangeEnd(FieldRangeEnd {
                    id: self.edit_ids.next_id().map_err(|_| exhausted())?,
                    field,
                }));
            }
            blocks.push(BlockNode::Paragraph(Paragraph {
                id: self.edit_ids.next_id().map_err(|_| exhausted())?,
                properties: ParagraphProperties {
                    style_ref: styles.get(&heading.level).copied(),
                    tabs: vec![tab_stop],
                    ..ParagraphProperties::default()
                }
                .into(),
                inlines,
            }));
        }
        Ok((style_ops, blocks))
    }
}

/// The deepest level a `TOC` instruction asks for: the upper bound of its
/// `\o "1-3"` switch, clamped to 1..=9. Defaults to Word's 3 when the switch is
/// absent or unreadable, which is what Word's own dialog offers.
///
/// **Complexity: O(instruction length).**
#[must_use]
fn toc_levels_of(instruction: &str) -> u8 {
    let Some(rest) = instruction.split("\\o").nth(1) else {
        return DEFAULT_TOC_LEVEL;
    };
    let Some(quoted) = rest.split('"').nth(1) else {
        return DEFAULT_TOC_LEVEL;
    };
    quoted
        .split('-')
        .nth(1)
        .and_then(|value| value.trim().parse::<u8>().ok())
        .map_or(DEFAULT_TOC_LEVEL, |level| level.clamp(1, MAX_TOC_LEVEL))
}

/// The instruction a generated table of contents carries.
///
/// `\z` (hide in web layout) and `\u` (use outline levels) are Word's own
/// defaults; `\h` is deliberately absent — see the module note on hyperlinks.
///
/// **Complexity: O(1).**
#[must_use]
fn toc_instruction(max_level: u8) -> String {
    format!(" TOC \\o \"1-{max_level}\" \\z \\u ")
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
        let max_level = if max_level == 0 {
            DEFAULT_TOC_LEVEL
        } else {
            (max_level.min(u32::from(MAX_TOC_LEVEL)) as u8).max(1)
        };
        let index = self.block_index_of(node);
        if index < 0 {
            return Err(refused!(
                "toc.body-only",
                "A table of contents goes in the document body, not in a header, footer or note."
            )
            .to_owned());
        }
        let headings = self.toc_headings(max_level)?;
        if headings.is_empty() {
            return Err(format!(
                "the document has no headings at level {max_level} or above to list"
            ));
        }
        let field = FieldRangeId::new(
            self.edit_ids
                .next_id()
                .map_err(|_| "id space exhausted".to_owned())?,
        );
        let (mut ops, blocks) = self.toc_blocks(&headings, field)?;
        let caret = blocks
            .first()
            .and_then(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(Pos::new(paragraph.id, 0)),
                _ => None,
            })
            .ok_or_else(|| {
                refused!(
                    "toc.empty",
                    "This table of contents has no entries to go to."
                )
                .to_owned()
            })?;
        let instruction = toc_instruction(max_level);
        ops.push(Operation::InsertFieldRange {
            field,
            definition: Box::new(FieldRange {
                kind: FieldKind::parse(&instruction),
                instruction,
                update: casual_doc_model::v1::FieldUpdateState::default(),
            }),
            index: index as u32,
            blocks,
        });
        self.apply_action_caret_as(ops, caret, HistoryKind::FieldChange)
    }

    /// See [`WasmDocument::update_table_of_contents`].
    pub(crate) fn update_table_of_contents_inner(
        &mut self,
        id: &str,
        mode: &str,
    ) -> Result<EditResult, String> {
        if mode != "pageNumbers" && mode != "entire" {
            return Err("update mode must be pageNumbers or entire".to_owned());
        }
        let (field, instruction) = self.toc_field(id)?;
        let (first, last) = body_field_range_blocks(self.document.body(), field)
            .map_err(|_| "the contents field's markers are not at body top level".to_owned())?;
        let count = (last - first + 1) as u32;
        let max_level = toc_levels_of(&instruction);
        let headings = self.toc_headings(max_level)?;

        let blocks = if mode == "entire" {
            if headings.is_empty() {
                return Err(refused!(
                    "toc.no-headings",
                    "This document has no headings to list. Apply a heading style to the text \
                     you want listed."
                )
                .to_owned());
            }
            let (style_ops, blocks) = self.toc_blocks(&headings, field)?;
            let mut ops = style_ops;
            ops.push(Operation::DeleteBlocks {
                container: None,
                index: first as u32,
                count,
            });
            ops.push(Operation::InsertBlocks {
                container: None,
                index: first as u32,
                blocks,
            });
            return self.apply_action_as(ops, HistoryKind::FieldChange);
        } else {
            self.toc_renumbered_blocks(first, last, &headings)?
        };

        self.apply_action_as(
            vec![
                Operation::DeleteBlocks {
                    container: None,
                    index: first as u32,
                    count,
                },
                Operation::InsertBlocks {
                    container: None,
                    index: first as u32,
                    blocks,
                },
            ],
            HistoryKind::FieldChange,
        )
    }

    /// The field range a contents command acts on: the one `id` names, or — when
    /// `id` is empty — the document's single `TOC` field range. Returns its id and
    /// its instruction.
    ///
    /// **Complexity: O(field ranges).**
    fn toc_field(&self, id: &str) -> Result<(FieldRangeId, String), String> {
        if !id.is_empty() {
            let node = NodeId::from_str(id).map_err(|_| "invalid field range id".to_owned())?;
            let field = FieldRangeId::new(node);
            let range = self
                .document
                .definitions()
                .field_ranges
                .get(&field)
                .ok_or_else(|| "no such field range".to_owned())?;
            return Ok((field, range.instruction.clone()));
        }
        let mut found = self
            .document
            .definitions()
            .field_ranges
            .iter()
            .filter(|(_, range)| range.kind == FieldKind::Toc)
            .map(|(id, range)| (*id, range.instruction.clone()));
        let first = found.next().ok_or_else(|| {
            refused!(
                "toc.none",
                "This document has no table of contents to update."
            )
            .to_owned()
        })?;
        if found.next().is_some() {
            return Err(
                "this document holds more than one table of contents; name the one to update"
                    .to_owned(),
            );
        }
        Ok(first)
    }

    /// The contents blocks `first..=last` with only their page numbers rewritten
    /// from `headings`: each entry keeps its own runs, its own formatting, and the
    /// field markers it carries, and only the text after its tab is replaced.
    ///
    /// Entries are paired with headings POSITIONALLY, which is what "update page
    /// numbers only" means — Word's other mode is the one that re-derives the
    /// entries. When the counts disagree the outline has changed underneath the
    /// table, so this refuses and names the mode that fixes it rather than
    /// writing each heading's page onto the wrong row.
    ///
    /// **Complexity: O(entries + inlines in them).**
    fn toc_renumbered_blocks(
        &mut self,
        first: usize,
        last: usize,
        headings: &[Heading],
    ) -> Result<Vec<BlockNode>, String> {
        let blocks: Vec<BlockNode> = self.document.body()[first..=last].to_vec();
        let entries = blocks
            .iter()
            .filter(|block| matches!(block, BlockNode::Paragraph(p) if has_tab(&p.inlines)))
            .count();
        if entries != headings.len() {
            return Err(format!(
                "this table lists {entries} entries but the document now has {} headings; \
                 update the entire table instead of only its page numbers",
                headings.len()
            ));
        }

        let mut out = Vec::with_capacity(blocks.len());
        let mut next = 0_usize;
        for block in blocks {
            let BlockNode::Paragraph(mut paragraph) = block else {
                out.push(block);
                continue;
            };
            let Some(tab) = paragraph
                .inlines
                .iter()
                .position(|inline| matches!(inline, InlineNode::Tab(_)))
            else {
                out.push(BlockNode::Paragraph(paragraph));
                continue;
            };
            let end_marker = match paragraph.inlines.last() {
                Some(InlineNode::FieldRangeEnd(marker)) => Some(InlineNode::FieldRangeEnd(*marker)),
                _ => None,
            };
            let mut inlines: Vec<InlineNode> = paragraph.inlines[..=tab].to_vec();
            inlines.push(InlineNode::Run(Run {
                id: self
                    .edit_ids
                    .next_id()
                    .map_err(|_| "id space exhausted".to_owned())?,
                properties: RunProperties::default().into(),
                text: headings[next].page.clone(),
            }));
            inlines.extend(end_marker);
            paragraph.inlines = inlines;
            next += 1;
            out.push(BlockNode::Paragraph(paragraph));
        }
        Ok(out)
    }
}

/// Whether an inline list holds a tab — what makes a contents paragraph an
/// ENTRY rather than, say, a "Contents" title the field happens to enclose.
///
/// **Complexity: O(inlines).**
#[must_use]
fn has_tab(inlines: &[InlineNode]) -> bool {
    inlines
        .iter()
        .any(|inline| matches!(inline, InlineNode::Tab(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::wasm_document;
    use casual_doc_model::v1::Document;

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

    #[test]
    fn toc_levels_are_read_out_of_the_instruction() {
        assert_eq!(toc_levels_of(r#" TOC \o "1-5" \h "#), 5);
        assert_eq!(toc_levels_of(r#" TOC \o "1-99" \h "#), MAX_TOC_LEVEL);
        assert_eq!(toc_levels_of(" TOC \\h "), DEFAULT_TOC_LEVEL);
        assert_eq!(toc_levels_of(""), DEFAULT_TOC_LEVEL);
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
