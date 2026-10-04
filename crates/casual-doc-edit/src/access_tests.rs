// SPDX-License-Identifier: Apache-2.0

//! Guards for a participant's access level.
//!
//! Every one of these **creates the condition** rather than inheriting it: a guard that passes
//! because the fixture happened to be read-only tells you nothing (`SKILL` §4). The mutations
//! that drove each of them red are recorded in the commit message.

use super::*;

use casual_doc_model::IdGenerator;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, CommentId, CommentRangeEnd, CommentRangeStart, Definitions, DocumentProtection,
    DocumentProtectionEdit, InlineNode, Paragraph, ParagraphProperties, Revision, RevisionKind,
    Run, RunProperties,
};

use crate::protection::{ProtectionRefusal, exempt_from_protection, refuse_if_protected};
use crate::{Pos, Range, ReviewParagraphState};

fn n(counter: u64) -> NodeId {
    NodeId::from_parts(7, counter).expect("a node id")
}

fn run(id: u64, text: &str) -> InlineNode {
    InlineNode::Run(Run {
        id: n(id),
        properties: RunProperties::default().into(),
        text: text.to_owned(),
    })
}

/// One paragraph reading `abcdefgh`, protected at `edit` when `protection` says so.
fn document(protection: Option<DocumentProtectionEdit>) -> Document {
    let mut ids = IdGenerator::new(1);
    let document_id = ids.next_id().expect("an id");
    let mut definitions = Definitions::default();
    definitions.settings.document_protection = protection.map(|edit| DocumentProtection {
        edit,
        enforcement: true,
        formatting: false,
    });
    Document::new(
        document_id,
        vec![BlockNode::Paragraph(Paragraph {
            id: n(10),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(11, "abcdefgh")],
        })],
        definitions,
    )
    .expect("a valid document")
}

fn typing() -> Operation {
    Operation::InsertText {
        at: Pos::new(n(10), 0),
        text: "x".to_owned(),
    }
}

fn deleting() -> Operation {
    Operation::DeleteText {
        range: Range {
            start: Pos::new(n(10), 0),
            end: Pos::new(n(10), 1),
        },
    }
}

/// An `UpdateReviewState` replacing paragraph 10's inlines with `inlines`.
fn review(inlines: Vec<InlineNode>) -> Operation {
    Operation::UpdateReviewState {
        paragraphs: vec![ReviewParagraphState {
            node: n(10),
            inlines,
        }],
        comments: None,
    }
}

fn lift() -> Operation {
    Operation::SetDocumentProtection { protection: None }
}

fn impose(edit: DocumentProtectionEdit) -> Operation {
    Operation::SetDocumentProtection {
        protection: Some(DocumentProtection {
            edit,
            enforcement: true,
            formatting: false,
        }),
    }
}

/// The text unchanged, wrapped in a comment range and nothing else — a comment anchor.
fn anchored_comment() -> Operation {
    review(vec![
        InlineNode::CommentRangeStart(CommentRangeStart {
            id: n(20),
            comment: CommentId::new(n(21)),
        }),
        run(11, "abcdefgh"),
        InlineNode::CommentRangeEnd(CommentRangeEnd {
            id: n(22),
            comment: CommentId::new(n(21)),
        }),
    ])
}

/// The same one paragraph, already **holding** the suggestion [`tracked_suggestion`] proposes.
///
/// The condition is created rather than inherited, which is the rule every guard in this file
/// follows: on [`document`] there is nothing to accept or reject, so an accept/reject matrix
/// written against it would be measuring the absence of a revision and not a participant's
/// right to decide one.
fn document_holding_a_suggestion() -> Document {
    let mut document = document(None);
    if let Some(BlockNode::Paragraph(paragraph)) = document.body_mut().first_mut() {
        paragraph.inlines = vec![
            run(11, "abcdefgh"),
            InlineNode::Revision(Box::new(Revision {
                id: n(30),
                kind: RevisionKind::Insertion,
                author: Some("Reviewer".to_owned()),
                date: None,
                revision_id: None,
                editor_group: None,
                inlines: vec![run(31, "!")],
            })),
        ];
    }
    document
}

/// Accepting the suggestion [`document_holding_a_suggestion`] holds: the text goes plain.
///
/// Run 31 keeps its id, which is not cosmetic — the projections compare inlines by full
/// equality, `NodeId` included, so an accept that re-minted the run would not be recognised as
/// an accept at all. `protection`'s own guards assert that sensitivity and that
/// `is_tracked_only` has shared it since ADR-052.
fn accept_the_suggestion() -> Operation {
    review(vec![run(11, "abcdefgh"), run(31, "!")])
}

/// Rejecting it: the revision and its text go together, leaving the before-state.
fn reject_the_suggestion() -> Operation {
    review(vec![run(11, "abcdefgh")])
}

/// The same text with one character proposed as a tracked insertion — a suggestion.
fn tracked_suggestion() -> Operation {
    review(vec![
        run(11, "abcdefgh"),
        InlineNode::Revision(Box::new(Revision {
            id: n(30),
            kind: RevisionKind::Insertion,
            author: Some("Reviewer".to_owned()),
            date: None,
            revision_id: None,
            editor_group: None,
            inlines: vec![run(31, "!")],
        })),
    ])
}

/// One character typed with no tracked mark at all, carried by the review vehicle.
fn untracked_typing_in_the_review_vehicle() -> Operation {
    review(vec![run(11, "abcdefgh!")])
}

#[test]
fn a_room_s_guest_cannot_lift_a_restriction_and_a_local_reader_still_can() {
    // The defect `152` §10 Q5 recorded and ADR-059 named: "anyone who can open a document can
    // lift its restriction, which is correct for an unpassworded Word restriction and wrong for
    // a room". BOTH halves are asserted, because fixing one by breaking the other is the easy
    // mistake — a local reader must keep the authority ADR-052 gave them, or `readOnly` becomes
    // a one-way door again.
    let document = document(Some(DocumentProtectionEdit::ReadOnly));
    for op in [lift(), impose(DocumentProtectionEdit::Comments)] {
        // The standalone mode, unchanged: no session, no grant, the reader holds the bytes.
        assert!(
            refuse_if_not_permitted(
                Some(&document),
                std::slice::from_ref(&op),
                Capabilities::local()
            )
            .is_ok(),
            "a local reader must still be able to change their own document's protection"
        );
        assert!(
            exempt_from_protection(&op, Capabilities::local()),
            "and the exemption must still fire for them"
        );
        assert_eq!(
            refuse_if_protected(&document, std::slice::from_ref(&op), Capabilities::local()),
            Ok(())
        );

        // The room, which is the half that did not exist.
        for (role, capabilities) in [
            ("a read-only guest", Capabilities::viewer()),
            ("a commenter", Capabilities::commenter()),
            ("a suggester", Capabilities::suggester()),
            (
                "an editor without the protection right",
                Capabilities::editor(),
            ),
        ] {
            assert_eq!(
                refuse_if_not_permitted(Some(&document), std::slice::from_ref(&op), capabilities),
                Err(AccessRefusal::NoProtectionChange),
                "{role} was allowed to change the document's restriction"
            );
            assert!(
                !exempt_from_protection(&op, capabilities),
                "{role} is exempt from the restriction, so the two checks can be reordered \
                 into a hole"
            );
            // The inner gate refuses it on its own, which is what makes the ordering safe.
            assert_eq!(
                refuse_if_protected(&document, std::slice::from_ref(&op), capabilities),
                Err(ProtectionRefusal::ReadOnly),
                "{role}'s protection change got past the document's own policy as well"
            );
        }

        // An owner may, which is the point of having the capability at all.
        assert!(
            refuse_if_not_permitted(
                Some(&document),
                std::slice::from_ref(&op),
                Capabilities::owner()
            )
            .is_ok()
        );
        assert_eq!(
            refuse_if_protected(&document, std::slice::from_ref(&op), Capabilities::owner()),
            Ok(())
        );
    }
}

#[test]
fn a_read_only_participant_is_refused_on_an_unprotected_document() {
    // Why the grant is the OUTER gate. The document here asks for nothing at all — no
    // `w:documentProtection` — so `refuse_if_protected` allows everything, and if the access
    // check were folded inside it, or run after it and short-circuited, a read-only guest would
    // be able to type.
    let document = document(None);
    assert!(
        document
            .definitions()
            .settings
            .document_protection
            .is_none(),
        "the condition this guard needs is an UNPROTECTED document: the whole point is that the \
         participant is refused anyway"
    );
    for op in [typing(), deleting(), anchored_comment()] {
        assert_eq!(
            refuse_if_protected(&document, std::slice::from_ref(&op), Capabilities::viewer()),
            Ok(()),
            "the document's own policy must allow {op:?}, or this guard measures the wrong thing"
        );
        assert_eq!(
            refuse_if_not_permitted(
                Some(&document),
                std::slice::from_ref(&op),
                Capabilities::viewer()
            ),
            Err(AccessRefusal::ReadOnly),
            "a read-only participant was allowed to send {op:?}"
        );
        assert!(
            refuse_if_not_permitted(
                Some(&document),
                std::slice::from_ref(&op),
                Capabilities::editor()
            )
            .is_ok(),
            "an editor must still be allowed {op:?}"
        );
    }
}

#[test]
fn comment_suggest_and_type_travel_as_one_operation_and_are_told_apart_exactly() {
    // ADR-052's hard part, now as an ACCESS question. All three of these are
    // `UpdateReviewState`, so a rule that looked at the variant would have to allow or forbid
    // all three; the projections decide it exactly, with no heuristic.
    let document = document(None);
    let cases = [
        ("a comment anchor", anchored_comment()),
        ("a tracked suggestion", tracked_suggestion()),
        ("untracked typing", untracked_typing_in_the_review_vehicle()),
    ];
    // Rows are capabilities, columns are the three gestures. Written out rather than derived,
    // so a change of behaviour is a change of table and not a change of expectation.
    let expected = [
        (Capabilities::viewer(), [false, false, false]),
        (Capabilities::commenter(), [true, false, false]),
        (Capabilities::suggester(), [true, true, false]),
        // A reviewer reads exactly like a commenter on these three gestures, and that is the
        // row that says the class is DISJOINT from `suggest` rather than a rung above it: a
        // reviewer may not author the suggestion in column two. What a reviewer may do that a
        // commenter may not needs a document that already holds a suggestion, which is
        // `a_reviewer_decides_what_it_did_not_author_and_a_suggester_does_not`.
        (Capabilities::reviewer(), [true, false, false]),
        (Capabilities::editor(), [true, true, true]),
    ];
    for (capabilities, allowed) in expected {
        for ((what, op), should_allow) in cases.iter().zip(allowed) {
            let answer =
                refuse_if_not_permitted(Some(&document), std::slice::from_ref(op), capabilities);
            assert_eq!(
                answer.is_ok(),
                should_allow,
                "{capabilities:?} and {what}: expected allowed={should_allow}, got {answer:?}"
            );
        }
    }
}

#[test]
fn a_reviewer_decides_what_it_did_not_author_and_a_suggester_does_not() {
    // The `review` class, as an access matrix over a document that ALREADY holds a suggestion —
    // the one fixture on which accept and reject are expressible at all. Five roles × two
    // directions, written out rather than derived, so a change of behaviour is a change of table.
    //
    // The two interesting rows are adjacent and opposite: a **suggester** may author the
    // suggestion and may not decide it; a **reviewer** may decide it and may not author it. That
    // is why `reviewer()` is deliberately not a superset of `suggester()`, and why `review` is
    // its own clause in `refuse_if_not_permitted` rather than an ordering of the others.
    let document = document_holding_a_suggestion();
    let cases = [
        ("accepting it", accept_the_suggestion()),
        ("rejecting it", reject_the_suggestion()),
    ];
    let expected = [
        (Capabilities::viewer(), [false, false]),
        (Capabilities::commenter(), [false, false]),
        (Capabilities::suggester(), [false, false]),
        (Capabilities::reviewer(), [true, true]),
        (Capabilities::editor(), [true, true]),
    ];
    for (capabilities, allowed) in expected {
        for ((what, op), should_allow) in cases.iter().zip(allowed) {
            let answer =
                refuse_if_not_permitted(Some(&document), std::slice::from_ref(op), capabilities);
            assert_eq!(
                answer.is_ok(),
                should_allow,
                "{capabilities:?} and {what}: expected allowed={should_allow}, got {answer:?}"
            );
        }
    }

    // The mirror half, on the same document: a reviewer may not author a suggestion of its own
    // even where one already exists, and a suggester may. Without this the table above could be
    // satisfied by a `review` bit that simply meant `suggest` on a different fixture.
    assert_eq!(
        refuse_if_not_permitted(
            Some(&document),
            &[review(vec![
                run(11, "abcdefgh"),
                InlineNode::Revision(Box::new(Revision {
                    id: n(30),
                    kind: RevisionKind::Insertion,
                    author: Some("Reviewer".to_owned()),
                    date: None,
                    revision_id: None,
                    editor_group: None,
                    inlines: vec![run(31, "!")],
                })),
                InlineNode::Revision(Box::new(Revision {
                    id: n(40),
                    kind: RevisionKind::Insertion,
                    author: Some("Someone else".to_owned()),
                    date: None,
                    revision_id: None,
                    editor_group: None,
                    inlines: vec![run(41, "?")],
                })),
            ])],
            Capabilities::reviewer()
        ),
        Err(AccessRefusal::ReviewOnly),
        "a reviewer authored a tracked change of its own: the class is `accept/reject and \
         moderate comments`, and a preset that granted the wider right silently is how this \
         capability spent its first increment"
    );

    // And the refusal a reviewer gets names what a reviewer may do, rather than borrowing a
    // sentence that would misdescribe the class.
    assert_eq!(
        refuse_if_not_permitted(
            Some(&document),
            &[untracked_typing_in_the_review_vehicle()],
            Capabilities::reviewer()
        ),
        Err(AccessRefusal::ReviewOnly)
    );
}

#[test]
fn the_review_only_refusal_is_reachable() {
    // `AccessRefusal::ReviewOnly` shipped without this, and a refusal variant nothing can produce
    // is a `session.*` code a host routes and never sees — the "built is not reachable" failure
    // the working contract names, inside a permission vocabulary where it is worst. So: the
    // narrowest grant that holds `review` and nothing else, on an ordinary keystroke.
    let bare_reviewer = Capabilities::viewer().with_review();
    assert!(bare_reviewer.may_review());
    assert!(
        !bare_reviewer.may_comment() && !bare_reviewer.may_suggest() && !bare_reviewer.may_edit(),
        "the point of this grant is that `review` is the ONLY thing it holds, or the refusal \
         below could be charged to another class"
    );
    assert!(
        bare_reviewer.may_write(),
        "`review` must count as a write at the relay, which holds no document and can only ask \
         whether this participant changes the document at all"
    );
    let document = document(None);
    for op in [typing(), deleting()] {
        assert_eq!(
            refuse_if_not_permitted(Some(&document), std::slice::from_ref(&op), bare_reviewer),
            Err(AccessRefusal::ReviewOnly),
            "{op:?} from a review-only participant must be refused AS review-only"
        );
    }
    // `reviewer()` itself reports the same class, because `comment` is not the widest write
    // class it holds — the ordering in `refusal_for` is what decides this, and it is asserted
    // rather than left to be read off the `if`/`else` chain.
    assert_eq!(
        refuse_if_not_permitted(Some(&document), &[typing()], Capabilities::reviewer()),
        Err(AccessRefusal::ReviewOnly),
        "a reviewer was told they may only comment, which names the narrower of the two rights \
         they hold"
    );
    // And a participant holding BOTH is told about suggesting, which `refusal_for` documents as
    // deliberate: it is the class whose gestures are the commoner ones.
    assert_eq!(
        refuse_if_not_permitted(
            Some(&document),
            &[typing()],
            Capabilities::suggester().with_review()
        ),
        Err(AccessRefusal::SuggestionsOnly)
    );
}

#[test]
fn the_relay_s_document_free_answer_never_refuses_what_a_replica_allows() {
    // The one property that makes two amounts of information safe to judge with one rule: the
    // relay's answer is strictly weaker. If it ever refused something a replica allows, a
    // legitimate commenter's comment would be dropped by the network with nothing able to
    // explain it.
    let document = document(None);
    let candidates = [
        typing(),
        deleting(),
        lift(),
        impose(DocumentProtectionEdit::ReadOnly),
        anchored_comment(),
        tracked_suggestion(),
        untracked_typing_in_the_review_vehicle(),
        // The review decisions too, which are the operations `review` exists for. They are
        // judged against the suggestion-holding document below, because against this one they
        // decide nothing and the property would be asserted over a gesture that cannot happen.
        accept_the_suggestion(),
        reject_the_suggestion(),
    ];
    let holding_a_suggestion = document_holding_a_suggestion();
    for capabilities in [
        Capabilities::viewer(),
        Capabilities::commenter(),
        Capabilities::suggester(),
        Capabilities::reviewer(),
        Capabilities::editor(),
        Capabilities::owner(),
    ] {
        for op in &candidates {
            for replica_document in [&document, &holding_a_suggestion] {
                let replica = refuse_if_not_permitted(
                    Some(replica_document),
                    std::slice::from_ref(op),
                    capabilities,
                );
                let relay = refuse_if_not_permitted(None, std::slice::from_ref(op), capabilities);
                if replica.is_ok() {
                    assert!(
                        relay.is_ok(),
                        "the relay refused {op:?} for {capabilities:?} while a replica allows \
                         it: the weaker answer is not weaker"
                    );
                }
            }
            let replica =
                refuse_if_not_permitted(Some(&document), std::slice::from_ref(op), capabilities);
            let relay = refuse_if_not_permitted(None, std::slice::from_ref(op), capabilities);
            if replica.is_ok() {
                assert!(
                    relay.is_ok(),
                    "the relay refused {op:?} for {capabilities:?} while a replica allows it: the \
                     weaker answer is not weaker"
                );
            }
        }
    }

    // And the harness must be able to SEE the asymmetry it asserts about, or it proves only
    // that the two answers happen to agree everywhere. Here is the case where they differ.
    let sneaky = untracked_typing_in_the_review_vehicle();
    assert!(
        refuse_if_not_permitted(
            None,
            std::slice::from_ref(&sneaky),
            Capabilities::commenter()
        )
        .is_ok(),
        "the relay cannot tell typing from commenting without a document, and this guard is \
         worthless if the two answers never differ"
    );
    assert!(
        refuse_if_not_permitted(
            Some(&document),
            std::slice::from_ref(&sneaky),
            Capabilities::commenter()
        )
        .is_err(),
        "a replica can, which is the asymmetry the property is about"
    );
    // The relay's one real line, which it CAN hold: a viewer writes nothing at all.
    for op in &candidates {
        assert!(
            refuse_if_not_permitted(None, std::slice::from_ref(op), Capabilities::viewer())
                .is_err(),
            "the relay must refuse {op:?} from a read-only participant with no document in hand"
        );
    }
}

#[test]
fn a_batch_is_judged_on_its_worst_operation() {
    // ADR-052's batch rule, for the access authority too: otherwise a permitted comment carries
    // an edit in behind it. Asserted in both orders, because a check that stopped at the first
    // answer would pass one of them.
    let document = document(None);
    let permitted = anchored_comment();
    assert!(
        refuse_if_not_permitted(
            Some(&document),
            std::slice::from_ref(&permitted),
            Capabilities::commenter()
        )
        .is_ok(),
        "the permitted operation alone must be permitted, or the batch guard proves nothing"
    );
    for batch in [
        vec![permitted.clone(), typing()],
        vec![typing(), permitted.clone()],
    ] {
        assert_eq!(
            refuse_if_not_permitted(Some(&document), &batch, Capabilities::commenter()),
            Err(AccessRefusal::CommentsOnly),
            "a batch smuggled a keystroke past a commenter's grant"
        );
    }
}

#[test]
fn a_grant_can_only_ever_narrow() {
    // `143` §10: a host narrows a role rather than inventing a vocabulary, and a grant change
    // must not be able to restore a revoked right. Intersection is the only composition offered.
    let owner = Capabilities::owner();
    let narrowed = owner.narrowed_to(Capabilities::commenter());
    assert!(narrowed.may_comment());
    assert!(!narrowed.may_suggest());
    assert!(!narrowed.may_edit());
    assert!(!narrowed.may_manage_protection());
    assert_eq!(
        narrowed.narrowed_to(owner),
        narrowed,
        "re-applying a wider grant must not widen anything"
    );
    assert_eq!(
        Capabilities::viewer().narrowed_to(Capabilities::owner()),
        Capabilities::viewer()
    );
    assert!(
        !Capabilities::viewer().may_write(),
        "a viewer must not be able to write — that is the one line a relay with no document holds"
    );
    for capabilities in [
        Capabilities::commenter(),
        Capabilities::suggester(),
        Capabilities::editor(),
        Capabilities::owner(),
    ] {
        assert!(
            capabilities.may_write(),
            "{capabilities:?} should count as a writer"
        );
    }
    // The presets are a ladder, and each rung really adds something.
    assert_ne!(Capabilities::commenter(), Capabilities::viewer());
    assert_ne!(Capabilities::suggester(), Capabilities::commenter());
    assert_ne!(Capabilities::editor(), Capabilities::suggester());
    assert_ne!(Capabilities::owner(), Capabilities::editor());

    // Except that `reviewer` is NOT on that ladder, and the asymmetry is the role: a reviewer
    // resolves what it did not author. A preset granting more than its name says is how this
    // capability spent its first increment, so the two directions are pinned here rather than
    // left to be read off `reviewer()`'s body.
    assert!(
        !Capabilities::reviewer().may_suggest(),
        "`reviewer()` grants `suggest`: a reviewer would be able to author the suggestions it \
         exists to decide, and the preset would be wider than its name"
    );
    assert!(
        !Capabilities::suggester().may_review(),
        "`suggester()` grants `review`: a suggester would be able to accept its own proposal, \
         which is the whole point of separating the two"
    );
    // A host that wants both composes them, and the composition is commutative — a narrowing
    // vocabulary with an order-dependent union would not be one.
    assert_eq!(
        Capabilities::suggester().with_review(),
        Capabilities::reviewer().with_suggest()
    );
    // An editor holds `review` because it can already produce any state a reviewer can;
    // withholding the bit would make the bit a lie rather than a restriction.
    assert!(Capabilities::editor().may_review());
    assert!(
        Capabilities::reviewer().may_write(),
        "a reviewer must count as a writer, or the relay — which holds no document — would \
         refuse every decision it is admitted to make"
    );
}

#[test]
fn a_capability_a_peer_does_not_understand_is_not_granted() {
    // The serde direction rule: a missing field and an unknown one must both fail towards LESS
    // access. A permission that drifts open across a version boundary is the one direction that
    // cannot be allowed, and the wire is JSON (ADR-057) so both cases are expressible.
    let unknown: Capabilities =
        serde_json::from_str(r#"{"comment":true,"restructureEverything":true,"review":true}"#)
            .expect(
                "an unknown capability must be ignored, not refused: an old peer has to be \
                     able to read a newer grant at all",
            );
    assert!(unknown.may_comment());
    assert!(!unknown.may_suggest());
    assert!(!unknown.may_edit());
    assert!(!unknown.may_manage_protection());

    let absent: Capabilities = serde_json::from_str("{}").expect("every field defaults");
    assert_eq!(absent, Capabilities::viewer());
    assert!(!absent.may_write());

    // And the round trip is faithful, or a grant would narrow itself in transit.
    let granted = Capabilities::suggester().with_manage_protection();
    let encoded = serde_json::to_string(&granted).expect("serialisable");
    assert_eq!(
        serde_json::from_str::<Capabilities>(&encoded).expect("decodable"),
        granted
    );
}

#[test]
fn the_document_s_policy_and_the_participant_s_grant_are_enforced_independently() {
    // The two authorities, crossed. Four cells and four different answers: the whole reason
    // `152` §10 Q5 says they must not be collapsed into one.
    let unprotected = document(None);
    let protected = document(Some(DocumentProtectionEdit::ReadOnly));
    let ops = [typing()];

    // Unprotected + editor: allowed by both.
    assert!(refuse_if_not_permitted(Some(&unprotected), &ops, Capabilities::editor()).is_ok());
    assert_eq!(
        refuse_if_protected(&unprotected, &ops, Capabilities::editor()),
        Ok(())
    );

    // Unprotected + viewer: the grant refuses and the document does not.
    assert!(refuse_if_not_permitted(Some(&unprotected), &ops, Capabilities::viewer()).is_err());
    assert_eq!(
        refuse_if_protected(&unprotected, &ops, Capabilities::viewer()),
        Ok(())
    );

    // Protected + editor: the document refuses and the grant does not.
    assert!(refuse_if_not_permitted(Some(&protected), &ops, Capabilities::editor()).is_ok());
    assert_eq!(
        refuse_if_protected(&protected, &ops, Capabilities::editor()),
        Err(ProtectionRefusal::ReadOnly)
    );

    // Protected + viewer: both refuse, and with different routing codes, so a host can tell the
    // reader which of the two it was — one of them they can do something about.
    let grant = refuse_if_not_permitted(Some(&protected), &ops, Capabilities::viewer())
        .expect_err("a viewer is refused");
    let policy = refuse_if_protected(&protected, &ops, Capabilities::viewer())
        .expect_err("a protected document refuses");
    assert_ne!(
        grant.reason(),
        policy.reason(),
        "a read-only guest and a protected document must not produce the same sentence"
    );
    assert!(grant.reason().contains("session."));
    assert!(policy.reason().contains("document."));
}

#[test]
fn every_access_refusal_carries_a_distinct_routed_reason() {
    // `crate::refusal`'s contract: the sentence is a fallback and the code is what a host routes
    // through its own catalogue, so two refusals that share a code are two refusals a non-English
    // reader cannot tell apart.
    let all = [
        AccessRefusal::ReadOnly,
        AccessRefusal::CommentsOnly,
        AccessRefusal::SuggestionsOnly,
        AccessRefusal::ReviewOnly,
        AccessRefusal::NoProtectionChange,
    ];
    let mut codes = Vec::new();
    for refusal in all {
        let reason = refusal.reason();
        assert!(
            reason.starts_with(crate::refusal::MARKER),
            "{refusal:?} is not marked as already explained, so the host replaces it with the \
             generic sentence: {reason}"
        );
        let (_, code) = reason
            .split_once(crate::refusal::CODE_SEPARATOR)
            .expect("a routed reason carries its code");
        assert!(code.starts_with("session."), "{refusal:?} -> {code}");
        codes.push(code);
    }
    codes.sort_unstable();
    let before = codes.len();
    codes.dedup();
    assert_eq!(before, codes.len(), "two refusals share a routing code");
}

// ---------------------------------------------------------------------------------------------
// Changing somebody ELSE's access — `refuse_access_change`, `Capabilities::manage_access`.
//
// The owner's decision these exist for: "In case of co-editing we need a full rights-changing
// dialog for owner and editor — not viewer. And for SDK or single user, the role is pre-decided
// while loading the file." The engine's half of that is a capability that authorises the change
// and a rule that bounds it; the chrome's half is a surface, and a surface is not a boundary.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_request_above_the_target_s_own_host_signed_grant_is_refused_by_the_engine() {
    // THE INVARIANT, and the one a crafted client goes for: a room may redistribute the rights a
    // host has issued and may not mint one. The actor here is an owner — the most authority the
    // vocabulary has — and the target's own grant says `commenter`, so promoting them to an
    // editor is refused however legitimate the asker is.
    let refusal = refuse_access_change(
        Capabilities::owner(),
        false,
        Capabilities::commenter(),
        Capabilities::editor(),
    )
    .expect_err("an owner cannot exceed the host's grant for somebody else");
    assert_eq!(refusal, AccessChangeRefusal::AboveCeiling);

    // ...and the same request INSIDE that ceiling is allowed, so the guard is about the bound and
    // not about the function refusing everything. A commenter may be narrowed to a viewer.
    refuse_access_change(
        Capabilities::owner(),
        false,
        Capabilities::commenter(),
        Capabilities::viewer(),
    )
    .expect("narrowing inside the ceiling is the whole point of the feature");
    refuse_access_change(
        Capabilities::owner(),
        false,
        Capabilities::commenter(),
        Capabilities::commenter(),
    )
    .expect("restoring a participant to their own ceiling is inside it");
}

#[test]
fn nobody_may_hand_out_access_they_do_not_hold_themselves() {
    // A room where an editor may appoint an owner is a room in which owner is not a privilege.
    // The target's ceiling admits it — they were granted owner by the host — and the ACTOR is the
    // one who cannot, which is why this is a separate line from the ceiling check.
    let refusal = refuse_access_change(
        Capabilities::editor(),
        false,
        Capabilities::owner(),
        Capabilities::owner(),
    )
    .expect_err("an editor cannot grant `manageProtection` it does not hold");
    assert_eq!(refusal, AccessChangeRefusal::AboveActor);

    // The same editor restoring the same participant to `editor` is fine: inside both bounds.
    refuse_access_change(
        Capabilities::editor(),
        false,
        Capabilities::owner(),
        Capabilities::editor(),
    )
    .expect("an editor may move somebody to a level the editor itself holds");
}

#[test]
fn a_participant_who_may_not_manage_access_is_refused_before_a_target_is_read() {
    // Checked FIRST, deliberately: the later answers describe a target, and an unauthorised actor
    // must not be able to use them to probe one. Asserted by giving the call a request that would
    // produce `AboveCeiling` — if the order were wrong, that is the answer that would come back,
    // and it would tell the asker something about somebody else.
    for actor in [
        Capabilities::viewer(),
        Capabilities::commenter(),
        Capabilities::suggester(),
        Capabilities::reviewer(),
    ] {
        let refusal =
            refuse_access_change(actor, false, Capabilities::viewer(), Capabilities::editor())
                .expect_err("a participant without `manage_access` changes nobody");
        assert_eq!(refusal, AccessChangeRefusal::NotPermitted, "{actor:?}");
    }
}

#[test]
fn a_participant_may_not_change_their_own_access() {
    // Google Docs and Word both hold this line, and the reason is not escalation — the ceiling
    // rule already stops that, since a participant's ceiling is their own grant. It is that the
    // one change nobody can undo for you is the one that took your ability to make changes.
    let refusal = refuse_access_change(
        Capabilities::owner(),
        true,
        Capabilities::owner(),
        Capabilities::viewer(),
    )
    .expect_err("a sole owner must not be able to lock the room out with one click");
    assert_eq!(refusal, AccessChangeRefusal::OwnAccessUnchangeable);

    // And it is the SELF-ness that refuses, not the narrowing: the identical request about
    // somebody else is allowed.
    refuse_access_change(
        Capabilities::owner(),
        false,
        Capabilities::owner(),
        Capabilities::viewer(),
    )
    .expect("narrowing somebody else to a viewer is inside every bound");
}

#[test]
fn managing_access_is_not_permission_to_change_the_document() {
    // `may_write` is the one line a relay holding no document can hold, and `manage_access` is
    // deliberately not a term in it: changing what somebody else may do changes the ROOM and
    // leaves the bytes untouched. A participant holding only this must still be refused on the
    // keystroke path.
    let only_access = Capabilities::viewer().with_manage_access();
    assert!(!only_access.may_write());
    let document = document(None);
    let refusal = refuse_if_not_permitted(Some(&document), &[typing()], only_access)
        .expect_err("a rights manager who was granted no edit may not type");
    assert_eq!(refusal, AccessRefusal::ReadOnly);
    // Nor comment, suggest, decide a revision, or touch the document's own policy.
    assert!(!only_access.may_comment());
    assert!(!only_access.may_suggest());
    assert!(!only_access.may_review());
    assert!(!only_access.may_edit());
    assert!(!only_access.may_manage_protection());
}

#[test]
fn the_review_vehicle_is_not_admitted_by_manage_access_alone() {
    // `admitted_by` is read by `intersects`, so ANY capability in the value it returns admits the
    // operation. It used to spell the review vehicle's answer `Capabilities::editor()`, and the
    // day `manage_access` joined that preset is the day a rights manager holding nothing else
    // would have been admitted to send a review change. The four content classes are named there
    // now, and this is the guard that they stay named.
    // The one the file's own helper builds, so the guard exercises the shape the review path
    // really sends rather than one invented here.
    let vehicle = review(vec![run(11, "abcdefgh")]);
    let admitted = admitted_by(&vehicle);
    assert!(
        !admitted.may_manage_access(),
        "the review vehicle admits a participant who may only manage access"
    );
    let document = document(None);
    let refusal = refuse_if_not_permitted(
        Some(&document),
        &[vehicle],
        Capabilities::viewer().with_manage_access(),
    )
    .expect_err("managing access is not a licence to decide somebody's revision");
    assert_eq!(refusal, AccessRefusal::ReadOnly);
}

#[test]
fn the_rights_dialog_is_for_an_owner_and_an_editor_and_for_nobody_below() {
    // The owner's decision, read off the presets rather than off a comment: "a full
    // rights-changing dialog for owner and editor — not viewer."
    assert!(Capabilities::owner().may_manage_access());
    assert!(Capabilities::editor().may_manage_access());
    for below in [
        Capabilities::viewer(),
        Capabilities::commenter(),
        Capabilities::suggester(),
        Capabilities::reviewer(),
    ] {
        assert!(
            !below.may_manage_access(),
            "{below:?} would be offered the rights dialog"
        );
    }
    // Standalone is the owner's other half — "for SDK or single user, the role is pre-decided
    // while loading the file" — and `local()` says the local reader is the only authority. It
    // holds the capability because it holds all of them; what makes the dialog absent there is
    // that there is no room, which is the chrome's answer and `session_access.mjs`'s guard.
    assert!(Capabilities::local().may_manage_access());
}

#[test]
fn narrowing_cannot_restore_the_right_to_manage_access() {
    // Intersection is the only composition `Capabilities` offers, and the sixth field has to be
    // in it or a narrowed grant would carry a capability neither side granted. This is the shape
    // that went wrong for `review`: a field the engine modelled and a composition that did not
    // know about it.
    let narrowed = Capabilities::owner().narrowed_to(Capabilities::commenter());
    assert!(!narrowed.may_manage_access());
    let kept = Capabilities::owner().narrowed_to(Capabilities::editor());
    assert!(kept.may_manage_access());
    assert!(
        !Capabilities::viewer()
            .narrowed_to(Capabilities::owner())
            .may_manage_access()
    );
}

#[test]
fn every_access_change_refusal_carries_a_distinct_routed_reason() {
    // The same contract `every_access_refusal_carries_a_distinct_routed_reason` holds for the
    // other vocabulary, and for the same reason: two refusals sharing a code are two refusals a
    // non-English reader cannot tell apart. Held separately because the two enums are separate
    // vocabularies on purpose — one is about the document, one about the room.
    let all = [
        AccessChangeRefusal::NotPermitted,
        AccessChangeRefusal::OwnAccessUnchangeable,
        AccessChangeRefusal::AboveCeiling,
        AccessChangeRefusal::AboveActor,
        AccessChangeRefusal::NotAParticipant,
    ];
    let mut codes = Vec::new();
    for refusal in all {
        let reason = refusal.reason();
        assert!(
            reason.starts_with(crate::refusal::MARKER),
            "{refusal:?} is not marked as already explained: {reason}"
        );
        let (_, code) = reason
            .split_once(crate::refusal::CODE_SEPARATOR)
            .expect("a routed reason carries its code");
        assert!(code.starts_with("session."), "{refusal:?} -> {code}");
        codes.push(code);
    }
    codes.sort_unstable();
    let before = codes.len();
    codes.dedup();
    assert_eq!(before, codes.len(), "two refusals share a routing code");
}
