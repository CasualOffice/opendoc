// SPDX-License-Identifier: Apache-2.0

//! Guards for [`conform`]: who the order says wrote a comment or a tracked change.
//!
//! Every guard creates its condition: a document whose comment and suggestion belong to "Ada
//! Lovelace", minted in the document's space (namespace 7), and a sender "Bob Stone" minting in
//! namespace 9. The mutations that drove each guard red are recorded in the commit message.

use super::*;

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{
    BlockNode, Comment, CommentId, CommentRangeEnd, CommentRangeStart, DefinitionMap, Definitions,
    InlineNode, Paragraph, ParagraphProperties, PropChange, Revision, RevisionGroup,
    RevisionGroupKind, RevisionKind, Run, RunProperties,
};

use crate::ReviewParagraphState;

const DOCUMENT: u64 = 7;
const SENDER: u64 = 9;
const OTHER: u64 = 11;

fn id(space: u64, counter: u64) -> NodeId {
    NodeId::from_parts(space, counter).expect("a node id")
}

fn run(node: NodeId, text: &str) -> InlineNode {
    InlineNode::Run(Run {
        id: node,
        properties: RunProperties::default().into(),
        text: text.to_owned(),
    })
}

fn insertion(node: NodeId, author: &str, inner: Vec<InlineNode>) -> InlineNode {
    InlineNode::Revision(Box::new(Revision {
        id: node,
        kind: RevisionKind::Insertion,
        author: Some(author.to_owned()),
        date: Some("2026-10-24T00:00:00Z".to_owned()),
        revision_id: None,
        editor_group: None,
        inlines: inner,
    }))
}

fn comment(author: &str, initials: &str) -> Comment {
    Comment {
        author: Some(author.to_owned()),
        initials: Some(initials.to_owned()),
        date: Some("2026-10-24T00:00:00Z".to_owned()),
        ..Comment::default()
    }
}

fn ada_comment_id() -> CommentId {
    CommentId::new(id(DOCUMENT, 21))
}

/// Paragraph 10 holds Ada's suggestion (node 30) and is anchored to Ada's comment (21).
fn held_inlines() -> Vec<InlineNode> {
    vec![
        InlineNode::CommentRangeStart(CommentRangeStart {
            id: id(DOCUMENT, 20),
            comment: ada_comment_id(),
        }),
        run(id(DOCUMENT, 11), "abc"),
        insertion(
            id(DOCUMENT, 30),
            "Ada Lovelace",
            vec![run(id(DOCUMENT, 31), "def")],
        ),
        InlineNode::CommentRangeEnd(CommentRangeEnd {
            id: id(DOCUMENT, 22),
            comment: ada_comment_id(),
        }),
    ]
}

fn document() -> Document {
    let mut ids = IdGenerator::new(1);
    let document_id = ids.next_id().expect("an id");
    let mut definitions = Definitions::default();
    definitions
        .comments
        .insert(ada_comment_id(), comment("Ada Lovelace", "AL"));
    Document::new(
        document_id,
        vec![BlockNode::Paragraph(Paragraph {
            id: id(DOCUMENT, 10),
            properties: ParagraphProperties::default().into(),
            inlines: held_inlines(),
        })],
        definitions,
    )
    .expect("a valid document")
}

fn bob() -> Stamp<'static> {
    Stamp {
        name: "Bob Stone",
        initials: "BS",
        space: IdSpace::new(SENDER),
    }
}

fn review(
    inlines: Vec<InlineNode>,
    comments: Option<DefinitionMap<CommentId, Comment>>,
) -> Operation {
    Operation::UpdateReviewState {
        paragraphs: vec![ReviewParagraphState {
            node: id(DOCUMENT, 10),
            inlines,
        }],
        comments,
    }
}

fn comments_of(operation: &Operation) -> &DefinitionMap<CommentId, Comment> {
    let Operation::UpdateReviewState {
        comments: Some(comments),
        ..
    } = operation
    else {
        panic!("a review operation carrying comments");
    };
    comments
}

fn authors(inlines: &[InlineNode], out: &mut Vec<(NodeId, Option<String>)>) {
    for inline in inlines {
        if let Some((_, node, _, author)) = mark_of(inline) {
            out.push((node, author.cloned()));
        }
        // container-set: inline-in-inline only, matching what `conform` descends into.
        if let Some(children) = contained_inlines(inline) {
            authors(children, out);
        }
    }
}

fn marks_of(operation: &Operation) -> Vec<(NodeId, Option<String>)> {
    let mut out = Vec::new();
    match operation {
        Operation::UpdateReviewState { paragraphs, .. } => {
            for state in paragraphs {
                authors(&state.inlines, &mut out);
            }
        }
        Operation::InsertBlocks { blocks, .. } => {
            for block in blocks {
                if let BlockNode::Paragraph(paragraph) = block {
                    authors(&paragraph.inlines, &mut out);
                }
            }
        }
        _ => panic!("an operation this file builds"),
    }
    out
}

/// The forgery `docs/175` §5.1 is about: a NEW comment, minted by the sender, claiming to be
/// somebody else's. It takes the stamp's name and initials.
#[test]
fn a_new_comment_under_somebody_elses_name_takes_the_senders() {
    let document = document();
    let forged = CommentId::new(id(SENDER, 40));
    let mut comments = document.definitions().comments.clone();
    comments.insert(forged, comment("Ada Lovelace", "AL"));
    let mut operation = review(held_inlines(), Some(comments));
    let conformed = conform(&document, &mut operation, &bob());
    let stamped = comments_of(&operation)
        .get(&forged)
        .expect("the new comment");
    assert_eq!(stamped.author.as_deref(), Some("Bob Stone"));
    assert_eq!(stamped.initials.as_deref(), Some("BS"));
    // The date is the author's, kept as written (`docs/175` §5.2).
    assert_eq!(stamped.date.as_deref(), Some("2026-10-24T00:00:00Z"));
    assert_eq!(conformed.restamped, 1);
}

/// An operation that rewrites an EXISTING comment's author has that part put back, and the rest
/// of it still applies.
#[test]
fn an_existing_comments_author_cannot_be_rewritten() {
    let document = document();
    let mut comments = document.definitions().comments.clone();
    let mut rewritten = comment("Bob Stone", "BS");
    rewritten.done = true;
    comments.insert(ada_comment_id(), rewritten);
    let mut operation = review(held_inlines(), Some(comments));
    let conformed = conform(&document, &mut operation, &bob());
    let kept = comments_of(&operation)
        .get(&ada_comment_id())
        .expect("Ada's comment");
    assert_eq!(kept.author.as_deref(), Some("Ada Lovelace"));
    assert_eq!(kept.initials.as_deref(), Some("AL"));
    assert!(kept.done, "the rest of the operation still applies");
    assert_eq!(conformed.restored, 1);
}

/// A new suggestion minted by the sender under somebody else's name takes the sender's.
#[test]
fn a_new_suggestion_under_somebody_elses_name_takes_the_senders() {
    let document = document();
    let mut inlines = held_inlines();
    inlines.insert(
        2,
        insertion(
            id(SENDER, 50),
            "Ada Lovelace",
            vec![run(id(SENDER, 51), "x")],
        ),
    );
    let mut operation = review(inlines, None);
    conform(&document, &mut operation, &bob());
    let marks = marks_of(&operation);
    assert!(
        marks.contains(&(id(SENDER, 50), Some("Bob Stone".to_owned()))),
        "{marks:?}"
    );
    assert!(
        marks.contains(&(id(DOCUMENT, 30), Some("Ada Lovelace".to_owned()))),
        "Ada's own suggestion keeps her name: {marks:?}"
    );
}

/// An existing suggestion's author cannot be rewritten either.
#[test]
fn an_existing_suggestions_author_cannot_be_rewritten() {
    let document = document();
    let mut inlines = held_inlines();
    inlines[2] = insertion(
        id(DOCUMENT, 30),
        "Bob Stone",
        vec![run(id(DOCUMENT, 31), "def")],
    );
    let mut operation = review(inlines, None);
    let conformed = conform(&document, &mut operation, &bob());
    assert_eq!(
        marks_of(&operation),
        vec![(id(DOCUMENT, 30), Some("Ada Lovelace".to_owned()))]
    );
    assert_eq!(conformed.restored, 1);
}

/// An undo of "accept" puts back a suggestion somebody else minted. It is not held (accepting
/// removed it) and not in the sender's space, so it keeps the author it carries — otherwise the
/// person pressing undo would take the suggestion over.
#[test]
fn a_restored_suggestion_minted_elsewhere_keeps_its_author() {
    let document = document();
    let mut inlines = held_inlines();
    inlines.insert(
        2,
        insertion(id(OTHER, 60), "Cleo Ng", vec![run(id(OTHER, 61), "y")]),
    );
    let mut operation = review(inlines, None);
    let conformed = conform(&document, &mut operation, &bob());
    assert!(marks_of(&operation).contains(&(id(OTHER, 60), Some("Cleo Ng".to_owned()))));
    assert!(!conformed.changed());
}

/// An honest sender's operation is already conformed, so conforming it changes nothing — the
/// property that keeps an honest replica's optimistic state equal to what everyone else
/// computes, with nothing rolled back.
#[test]
fn an_honest_operation_is_a_no_op() {
    let document = document();
    let fresh = CommentId::new(id(SENDER, 40));
    let mut comments = document.definitions().comments.clone();
    comments.insert(fresh, comment("Bob Stone", "BS"));
    let mut inlines = held_inlines();
    inlines.insert(
        2,
        insertion(id(SENDER, 50), "Bob Stone", vec![run(id(SENDER, 51), "x")]),
    );
    let honest = review(inlines, Some(comments));
    let mut operation = honest.clone();
    assert_eq!(
        conform(&document, &mut operation, &bob()),
        Conformed::default()
    );
    assert_eq!(operation, honest);
}

/// Two participants with one display name are kept apart by id, never by renaming one: the
/// stored author is the name exactly as given, with no "(2)" (`docs/175` §12 Q3).
#[test]
fn a_duplicate_display_name_is_stored_exactly_as_given() {
    let document = document();
    let twin = Stamp {
        name: "Ada Lovelace",
        initials: "AL",
        space: IdSpace::new(SENDER),
    };
    let fresh = CommentId::new(id(SENDER, 40));
    let mut comments = document.definitions().comments.clone();
    comments.insert(fresh, comment("Ada Lovelace (2)", "AL"));
    let mut operation = review(held_inlines(), Some(comments));
    conform(&document, &mut operation, &twin);
    assert_eq!(
        comments_of(&operation)
            .get(&fresh)
            .and_then(|c| c.author.as_deref()),
        Some("Ada Lovelace")
    );
}

/// A tracked format change authored with the action's own group, minted by the sender.
#[test]
fn a_new_format_suggestion_takes_the_senders_name() {
    let document = document();
    let mut inlines = held_inlines();
    let properties = RunProperties {
        prop_change: Some(Box::new(PropChange {
            author: Some("Ada Lovelace".to_owned()),
            date: None,
            revision_id: None,
            editor_group: Some(RevisionGroup {
                id: id(SENDER, 70),
                kind: RevisionGroupKind::Formatting,
            }),
            prior: Box::new(RunProperties::default()),
        })),
        ..RunProperties::default()
    };
    inlines[1] = InlineNode::Run(Run {
        id: id(DOCUMENT, 11),
        properties: properties.into(),
        text: "abc".to_owned(),
    });
    let mut operation = review(inlines, None);
    conform(&document, &mut operation, &bob());
    assert!(
        marks_of(&operation).contains(&(id(DOCUMENT, 11), Some("Bob Stone".to_owned()))),
        "{:?}",
        marks_of(&operation)
    );
}

/// A pasted tracked change is minted fresh by the paster, so in a room it takes the paster's
/// name — the owner's decision, and a known difference from Word (`docs/175` §12 Q5).
#[test]
fn a_pasted_tracked_change_takes_the_pasters_name() {
    let document = document();
    let mut operation = Operation::InsertBlocks {
        container: None,
        index: 1,
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: id(SENDER, 80),
            properties: ParagraphProperties::default().into(),
            inlines: vec![insertion(
                id(SENDER, 81),
                "Ada Lovelace",
                vec![run(id(SENDER, 82), "pasted")],
            )],
        })],
    };
    conform(&document, &mut operation, &bob());
    assert_eq!(
        marks_of(&operation),
        vec![(id(SENDER, 81), Some("Bob Stone".to_owned()))]
    );
}

#[test]
fn initials_come_from_the_first_two_words() {
    assert_eq!(derive_initials("Ada Lovelace"), "AL");
    assert_eq!(derive_initials("  ada   byron lovelace "), "AB");
    assert_eq!(derive_initials("Ada"), "A");
    assert_eq!(derive_initials(""), "");
}
