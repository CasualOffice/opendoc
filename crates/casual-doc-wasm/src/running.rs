// SPDX-License-Identifier: Apache-2.0

//! Header and footer band commands the editing chrome runs against the page in
//! view: Link to Previous, Remove, and the page-number position presets
//! (`docs/109` HF-192 and RM-23; `docs/168` batch 8; `docs/129`).
//!
//! The lowerings are `casual_doc_command::sections`, shared with agents
//! (`link_to_previous`, `remove_header_footer`, `insert_page_number`); this
//! module resolves "which section, which variant" from a PAGE — the same
//! resolution every running-content command here uses (`running_section`,
//! `running_kind_for_page`), so the band the user is looking at and the body the
//! change lands in are the same one — and applies the result as one undoable
//! action.
//!
//! Complexity: every command is O(sections + size of the band it touches); none
//! walks the body.

use casual_doc_command::sections::{self, PageNumberAlign};
use casual_doc_edit::{Pos, RunningRegion};
use casual_doc_model::NodeId;
use casual_doc_model::v1::HeaderFooterKind;
use std::str::FromStr;
use wasm_bindgen::prelude::*;

use crate::{EditResult, HistoryKind, WasmDocument, running_region, to_js};

/// `default`, `first` or `even`: the variant a page applies, as the host reads it.
const fn kind_token(kind: HeaderFooterKind) -> &'static str {
    match kind {
        HeaderFooterKind::Default => "default",
        HeaderFooterKind::First => "first",
        HeaderFooterKind::Even => "even",
    }
}

fn region_of(region: &str) -> Result<RunningRegion, String> {
    running_region(region).ok_or_else(|| "region must be \"header\" or \"footer\"".to_owned())
}

#[wasm_bindgen]
impl WasmDocument {
    /// The Link to Previous state of one band on one page, as JSON
    /// `{"section":2,"kind":"default","linked":true,"shown":true}`.
    ///
    /// `linked` is `null` on the first section's pages — there is no previous
    /// section, which is the state ONLYOFFICE disables the control in
    /// (`controller/HeaderFooterTab.js:247`) — and otherwise whether this
    /// section declares the page's variant itself. `kind` is the variant the
    /// page applies (first page, even page, or default), because linkage is per
    /// variant (`docs/85` §8d Q7). `shown` says whether the band holds anything
    /// visible, which is what Remove needs to be offered.
    ///
    /// O(sections).
    #[wasm_bindgen(js_name = runningLinkState)]
    #[must_use]
    pub fn running_link_state(&self, region: &str, page: u32) -> String {
        let Ok(region) = region_of(region) else {
            return "null".to_owned();
        };
        let Some((index, _)) = self.running_section(page) else {
            return "null".to_owned();
        };
        let kind = self.running_kind_for_page(page, index);
        let linked = sections::is_linked_to_previous(&self.document, index, region, kind)
            .map_or_else(|| "null".to_owned(), |linked| linked.to_string());
        let shown = sections::shown_blocks(&self.document, index, region, kind)
            .is_some_and(|blocks| !sections::blocks_are_blank(blocks));
        format!(
            "{{\"section\":{},\"kind\":\"{}\",\"linked\":{linked},\"shown\":{shown}}}",
            index + 1,
            kind_token(kind)
        )
    }

    /// Links the band `region` on `page` to the previous section's, or unlinks
    /// it (Word's and ONLYOFFICE's "Link to previous"), as one undoable action.
    ///
    /// Unlinking copies the band the page was showing into the section — every
    /// inline kind, the page number and the logo included, with fresh ids
    /// (`docs/129` §2) — so nothing on the page changes until the user edits it,
    /// and then only this section changes. `(node, offset)` is the caret; when it
    /// was in the band, the result's caret is the same place in the band the page
    /// shows afterwards. Asking for the state the band is already in is reported
    /// UNCHANGED (same revision, nothing on the undo stack).
    ///
    /// The result's `pasteLoss` names any marker family the copy could not carry
    /// (a bookmark or a comment anchor cannot exist twice in one document).
    ///
    /// # Errors
    /// On the first section's pages (nothing to link to), an unknown region, or
    /// a refusal from the choke point.
    #[wasm_bindgen(js_name = setRunningLinked)]
    pub fn set_running_linked(
        &mut self,
        region: &str,
        page: u32,
        linked: bool,
        node: &str,
        offset: u32,
    ) -> Result<EditResult, JsValue> {
        self.set_running_linked_inner(region, page, linked, node, offset)
            .map_err(to_js)
    }

    /// Removes the header or footer the page shows (Word's Remove Header,
    /// ONLYOFFICE's `asc_RemoveHeader`), from this section's pages and the
    /// sections linked to it, never from an earlier section's — see
    /// `casual_doc_command::sections::remove_running_content`. One undoable
    /// action; `(node, offset)` is the caret to keep, normally the body position
    /// the host returns to.
    ///
    /// # Errors
    /// When the page shows no such band, or the choke point refuses.
    #[wasm_bindgen(js_name = removeRunningContent)]
    pub fn remove_running_content(
        &mut self,
        region: &str,
        page: u32,
        node: &str,
        offset: u32,
    ) -> Result<EditResult, JsValue> {
        self.remove_running_content_inner(region, page, node, offset)
            .map_err(to_js)
    }

    /// Puts a page number in the band `region` on `page`, aligned `left`,
    /// `center` or `right` — the six top/bottom position presets of Word's and
    /// ONLYOFFICE's Page Number gallery. Creates the band when the page has
    /// none, and keeps whatever the band already holds (where both references
    /// clear it — `casual_doc_command::sections::insert_page_number`). The caret
    /// lands after the number, in the band. One undoable action.
    ///
    /// # Errors
    /// An unknown region or alignment, a band whose edge is a table, or a
    /// refusal from the choke point.
    #[wasm_bindgen(js_name = insertRunningPageNumber)]
    pub fn insert_running_page_number(
        &mut self,
        region: &str,
        page: u32,
        align: &str,
    ) -> Result<EditResult, JsValue> {
        self.insert_running_page_number_inner(region, page, align)
            .map_err(to_js)
    }
}

impl WasmDocument {
    /// The caret the host passed, or the document root when it passed none.
    fn caret_or_root(&self, node: &str, offset: u32) -> Pos {
        NodeId::from_str(node).map_or_else(
            |_| Pos::new(self.document.id(), 0),
            |node| Pos::new(node, offset),
        )
    }

    /// An edit that changed nothing, reported as such.
    fn unchanged(&self, caret: Pos) -> EditResult {
        EditResult {
            node: caret.node.to_string(),
            offset: caret.offset,
            revision: self.revision,
            page_count: self.page_count(),
            dirty: Vec::new(),
            paste_loss: Vec::new(),
            placed_object: String::new(),
        }
    }

    /// [`Self::set_running_linked`] with a `String` error, for native tests.
    pub(crate) fn set_running_linked_inner(
        &mut self,
        region: &str,
        page: u32,
        linked: bool,
        node: &str,
        offset: u32,
    ) -> Result<EditResult, String> {
        let region = region_of(region)?;
        let (index, _) = self
            .running_section(page)
            .ok_or_else(|| "the document has no section".to_owned())?;
        let kind = self.running_kind_for_page(page, index);
        let caret = self.caret_or_root(node, offset);
        let ids = &mut self.edit_ids;
        let lowered = sections::set_linked_to_previous(
            &self.document,
            index,
            region,
            kind,
            linked,
            &mut || ids.next_id().ok(),
        )?;
        if lowered.operations.is_empty() {
            return Ok(self.unchanged(caret));
        }
        // The caret follows the band: a caret in the body the page showed lands
        // on the matching paragraph of the body it shows now. A caret anywhere
        // else (the body text) is left where it was.
        let caret = lowered
            .paragraphs
            .iter()
            .find(|(before, _)| *before == caret.node)
            // An unlink copies the body, so the same offset in the copy is the
            // same place; a re-link shows an unrelated body, so its start.
            .map_or(caret, |(_, after)| {
                Pos::new(*after, if linked { 0 } else { caret.offset })
            });
        let mut result =
            self.apply_action_caret_as(lowered.operations, caret, HistoryKind::Edit)?;
        result.paste_loss = lowered
            .loss
            .kinds()
            .into_iter()
            .map(str::to_owned)
            .collect();
        Ok(result)
    }

    /// [`Self::remove_running_content`] with a `String` error, for native tests.
    pub(crate) fn remove_running_content_inner(
        &mut self,
        region: &str,
        page: u32,
        node: &str,
        offset: u32,
    ) -> Result<EditResult, String> {
        let region = region_of(region)?;
        let (index, _) = self
            .running_section(page)
            .ok_or_else(|| "the document has no section".to_owned())?;
        let kind = self.running_kind_for_page(page, index);
        let caret = self.caret_or_root(node, offset);
        let ids = &mut self.edit_ids;
        let operations =
            sections::remove_running_content(&self.document, index, region, kind, &mut || {
                ids.next_id().ok()
            })?;
        self.apply_action_caret_as(operations, caret, HistoryKind::Edit)
    }

    /// [`Self::insert_running_page_number`] with a `String` error, for native
    /// tests.
    pub(crate) fn insert_running_page_number_inner(
        &mut self,
        region: &str,
        page: u32,
        align: &str,
    ) -> Result<EditResult, String> {
        let region = region_of(region)?;
        let align = PageNumberAlign::parse(align)
            .ok_or_else(|| "align must be \"left\", \"center\" or \"right\"".to_owned())?;
        let (index, _) = self
            .running_section(page)
            .ok_or_else(|| "the document has no section".to_owned())?;
        let kind = self.running_kind_for_page(page, index);
        let ids = &mut self.edit_ids;
        let lowered =
            sections::insert_page_number(&self.document, index, region, kind, align, &mut || {
                ids.next_id().ok()
            })?;
        self.apply_action_caret_as(lowered.operations, lowered.caret, HistoryKind::FieldChange)
    }
}

#[cfg(test)]
#[path = "running_tests.rs"]
mod running_tests;
