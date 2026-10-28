// SPDX-License-Identifier: Apache-2.0

//! What a headless session's layout knows, lent to the shared reference
//! lowerings (`casual_doc_command::references::LayoutFacts`, `docs/181` §12,
//! lane references).
//!
//! The editor answers the same questions from its own pagination and style
//! cascade. Here heading levels come from the same cascade
//! (`casual_doc_layout::fold::heading_level`), and page labels from a layout
//! pass of the plan's current state through `opendoc-viewer`, which reads them
//! with the same function the editor's table of contents does
//! (`casual_doc_layout::paginate::first_page_labels`).

use std::collections::{BTreeMap, HashMap};

use casual_doc_command::references::LayoutFacts;
use casual_doc_layout::cascade::StyleCascade;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{Definitions, Document, Paragraph};
use opendoc_viewer::{ViewerDocument, ViewerOptions};

/// Heading levels, and the page labels of a layout pass when a command needs
/// pages.
pub(crate) struct AgentFacts<'a> {
    definitions: &'a Definitions,
    cascade: StyleCascade<'a>,
    pages: HashMap<NodeId, String>,
}

impl<'a> AgentFacts<'a> {
    /// Heading levels only, for a command that never prints a page (a
    /// caption, a renumber). O(1).
    pub(crate) fn headings(document: &'a Document) -> Self {
        let definitions = document.definitions();
        Self {
            definitions,
            cascade: StyleCascade::new(definitions),
            pages: HashMap::new(),
        }
    }

    /// Heading levels and the page label of every node, from one pagination
    /// of `document` as it stands mid-plan, so a table of contents inserted
    /// after other edits in the same plan lists the pages those edits produce.
    /// Pictures paginate from their modelled extents, so no media bytes are
    /// needed.
    ///
    /// **O(document)**: one pagination. Only the commands that print a page
    /// pay it.
    pub(crate) fn paginated(document: &'a Document) -> Self {
        let viewer = ViewerDocument::from_document(
            document.clone(),
            BTreeMap::new(),
            &ViewerOptions::default(),
        );
        Self {
            pages: viewer.page_labels(),
            ..Self::headings(document)
        }
    }
}

impl LayoutFacts for AgentFacts<'_> {
    fn heading_level(&self, paragraph: &Paragraph) -> Option<u8> {
        casual_doc_layout::fold::heading_level(
            paragraph.properties.get(),
            self.definitions,
            &self.cascade,
        )
    }

    fn page_label(&self, node: NodeId) -> Option<String> {
        self.pages.get(&node).cloned()
    }
}
