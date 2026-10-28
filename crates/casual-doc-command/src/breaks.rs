// SPDX-License-Identifier: Apache-2.0

//! Break lowerings the editor and agents share (`docs/181` §4.2, step 6):
//! line, page and column breaks, and section breaks.
//!
//! The site rules (where a forced break is honoured, which section a caret is
//! in, what a new section inherits) are `casual_doc_edit::breaks`'s; what lives
//! here is the gesture: which ids are minted in which order and which
//! operations one break is.

use casual_doc_edit::breaks::{BreakSite, break_site, section_break_ops, section_split_site};
use casual_doc_edit::table_structure::MintId;
use casual_doc_edit::{Operation, Pos, refused};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{Break, BreakKind, Document, InlineNode, SectionId, SectionType};

fn next(ids: MintId<'_>) -> Result<NodeId, String> {
    ids().ok_or_else(|| "id space exhausted".to_owned())
}

/// The break kind a name gives: `line`, `page` or `column`. O(1).
#[must_use]
pub fn break_kind(name: &str) -> Option<BreakKind> {
    match name {
        "line" => Some(BreakKind::Line),
        "page" => Some(BreakKind::Page),
        "column" => Some(BreakKind::Column),
        _ => None,
    }
}

/// A line, page or column break at `at`. A line break goes anywhere a caret
/// does (Shift+Enter); a page or column break is refused, naming the
/// container, where only the body's paginator would ignore it (a table cell, a
/// text box, a header, a note). O(top-level body blocks) for the site check.
///
/// # Errors
/// The refusal sentence, or ids running out.
pub fn insert_break(
    document: &Document,
    at: Pos,
    kind: BreakKind,
    ids: MintId<'_>,
) -> Result<Vec<Operation>, String> {
    if kind != BreakKind::Line
        && let BreakSite::Elsewhere(refusal) = break_site(document, at.node)
    {
        return Err(refusal.reason().to_owned());
    }
    let id = next(ids)?;
    Ok(vec![Operation::InsertInlineObject {
        at,
        node: Box::new(InlineNode::Break(Break { id, kind })),
    }])
}

/// Word's Blank Page: two page breaks at `at`, leaving one empty page between
/// them. Refused where a page break is. O(top-level body blocks).
///
/// # Errors
/// The refusal sentence, or ids running out.
pub fn insert_blank_page(
    document: &Document,
    at: Pos,
    ids: MintId<'_>,
) -> Result<Vec<Operation>, String> {
    if let BreakSite::Elsewhere(refusal) = break_site(document, at.node) {
        return Err(refusal.reason().to_owned());
    }
    let first = next(ids)?;
    let second = next(ids)?;
    Ok([first, second]
        .into_iter()
        .map(|id| Operation::InsertInlineObject {
            at,
            node: Box::new(InlineNode::Break(Break {
                id,
                kind: BreakKind::Page,
            })),
        })
        .collect())
}

/// The section start type a name gives: `nextPage`, `continuous`, `evenPage`
/// or `oddPage` (the editor's tokens; snake_case is accepted too). O(1).
#[must_use]
pub fn section_start(name: &str) -> Option<SectionType> {
    match name {
        "nextPage" | "next_page" => Some(SectionType::NextPage),
        "continuous" => Some(SectionType::Continuous),
        "evenPage" | "even_page" => Some(SectionType::EvenPage),
        "oddPage" | "odd_page" => Some(SectionType::OddPage),
        _ => None,
    }
}

/// A lowered section break.
#[derive(Clone, Debug)]
pub struct SectionBreak {
    /// The operations, one step.
    pub operations: Vec<Operation>,
    /// The first paragraph of the new section, where the caret goes.
    pub caret: Pos,
}

/// A section break at `at`, starting the new section as `start`: the caret's
/// section keeps the content before the break; the content after it becomes
/// a new section inheriting its geometry, running content and watermark, and
/// its form protection (Word copies the whole `w:sectPr`). Refused unless `at`
/// is on a top-level body paragraph. O(top-level body blocks) + O(sections).
///
/// # Errors
/// The refusal sentence, or ids running out.
pub fn section_break(
    document: &Document,
    at: Pos,
    start: SectionType,
    ids: MintId<'_>,
) -> Result<SectionBreak, String> {
    let split =
        section_split_site(document, at.node).map_err(|refusal| refusal.reason().to_owned())?;
    let inherited = document
        .definitions()
        .sections
        .iter()
        .find(|boundary| boundary.id == split.current)
        .ok_or_else(|| {
            refused!("section.not-defined", "The caret's section is not defined.").to_owned()
        })?
        .clone();
    let new_section = SectionId::new(next(ids)?);
    let new_paragraph = next(ids)?;
    let mut operations =
        section_break_ops(at, &split, &inherited, new_section, new_paragraph, start);
    // The model keeps `w:formProt` in a side table keyed by section (`109`
    // FID-AT-06), so it travels as its own operation, after the boundary it
    // names exists.
    if let Some(protected) = document
        .definitions()
        .section_form_protection(split.current)
    {
        operations.push(Operation::SetSectionFormProtection {
            section: new_section,
            protected: Some(protected),
        });
    }
    Ok(SectionBreak {
        operations,
        caret: Pos::new(new_paragraph, 0),
    })
}
