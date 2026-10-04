// SPDX-License-Identifier: Apache-2.0

//! Combining two branches offline — `107` 6.5, `109` OO-007.
//!
//! The guards here are written so that a combine which *looks* like it worked cannot pass. The
//! way this goes wrong is not a panic: it is a merged document that quietly holds one side's
//! work and not the other's, or holds both at coordinates that make nonsense of one of them. So
//! every guard reads the merged TEXT, and the two that matter read it against the live
//! two-replica path — if offline combine and the live session disagree, one of them is wrong and
//! this file is what says so.

use casual_doc_edit::{Operation, Pos};
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, Paragraph, ParagraphProperties, Run,
    RunProperties,
};
use casual_doc_model::{IdGenerator, NodeId};

use crate::protocol::ClientId;
use crate::wire::IdSpace;
use crate::{Coalesce, Commit, RevisionLog, Transaction, TransactionId};

use super::{Branch, CombineError, combine};

/// A document of `paragraphs` one-run paragraphs, each holding `abcdefgh`.
///
/// The same shape `session_tests` seeds, so a guard that compares offline combine against the
/// live path is comparing like with like.
fn seed(paragraphs: usize) -> (Document, Vec<NodeId>) {
    let mut ids = IdGenerator::new(7);
    let document_id = ids.next_id().expect("id");
    let mut blocks = Vec::new();
    let mut out = Vec::new();
    for _ in 0..paragraphs {
        let id = ids.next_id().expect("id");
        let run = ids.next_id().expect("id");
        out.push(id);
        blocks.push(BlockNode::Paragraph(Paragraph {
            id,
            properties: ParagraphProperties::default().into(),
            inlines: vec![InlineNode::Run(Run {
                id: run,
                properties: RunProperties::default().into(),
                text: "abcdefgh".to_owned(),
            })],
        }));
    }
    (
        Document::new(document_id, blocks, Definitions::default()).expect("a valid document"),
        out,
    )
}

fn plain_text(document: &Document) -> String {
    let mut text = String::new();
    for block in document.body() {
        if let BlockNode::Paragraph(paragraph) = block {
            for inline in &paragraph.inlines {
                if let InlineNode::Run(run) = inline {
                    text.push_str(&run.text);
                }
            }
            text.push('\n');
        }
    }
    text
}

/// One branch of a document, recorded the way a replica records its own work: its own
/// participant number, therefore its own identity space, therefore its own mints.
struct Branched {
    log: RevisionLog,
    ids: IdGenerator,
    document: Document,
    next: u128,
}

impl Branched {
    fn new(ancestor: &Document, client: ClientId) -> Self {
        let space = crate::wire::space_of(IdSpace::of_document(ancestor.id()), client)
            .expect("the participant number has a space");
        Self {
            log: RevisionLog::default(),
            ids: IdGenerator::new(space.get()),
            document: ancestor.clone(),
            next: 0,
        }
    }

    fn edit(&mut self, label: &'static str, operations: Vec<Operation>) {
        self.edit_coalescing(label, operations, Coalesce::New);
    }

    fn edit_coalescing(
        &mut self,
        label: &'static str,
        operations: Vec<Operation>,
        coalesce: Coalesce,
    ) {
        self.next += 1;
        let transaction = Transaction::reserve(
            TransactionId::new(self.next),
            self.log.head(),
            label,
            &mut self.ids,
            operations,
        )
        .expect("identity spaces")
        .coalescing(coalesce);
        self.log
            .apply(&mut self.document, transaction)
            .expect("the edit applies");
    }

    fn commits(&self) -> Vec<Commit> {
        self.log.commits().cloned().collect()
    }
}

fn typing(node: NodeId, at: u32, text: &str) -> Operation {
    Operation::InsertText {
        at: Pos::new(node, at),
        text: text.to_owned(),
    }
}

/// **Both branches' work is in the merged document, and neither is at the other's offsets.**
///
/// The simplest thing that can go wrong, and the one a smoke test misses: two insertions into
/// ONE paragraph at ONE offset. Applied without a transform the second lands where the first
/// already is, and the merged text reads `ZYabcdefgh` or `Zabcdefgh` depending on which side
/// won — both of which are a combine that lost or reordered somebody's work. The transform is
/// what makes it `ZYabcdefgh` *deterministically*, with the ordered side first.
#[test]
fn both_branches_edits_survive_a_combine_of_the_same_paragraph() {
    let (ancestor, paragraphs) = seed(3);
    let mut ours = Branched::new(&ancestor, ClientId::new(0));
    let mut theirs = Branched::new(&ancestor, ClientId::new(1));
    ours.edit("Typing", vec![typing(paragraphs[0], 0, "OUR")]);
    theirs.edit("Typing", vec![typing(paragraphs[0], 0, "THEIR")]);

    let our_commits = ours.commits();
    let their_commits = theirs.commits();
    let merged = combine(
        &ancestor,
        Branch {
            client: ClientId::new(0),
            commits: &our_commits,
        },
        Branch {
            client: ClientId::new(1),
            commits: &their_commits,
        },
    )
    .expect("two branches of one paragraph combine");

    let text = plain_text(&merged.document);
    assert!(
        text.contains("OUR"),
        "our branch is missing from the merged document: {text:?}"
    );
    assert!(
        text.contains("THEIR"),
        "their branch is missing from the merged document: {text:?}"
    );
    assert!(
        text.starts_with("THEIRabcdefgh") || text.starts_with("THEIROURabcdefgh"),
        "the ordered side must be applied as written, so the merged paragraph begins with \
         their text: {text:?}"
    );
    assert_eq!(
        merged.replayed, 1,
        "our one commit had to be rolled back and replayed over theirs; {} were",
        merged.replayed
    );
    assert!(
        merged.tombstones.is_empty(),
        "nothing addressed anything that stopped existing: {:?}",
        merged.tombstones
    );
    // The original ancestor is untouched: combine takes it by reference and builds a third
    // document, which is what makes a refused combine cost nothing.
    assert_eq!(plain_text(&ancestor), "abcdefgh\nabcdefgh\nabcdefgh\n");
}

/// **A multi-commit branch is rebased as a SEQUENCE, not commit by commit against a stale
/// base.**
///
/// This is the guard the module's "one mechanism" argument exists for. Transforming each of
/// their commits over our branch *as originally written* is correct for the first one and wrong
/// from the second onwards, because our branch has moved under it. Two commits each side is the
/// smallest case that can tell the difference, and the text it produces is the evidence.
#[test]
fn two_commits_each_side_rebase_as_sequences_and_every_character_survives() {
    let (ancestor, paragraphs) = seed(2);
    let mut ours = Branched::new(&ancestor, ClientId::new(0));
    let mut theirs = Branched::new(&ancestor, ClientId::new(1));
    // THE OFFSETS ARE CHOSEN SO THE TWO ORDERS DO NOT COMMUTE, and that took re-deriving: with
    // both sides typing at 0 and 1 the merged text is the same string whether or not the rebase
    // happened, because each side's run lands beside the other's either way. That is a guard
    // pinned to a coincidence rather than to the guarantee — the shape `SKILL` §4 keeps finding
    // here. Ours goes at the head and theirs into the middle, so "ours first, theirs on top"
    // and "theirs as written, ours rebased over it" produce visibly different documents.
    ours.edit("Typing", vec![typing(paragraphs[0], 0, "A")]);
    ours.edit("Typing", vec![typing(paragraphs[0], 1, "B")]);
    theirs.edit("Typing", vec![typing(paragraphs[0], 2, "x")]);
    theirs.edit("Typing", vec![typing(paragraphs[0], 3, "y")]);

    let our_commits = ours.commits();
    let their_commits = theirs.commits();
    let merged = combine(
        &ancestor,
        Branch {
            client: ClientId::new(0),
            commits: &our_commits,
        },
        Branch {
            client: ClientId::new(1),
            commits: &their_commits,
        },
    )
    .expect("two two-commit branches combine");

    let text = plain_text(&merged.document);
    for expected in ["A", "B", "x", "y"] {
        assert!(
            text.contains(expected),
            "{expected:?} was written on one of the branches and is not in the merge: {text:?}"
        );
    }
    // PINNED EXACTLY, and this is the assertion that does the work. The four characters being
    // *present* is satisfied by a merge that interleaved them wrongly — a combine with no
    // rebase at all produces `abOUTHEIRRcdefgh` on the one-commit fixture and still contains
    // every letter — and "each side's two characters are adjacent" survives it too. What cannot
    // survive it is the whole string: their branch applied into the middle as written, with
    // ours sitting at the head in the coordinates their branch left behind. Without the rebase
    // this reads `ABxyabcdefgh`, because their offsets 2 and 3 then count past our own letters
    // instead of past the document's.
    assert_eq!(
        text, "ABabxycdefgh\nabcdefgh\n",
        "the merge must read as their branch as written with ours rebased onto it"
    );
    // Each side's own order is preserved, spelt out because the pin above does not say WHY it
    // is the right string: `B` was typed after `A` and immediately to its right, and a
    // per-commit transform against a stale base is what separates them.
    let first = text.find('A').expect("A is present");
    let second = text.find('B').expect("B is present");
    assert_eq!(
        second,
        first + 1,
        "our second keystroke must still sit beside our first: {text:?}"
    );
    let theirs_first = text.find('x').expect("x is present");
    let theirs_second = text.find('y').expect("y is present");
    assert_eq!(
        theirs_second,
        theirs_first + 1,
        "their second keystroke must still sit beside their first: {text:?}"
    );
    assert_eq!(
        merged.replayed, 4,
        "two commits of ours, rolled back and replayed once per arrival of theirs"
    );
    assert_eq!(
        plain_text(&ancestor),
        "abcdefgh\nabcdefgh\n",
        "the ancestor was mutated"
    );
}

/// **Two branches that minted in the same space are refused, not merged.**
///
/// The honest boundary of this feature, and the reason it is stated in the module docs rather
/// than discovered. An offline fork — one file opened twice — mints both branches in
/// `IdSpace::local`, so the two sides genuinely can name different nodes identically. Refusing
/// before anything is applied is the only safe answer; merging would overwrite one side's node
/// with the other's and the document would validate.
#[test]
fn two_branches_of_one_participant_are_refused_before_anything_is_applied() {
    let (ancestor, paragraphs) = seed(1);
    let mut ours = Branched::new(&ancestor, ClientId::new(0));
    ours.edit("Typing", vec![typing(paragraphs[0], 0, "Z")]);
    let commits = ours.commits();

    let error = combine(
        &ancestor,
        Branch {
            client: ClientId::new(0),
            commits: &commits,
        },
        Branch {
            client: ClientId::new(0),
            commits: &commits,
        },
    )
    .expect_err("one participant cannot be both branches");
    assert!(
        matches!(error, CombineError::SameParticipant(client) if client == ClientId::new(0)),
        "{error}"
    );
    assert!(
        error.to_string().contains("one identity space"),
        "the refusal has to say WHY, not just refuse: {error}"
    );
}

/// **A branch recorded with an inverse-less commit combines — on EITHER side — and offline
/// combine is therefore more capable than a live session on this axis.**
///
/// `Coalesce::ContinueKeepingFirstInverse` drops a commit's inverse to keep review typing
/// affordable, and `152` §5.3/§11 record the consequence: such a commit cannot be rolled back,
/// which is why suggesting mode cannot take part in a room. The first draft of `combine`
/// refused one on either branch on exactly that reading, and the mutation discipline is what
/// showed the reading does not transfer: **a combine re-applies both branches from scratch**,
/// and `RevisionLog::apply` computes each inverse as it applies the operation, so every commit
/// in the merged log carries one however its source was recorded. Removing each half of the
/// check produced a correctly merged document — `"abZabcdefgh"` and `"Zababcdefgh"` — so the
/// refusal was turning away a legitimate combine, and a refusal nobody needs is as much a
/// defect as a missing one.
///
/// This guard is therefore a CAPABILITY guard rather than a refusal guard, and it is the one
/// that would catch somebody reinstating the check on the §11 reading without re-deriving it.
#[test]
fn a_branch_whose_commits_kept_no_inverse_still_combines_from_either_side() {
    let (ancestor, paragraphs) = seed(1);

    /// A branch whose second commit deliberately keeps no inverse.
    fn coalesced(ancestor: &Document, client: ClientId, node: NodeId) -> Vec<Commit> {
        let mut branch = Branched::new(ancestor, client);
        branch.edit("Typing", vec![typing(node, 0, "a")]);
        branch.edit_coalescing(
            "Typing",
            vec![typing(node, 1, "b")],
            Coalesce::ContinueKeepingFirstInverse,
        );
        let commits = branch.commits();
        assert!(
            commits.iter().any(|commit| commit.changes().is_none()),
            "the fixture must actually contain an inverse-less commit, or this guard is vacuous"
        );
        commits
    }

    let mut plain = Branched::new(&ancestor, ClientId::new(9));
    plain.edit("Typing", vec![typing(paragraphs[0], 0, "Z")]);
    let plain_commits = plain.commits();

    // On THEIR side: their two keystrokes as written, ours rebased past them.
    let theirs = coalesced(&ancestor, ClientId::new(1), paragraphs[0]);
    let merged = combine(
        &ancestor,
        Branch {
            client: ClientId::new(9),
            commits: &plain_commits,
        },
        Branch {
            client: ClientId::new(1),
            commits: &theirs,
        },
    )
    .expect("an inverse-less commit on their branch combines");
    assert_eq!(plain_text(&merged.document), "abZabcdefgh\n");
    assert!(merged.tombstones.is_empty());

    // On OUR side: the rebase rolls it back, using the inverse the replay re-derived.
    let ours = coalesced(&ancestor, ClientId::new(0), paragraphs[0]);
    let merged = combine(
        &ancestor,
        Branch {
            client: ClientId::new(0),
            commits: &ours,
        },
        Branch {
            client: ClientId::new(9),
            commits: &plain_commits,
        },
    )
    .expect("an inverse-less commit on our branch is replayed with a fresh inverse");
    assert_eq!(plain_text(&merged.document), "Zababcdefgh\n");
    assert_eq!(
        merged.replayed, 2,
        "both of our commits were rolled back and replayed, which is what the re-derived \
         inverses made possible"
    );

    // And the cost, asserted rather than only described: the branch's undo GROUPING is not
    // preserved. Two commits that were one undo step on their own branch are two groups here,
    // because each is replayed as `Coalesce::New`.
    let groups: std::collections::BTreeSet<_> = merged
        .log
        .commits()
        .filter(|commit| commit.label() == "Typing")
        .map(super::super::Commit::group)
        .collect();
    assert_eq!(
        groups.len(),
        2,
        "the undo grouping is re-derived rather than carried, and the module says so"
    );
}

/// **A branch of nothing combines to the other branch, and loses nothing.**
///
/// The degenerate case, which is worth a guard because it is the one a caller hits first: a
/// document somebody opened and did not change. `replayed` is zero here and that is *not* the
/// same as "nothing happened", which is why `Combined` documents the distinction.
#[test]
fn an_empty_branch_contributes_nothing_and_takes_nothing_away() {
    let (ancestor, paragraphs) = seed(2);
    let mut theirs = Branched::new(&ancestor, ClientId::new(1));
    theirs.edit("Typing", vec![typing(paragraphs[1], 0, "ONLY")]);
    let their_commits = theirs.commits();

    let merged = combine(
        &ancestor,
        Branch {
            client: ClientId::new(0),
            commits: &[],
        },
        Branch {
            client: ClientId::new(1),
            commits: &their_commits,
        },
    )
    .expect("one empty branch combines");
    assert_eq!(plain_text(&merged.document), plain_text(&theirs.document));
    assert_eq!(merged.replayed, 0, "there was nothing of ours to replay");
    assert!(merged.tombstones.is_empty());
}

/// **Offline combine agrees with the live two-replica path, character for character.**
///
/// The guard that makes the module's claim — "this is the OT transform already built, applied
/// offline" — checkable rather than asserted. The same two branches are played through
/// `ServerSession` + two `ClientSession`s the way a transport would, and the merged text must be
/// identical. If offline combine ever grows a second rebase of its own, the two answers will
/// part company here before anybody ships it.
#[test]
fn offline_combine_agrees_with_the_live_session_path() {
    use crate::protocol::{
        ClientMessage, Identity, Join, Outcome, PROTOCOL_VERSION, Revision, ServerMessage,
    };
    use crate::session::{ClientSession, ServerSession};
    use casual_doc_edit::access::Capabilities;

    let (ancestor, paragraphs) = seed(2);

    // ---- The offline answer ------------------------------------------------
    let mut ours = Branched::new(&ancestor, ClientId::new(0));
    let mut theirs = Branched::new(&ancestor, ClientId::new(1));
    ours.edit("Typing", vec![typing(paragraphs[0], 2, "OUR")]);
    theirs.edit("Typing", vec![typing(paragraphs[0], 4, "THEIR")]);
    let our_commits = ours.commits();
    let their_commits = theirs.commits();
    let offline = combine(
        &ancestor,
        Branch {
            client: ClientId::new(0),
            commits: &our_commits,
        },
        Branch {
            client: ClientId::new(1),
            commits: &their_commits,
        },
    )
    .expect("the branches combine");

    // ---- The live answer, same two branches, through the relay --------------
    let mut server = ServerSession::default();
    let mut admitted = Vec::new();
    for who in ["ada", "grace"] {
        let (answer, _) = server.join(
            &ClientMessage::Join(Join {
                protocol: PROTOCOL_VERSION,
                identity: Identity::new(who).expect("an identity"),
                grant: None,
                resume: None,
            }),
            Capabilities::owner(),
        );
        match answer {
            ServerMessage::Welcome { client, .. } => admitted.push(client),
            other => panic!("expected a welcome, got {other:?}"),
        }
    }
    assert_eq!(admitted, vec![ClientId::new(0), ClientId::new(1)]);

    // Grace (participant 1) orders her edit first, so the live order matches the offline one:
    // `combine` applies `theirs` as written and rebases `ours` onto it.
    let mut ada_document = ancestor.clone();
    let mut ada_log = RevisionLog::default();
    let mut ada = ClientSession::joined(
        &ada_document,
        &ServerMessage::Welcome {
            protocol: PROTOCOL_VERSION,
            client: ClientId::new(0),
            revision: Revision::new(0),
            capabilities: Capabilities::owner(),
        },
        &mut ada_log,
    )
    .expect("ada joins");
    let mut ada_ids = IdGenerator::new(ada.id_space().get());
    // Ada's own branch, replayed with its recorded mints so the live and offline documents
    // name the same nodes.
    for commit in &our_commits {
        let transaction = Transaction::new(
            commit.id(),
            ada_log.head(),
            commit.label(),
            commit.mints().to_vec(),
            commit.operations().to_vec(),
        );
        ada_log
            .apply(&mut ada_document, transaction)
            .expect("ada's branch applies");
    }

    let mut grace_document = ancestor.clone();
    let mut grace_log = RevisionLog::default();
    let mut grace = ClientSession::joined(
        &grace_document,
        &ServerMessage::Welcome {
            protocol: PROTOCOL_VERSION,
            client: ClientId::new(1),
            revision: Revision::new(0),
            capabilities: Capabilities::owner(),
        },
        &mut grace_log,
    )
    .expect("grace joins");
    for commit in &their_commits {
        let transaction = Transaction::new(
            commit.id(),
            grace_log.head(),
            commit.label(),
            commit.mints().to_vec(),
            commit.operations().to_vec(),
        );
        grace_log
            .apply(&mut grace_document, transaction)
            .expect("grace's branch applies");
    }

    // Grace submits; the relay orders her chunk; Ada receives it and rebases.
    let submission = grace
        .flush(&grace_log)
        .expect("grace has unacknowledged work");
    let revision = match server.commit(&submission) {
        Outcome::Ordered { revision } => revision,
        other => panic!("grace's chunk was not ordered: {other:?}"),
    };
    grace
        .acknowledge(submission.seq, revision, &mut grace_log)
        .expect("the acknowledgement lands");
    let arrival = crate::protocol::Arrival {
        revision,
        client: submission.client,
        operations: submission.operations,
    };
    ada.receive(&arrival, &mut ada_document, &mut ada_ids, &mut ada_log)
        .expect("the arrival merges");

    assert_eq!(
        plain_text(&offline.document),
        plain_text(&ada_document),
        "offline combine and the live rebase driver produced different documents, so one of \
         them is not the transform this module claims to be"
    );
}
