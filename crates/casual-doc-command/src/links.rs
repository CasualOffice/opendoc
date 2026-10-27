// SPDX-License-Identifier: Apache-2.0

//! Hyperlink and bookmark lowerings the editor and agents share (`docs/181`
//! §4.2, step 4).
//!
//! The editor's `setHyperlink`, `removeHyperlink`, `createBookmark`,
//! `renameBookmark` and `deleteBookmark` resolve the caret or selection and
//! call these; an agent resolves a quote anchor and calls the same functions.
//! Every one is O(1) in document size except [`bookmark_named`], which is
//! O(bookmarks).

use casual_doc_edit::table_structure::MintId;
use casual_doc_edit::{Operation, Pos, Range};
use casual_doc_model::v1::{BookmarkId, Document, ExternalTarget, HyperlinkTarget, InternalTarget};

/// The most bytes a link target may hold (the model's URL bound).
pub const MAX_TARGET_BYTES: usize = 2048;

fn next(ids: MintId<'_>) -> Result<casual_doc_model::NodeId, String> {
    ids().ok_or_else(|| "id space exhausted".to_owned())
}

/// The model target of an editor target string: `#name` is an internal link to
/// the bookmark `name`; anything else is an external URL kept as written.
/// O(target).
#[must_use]
pub fn target(target: &str) -> HyperlinkTarget {
    match target.strip_prefix('#') {
        Some(anchor) => HyperlinkTarget::Internal(InternalTarget {
            anchor: anchor.to_owned(),
        }),
        None => HyperlinkTarget::External(ExternalTarget {
            url: target.to_owned(),
            anchor: None,
        }),
    }
}

/// Whether an agent may write `target`: a bookmark (`#name`) or an absolute
/// `http`, `https`, `mailto`, `ftp` or `tel` URL. The editor lets a person
/// type any target (the host owns activation); an agent's target is untrusted
/// input, so a `javascript:` or `file:` link is refused rather than planted
/// (`docs/181` R6, `docs/178` §5.8). O(target).
///
/// # Errors
/// The sentence an agent reads.
pub fn agent_target(target: &str) -> Result<(), String> {
    if target.is_empty() || target.len() > MAX_TARGET_BYTES {
        return Err(format!(
            "A link target is 1 to {MAX_TARGET_BYTES} bytes: a URL, or `#` and a bookmark name."
        ));
    }
    if let Some(name) = target.strip_prefix('#') {
        return if name.is_empty() {
            Err("`#` must be followed by a bookmark name.".to_owned())
        } else {
            Ok(())
        };
    }
    let lower = target.to_ascii_lowercase();
    let allowed = ["http://", "https://", "mailto:", "ftp://", "tel:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme));
    if allowed && !target.chars().any(char::is_control) {
        Ok(())
    } else {
        Err(
            "A link target must be an http, https, mailto, ftp or tel URL, or `#` and a bookmark \
             name."
                .to_owned(),
        )
    }
}

/// Links `[start, end)` of paragraph `node` to `target` (see [`target`]), with
/// an optional tooltip. One `SetHyperlink`; the engine refuses an empty or
/// cross-run-boundary range it cannot wrap. O(1).
///
/// # Errors
/// When ids run out.
pub fn set_hyperlink(
    node: casual_doc_model::NodeId,
    start: u32,
    end: u32,
    to: &str,
    tooltip: Option<String>,
    ids: MintId<'_>,
) -> Result<Vec<Operation>, String> {
    link_range(
        Range {
            start: Pos::new(node, start),
            end: Pos::new(node, end),
        },
        to,
        tooltip,
        ids,
    )
}

/// [`set_hyperlink`] over a model range (rich paste links each pasted run's
/// range this way). O(1).
///
/// # Errors
/// When ids run out.
pub fn link_range(
    range: Range,
    to: &str,
    tooltip: Option<String>,
    ids: MintId<'_>,
) -> Result<Vec<Operation>, String> {
    let id = next(ids)?;
    Ok(vec![Operation::SetHyperlink {
        range,
        id,
        target: Some(target(to)),
        tooltip,
    }])
}

/// Removes the hyperlink occupying exactly `[start, end)` of `node`, keeping
/// its text and formatting. O(1).
#[must_use]
pub fn remove_hyperlink(node: casual_doc_model::NodeId, start: u32, end: u32) -> Vec<Operation> {
    vec![Operation::SetHyperlink {
        range: Range {
            start: Pos::new(node, start),
            end: Pos::new(node, end),
        },
        // Unused when `target` is `None`; the editor has always passed the node.
        id: node,
        target: None,
        tooltip: None,
    }]
}

/// Creates the bookmark `name` from `start` to `end` (which may be in two
/// paragraphs): a fresh bookmark id and its two marker ids. O(1); the engine
/// refuses an empty or over-long name and endpoints that do not resolve.
///
/// # Errors
/// When ids run out.
pub fn create_bookmark(
    name: String,
    start: Pos,
    end: Pos,
    ids: MintId<'_>,
) -> Result<Vec<Operation>, String> {
    let bookmark = BookmarkId::new(next(ids)?);
    let start_id = next(ids)?;
    let end_id = next(ids)?;
    Ok(vec![Operation::CreateBookmark {
        bookmark,
        name,
        start,
        start_id,
        end,
        end_id,
    }])
}

/// Renames `bookmark` to `name`. O(1).
#[must_use]
pub fn rename_bookmark(bookmark: BookmarkId, name: String) -> Vec<Operation> {
    vec![Operation::RenameBookmark { bookmark, name }]
}

/// Deletes `bookmark`, its definition and both markers. O(1).
#[must_use]
pub fn delete_bookmark(bookmark: BookmarkId) -> Vec<Operation> {
    vec![Operation::DeleteBookmark { bookmark }]
}

/// The bookmark called exactly `name`, if any.
/// O(bookmarks).
#[must_use]
pub fn bookmark_named(document: &Document, name: &str) -> Option<BookmarkId> {
    document
        .definitions()
        .bookmarks
        .iter()
        .find(|(_, bookmark)| bookmark.name == name)
        .map(|(id, _)| *id)
}
