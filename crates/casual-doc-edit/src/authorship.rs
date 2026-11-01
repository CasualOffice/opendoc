// SPDX-License-Identifier: Apache-2.0

//! Who wrote a comment or a tracked change, decided by the order and not by the sender —
//! `docs/175` §5, ADR-070.
//!
//! # Why this is a function every replica runs, and not a check anybody refuses on
//!
//! A comment travels inside [`Operation::UpdateReviewState`], whose `comments` field replaces
//! the **whole** map, and the relay holds no document (ADR-047), so the relay cannot tell a new
//! comment from an old one and cannot check an author field. A replica that *refused* a forged
//! author would be one ordered revision short of everybody else, which is the divergence
//! `docs/169` records for `IdCollision`. So nothing is refused. The relay attaches the author it
//! verified to the arrival (`Arrival.author`), and every replica runs [`conform`], a
//! deterministic function of the document **before** the operation, the operation and that
//! stamp. Same ordered input, same output, so convergence is kept; a forged author never
//! reaches another screen or a saved file.
//!
//! # The rule
//!
//! For every **mark** an operation carries (a tracked insertion or deletion, a move range, a
//! tracked run-format change, a tracked paragraph mark or paragraph-format change) and every
//! **comment** in a replacement comments map:
//!
//! 1. **Held** — the document already has it (same node, same comment id): it keeps the
//!    author the document holds. An operation that rewrites somebody else's authorship has
//!    that part restored and the rest still applies.
//! 2. **Introduced by the sender** — not held, and its identity was minted in the sender's
//!    own id space (`IdSpace`, ADR-051): it takes the stamp's name (and, for a comment, its
//!    initials). This is the case a forger reaches, and the case a paste reaches: a pasted
//!    tracked change is minted fresh by the paster, so **it takes the paster's name**. That
//!    differs from Word, where a pasted tracked change keeps its original author; it is the
//!    owner's decision (`docs/175` §12 Q5) until `152` §10 Q2's subtree walk exists.
//! 3. **Restored** — not held, and minted in some *other* space: it is an undo or redo putting
//!    back a mark somebody else made, and it keeps the author it carries. Without this rule an
//!    undo of "accept" would hand the restored suggestion to whoever pressed undo.
//!
//! **The agent principal (`docs/178` §10, `docs/183` §24).** An introduced item may instead
//! carry [`agent_author`] of the stamp's name, "Casual AI on behalf of Ada Lovelace", and then
//! keeps it: the assistant a participant ran wrote it. The principal is DERIVED from the
//! attested stamp, never stated by the sender, so a participant can attribute work to their
//! own assistant and to nobody else's: "Casual AI on behalf of Bob Stone" from Ada's
//! connection is restamped to "Ada Lovelace" like any other forgery. The agent label is one
//! fixed name, [`AGENT_NAME`], not a free prefix, so "Bob Stone on behalf of Ada Lovelace" is
//! not a principal either. Every replica runs the same check on the same stamp, so the agent
//! author converges as the person's name does.
//!
//! The date of an introduced item is kept as authored: it is display data, Word lets it be
//! anything, and a relay clock would be one more value to trust (`docs/175` §5.2).
//!
//! # What this does not see, stated rather than claimed away
//!
//! - **A forger minting in another participant's space** reaches rule 3 and keeps the author
//!   it wrote. Telling that apart from a genuine undo needs the history of who minted what,
//!   which no replica keeps. The id-space rule closes the cheap forgery — a new comment or a
//!   new suggestion in one's own space under somebody else's name — which is the one an honest
//!   client's code path could be bent into.
//! - **Table row and cell marks** (`w:trPr`/`w:tcPr` insertions and deletions) are carried
//!   unconformed. No editor command authors one today.
//! - **Block stories inside an inline** (a text box, a group) are separate paragraphs with their
//!   own `UpdateReviewState`, and are not descended into from their anchor's paragraph.
//!
//! # Complexity
//!
//! O(the inlines of the paragraphs the operation names) plus one paragraph lookup per named
//! paragraph, and O(comments in the replacement map × log comments). Never a walk of the
//! document: the operations that carry review state already carry only touched paragraphs.

use std::collections::BTreeMap;

use casual_doc_model::v1::{BlockNode, Document, InlineNode, Paragraph};
use casual_doc_model::{IdSpace, NodeId};

use crate::Operation;
use crate::containers::{contained_inlines, contained_inlines_mut};

/// The one agent this engine knows as a principal: the product's own assistant. Written
/// into documents as is (an author name, never translated), like a person's name.
pub const AGENT_NAME: &str = "Casual AI";

/// The comment initials the agent principal's comments carry.
pub const AGENT_INITIALS: &str = "AI";

const ON_BEHALF_OF: &str = " on behalf of ";

/// The author the agent principal writes under for `person`: "Casual AI on behalf of Ada
/// Lovelace". O(name).
#[must_use]
pub fn agent_author(person: &str) -> String {
    format!("{AGENT_NAME}{ON_BEHALF_OF}{person}")
}

/// Whether `author` is the agent principal acting for `person`. O(name), no allocation.
#[must_use]
pub fn is_agent_for(author: &str, person: &str) -> bool {
    author
        .strip_prefix(AGENT_NAME)
        .and_then(|rest| rest.strip_prefix(ON_BEHALF_OF))
        == Some(person)
}

/// The person an agent-principal author acts for, if `author` is one. O(name).
#[must_use]
pub fn agent_principal(author: &str) -> Option<&str> {
    author
        .strip_prefix(AGENT_NAME)
        .and_then(|rest| rest.strip_prefix(ON_BEHALF_OF))
        .filter(|person| !person.is_empty())
}

/// Who the order says wrote an operation: the stamp [`conform`] writes onto what it introduces.
#[derive(Clone, Copy, Debug)]
pub struct Stamp<'a> {
    /// The display name, exactly as the host gave it. Never suffixed (`docs/175` §12 Q3): a
    /// duplicate name's "(2)" is a rendering, and altering a person's name inside a document is
    /// not the editor's to do.
    pub name: &'a str,
    /// The initials a comment carries.
    pub initials: &'a str,
    /// The id space the sender mints in, which is what tells "introduced" from "restored".
    pub space: IdSpace,
}

impl Stamp<'_> {
    /// Whether an introduced item may keep `author`: the stamp's own name, or its agent
    /// principal's. O(name).
    fn admits(&self, author: Option<&str>) -> bool {
        author.is_some_and(|author| author == self.name || is_agent_for(author, self.name))
    }
}

/// What [`conform`] changed, for a caller that wants to report it. Zero on an honest arrival.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Conformed {
    /// Introduced items whose author was not the stamp's and now is.
    pub restamped: usize,
    /// Held items whose author the operation tried to change and which were put back.
    pub restored: usize,
}

impl Conformed {
    /// Whether anything was changed.
    #[must_use]
    pub const fn changed(self) -> bool {
        self.restamped + self.restored > 0
    }
}

/// Initials derived from a display name: the first character of up to its first two
/// whitespace-separated words, upper-cased ("Ada Lovelace" → "AL").
///
/// One derivation, used by the editor's own active author and by [`conform`], so an honest
/// replica's optimistic comment and everybody else's conformed copy carry the same initials.
#[must_use]
pub fn derive_initials(name: &str) -> String {
    name.split_whitespace()
        .filter(|part| !part.is_empty())
        .take(2)
        .filter_map(|part| part.chars().next())
        .flat_map(char::to_uppercase)
        .collect()
}

/// Conforms the authorship one operation carries to `stamp`, against `document` as it stands
/// **before** the operation applies. See the module docs for the rule.
///
/// Operations that carry no authorship are left untouched.
pub fn conform(document: &Document, operation: &mut Operation, stamp: &Stamp<'_>) -> Conformed {
    let mut out = Conformed::default();
    match operation {
        Operation::UpdateReviewState {
            paragraphs,
            comments,
        } => {
            for state in paragraphs.iter_mut() {
                let held = held_marks(document.paragraph(state.node).map(|p| p.inlines.as_slice()));
                conform_inlines(&mut state.inlines, &held, stamp, &mut out);
            }
            if let Some(comments) = comments {
                let log = &document.definitions().comments;
                // Collected first: `DefinitionMap` lends its values mutably one key at a time.
                let ids: Vec<_> = comments.iter().map(|(id, _)| *id).collect();
                for id in ids {
                    let Some(comment) = comments.get_mut(&id) else {
                        continue;
                    };
                    if let Some(prior) = log.get(&id) {
                        if comment.author != prior.author
                            || comment.initials != prior.initials
                            || comment.person != prior.person
                        {
                            comment.author.clone_from(&prior.author);
                            comment.initials.clone_from(&prior.initials);
                            comment.person.clone_from(&prior.person);
                            out.restored += 1;
                        }
                    } else if stamp.space.holds(id.node_id())
                        && comment
                            .author
                            .as_deref()
                            .is_some_and(|author| is_agent_for(author, stamp.name))
                    {
                        // The participant's own assistant: it keeps its principal, with the
                        // agent's initials and no `people.xml` join to somebody else.
                        let person_ok =
                            comment.person.is_none() || comment.person == comment.author;
                        let initials = Some(AGENT_INITIALS.to_owned());
                        if comment.initials != initials || !person_ok {
                            comment.initials = initials;
                            if !person_ok {
                                comment.person = None;
                            }
                            out.restamped += 1;
                        }
                    } else if stamp.space.holds(id.node_id()) {
                        let name = Some(stamp.name.to_owned());
                        let initials = Some(stamp.initials.to_owned());
                        // `person` joins a comment to `people.xml` BY NAME, so a value that is
                        // not the stamp's name would point the comment at somebody else's entry.
                        let person_ok = comment.person.is_none()
                            || comment.person.as_deref() == Some(stamp.name);
                        if comment.author != name || comment.initials != initials || !person_ok {
                            comment.author = name;
                            comment.initials = initials;
                            if !person_ok {
                                comment.person = None;
                            }
                            out.restamped += 1;
                        }
                    }
                }
            }
        }
        Operation::SetInlines { node, inlines } => {
            let held = held_marks(document.paragraph(*node).map(|p| p.inlines.as_slice()));
            conform_inlines(inlines, &held, stamp, &mut out);
        }
        Operation::SetParagraphProperties { node, properties } => {
            let prior = document.paragraph(*node);
            conform_paragraph_marks(*node, properties, prior, stamp, &mut out);
        }
        Operation::InsertBlocks { blocks, .. } => {
            conform_blocks(blocks, stamp, &mut out);
        }
        _ => {}
    }
    out
}

/// Which kind of mark a held author belongs to. Two kinds can hang off one node (a run's id is
/// also the key of its format change), so the node id alone is not the key.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Mark {
    Revision,
    Move,
    RunFormat,
}

/// The author each mark in `inlines` carries, keyed by mark kind and node. O(inlines).
fn held_marks(inlines: Option<&[InlineNode]>) -> BTreeMap<(Mark, NodeId), Option<String>> {
    fn walk(inlines: &[InlineNode], out: &mut BTreeMap<(Mark, NodeId), Option<String>>) {
        for inline in inlines {
            if let Some((mark, id, _, author)) = mark_of(inline) {
                out.insert((mark, id), author.cloned());
            }
            // container-set: inline-in-inline only (`Hyperlink`, `Field`, `Revision`, `Sdt`);
            // a `TextBox` or `Group` holds separate paragraphs with their own review state.
            if let Some(children) = contained_inlines(inline) {
                walk(children, out);
            }
        }
    }
    let mut out = BTreeMap::new();
    if let Some(inlines) = inlines {
        walk(inlines, &mut out);
    }
    out
}

/// The mark an inline carries, if any: its kind, the node it is keyed by, the identity whose
/// id space says who minted it, and its author.
///
/// The minted identity is the node itself for a revision or a move range. A run-format change
/// hangs off a run that may be far older than the change, so the change's own editor group
/// (minted with the action) speaks for it when there is one, and the run when there is not.
fn mark_of(inline: &InlineNode) -> Option<(Mark, NodeId, NodeId, Option<&String>)> {
    // container-set: a leaf question about this one node; descent is the caller's, through
    // `contained_inlines` (`Hyperlink`, `Field`, `Revision`, `Sdt`).
    match inline {
        InlineNode::Revision(revision) => Some((
            Mark::Revision,
            revision.id,
            revision.id,
            revision.author.as_ref(),
        )),
        InlineNode::MoveRangeStart(start) => {
            Some((Mark::Move, start.id, start.id, start.author.as_ref()))
        }
        InlineNode::Run(run) => run.properties.prop_change.as_ref().map(|change| {
            let minted = change.editor_group.map_or(run.id, |group| group.id);
            (Mark::RunFormat, run.id, minted, change.author.as_ref())
        }),
        _ => None,
    }
}

/// The author slot of the mark an inline carries, mutably.
fn author_slot(inline: &mut InlineNode) -> Option<&mut Option<String>> {
    // container-set: a leaf question about this one node; descent is the caller's, through
    // `contained_inlines_mut` (`Hyperlink`, `Field`, `Revision`, `Sdt`).
    match inline {
        InlineNode::Revision(revision) => Some(&mut revision.author),
        InlineNode::MoveRangeStart(start) => Some(&mut start.author),
        InlineNode::Run(run) => {
            // Copy-on-write only when the run carries a change at all; a run without one is
            // never written, so interning is untouched on the common path.
            run.properties.prop_change.as_ref()?;
            let change = run.properties.make_mut().prop_change.as_mut()?;
            Some(&mut change.author)
        }
        _ => None,
    }
}

/// Applies the rule to one author slot. `held` is `Some(author)` when the document holds the
/// mark, `None` when it does not; `minted` is the identity whose space says who made it.
fn settle(
    slot: &mut Option<String>,
    minted: NodeId,
    held: Option<&Option<String>>,
    stamp: &Stamp<'_>,
    out: &mut Conformed,
) {
    match held {
        Some(prior) if slot != prior => {
            slot.clone_from(prior);
            out.restored += 1;
        }
        None if stamp.space.holds(minted) && !stamp.admits(slot.as_deref()) => {
            *slot = Some(stamp.name.to_owned());
            out.restamped += 1;
        }
        _ => {}
    }
}

fn conform_inlines(
    inlines: &mut [InlineNode],
    held: &BTreeMap<(Mark, NodeId), Option<String>>,
    stamp: &Stamp<'_>,
    out: &mut Conformed,
) {
    for inline in inlines.iter_mut() {
        // Read-only first: `author_slot` copies a shared run's properties on write, so it is
        // reached only when the slot actually has to change.
        if let Some((mark, id, minted, author)) = mark_of(inline) {
            let prior = held.get(&(mark, id));
            let wanted = match prior {
                Some(prior) => author != prior.as_ref(),
                None => stamp.space.holds(minted) && !stamp.admits(author.map(String::as_str)),
            };
            if wanted && let Some(slot) = author_slot(inline) {
                settle(slot, minted, prior, stamp, out);
            }
        }
        // container-set: inline-in-inline only (`Hyperlink`, `Field`, `Revision`, `Sdt`);
        // a `TextBox` or `Group` holds separate paragraphs with their own review state.
        if let Some(children) = contained_inlines_mut(inline) {
            conform_inlines(children, held, stamp, out);
        }
    }
}

/// A paragraph's own tracked mark and tracked property change, against the paragraph it
/// replaces (`None` for a paragraph the operation introduces).
fn conform_paragraph_marks(
    node: NodeId,
    properties: &mut casual_doc_model::v1::ParagraphProperties,
    prior: Option<&Paragraph>,
    stamp: &Stamp<'_>,
    out: &mut Conformed,
) {
    let held_mark = prior.and_then(|p| {
        p.properties
            .mark_revision
            .as_ref()
            .map(|mark| mark.author.clone())
    });
    if let Some(mark) = properties.mark_revision.as_mut() {
        settle(&mut mark.author, node, held_mark.as_ref(), stamp, out);
    }
    let held_change = prior.and_then(|p| {
        p.properties
            .prop_change
            .as_ref()
            .map(|change| change.author.clone())
    });
    if let Some(change) = properties.prop_change.as_mut() {
        settle(&mut change.author, node, held_change.as_ref(), stamp, out);
    }
}

/// Every paragraph a block list carries, all of them introduced by the operation.
fn conform_blocks(blocks: &mut [BlockNode], stamp: &Stamp<'_>, out: &mut Conformed) {
    let nothing = BTreeMap::new();
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let id = paragraph.id;
                let has_marks = paragraph.properties.mark_revision.is_some()
                    || paragraph.properties.prop_change.is_some();
                if has_marks {
                    conform_paragraph_marks(id, paragraph.properties.make_mut(), None, stamp, out);
                }
                conform_inlines(&mut paragraph.inlines, &nothing, stamp, out);
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        conform_blocks(&mut cell.blocks, stamp, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => conform_blocks(&mut sdt.blocks, stamp, out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

#[cfg(test)]
#[path = "authorship_tests.rs"]
mod tests;
