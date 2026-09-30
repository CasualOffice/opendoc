// SPDX-License-Identifier: Apache-2.0

//! The protocol, the identity discipline, and the rebase driver, tested through the whole
//! state machine rather than a field at a time.
//!
//! # Why these run several participants and no network
//!
//! The sibling engine's recorded lesson about where its real collaboration bugs were is
//! *"both sides were individually correct and no test put them in a room together"*: a WASM
//! binding that sent a bare submission instead of a tagged message, and integer-keyed maps
//! that were undeliverable — both invisible to every test that *constructed* a message
//! instead of round-tripping one. So the tests here drive two replicas and a relay in one
//! process, which is the only way the interleavings reach the code at all.
//!
//! What they cannot reach yet is the encoding, because there is none: the operation set has
//! no `serde` and doc 152 §9 keeps the codec out of this increment deliberately. The class of
//! defect the sibling found lives exactly there, which is why doc 152 §10 records
//! round-tripping through a real encoded string — with populated payloads — as the codec
//! lane's first obligation rather than an afterthought.

use casual_doc_edit::{Operation, Pos, Range as EditRange};
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, Paragraph, ParagraphProperties, Run,
    RunProperties, Style, StyleId, StyleKind,
};
use casual_doc_model::{IdGenerator, NodeId};

use crate::protocol::{
    Base, ClientId, ClientMessage, Identity, Join, MAX_OUTSTANDING, Outcome, PROTOCOL_VERSION,
    Refusal, Resume, ResumeKey, Revision, Seq, ServerMessage,
};
use crate::wire::{self, Clash, Collision, IdSpace, WireOperation};
use crate::{Coalesce, RevisionLog, Transaction, TransactionId};

use super::{ClientSession, ServerSession, SessionError};

/// A document of three one-run paragraphs, each carrying the same text so an offset tie is
/// hit on purpose rather than by luck.
fn seed() -> (Document, Vec<NodeId>) {
    let mut ids = IdGenerator::new(7);
    let document_id = ids.next_id().expect("id");
    let mut blocks = Vec::new();
    let mut paragraphs = Vec::new();
    for _ in 0..3 {
        let id = ids.next_id().expect("id");
        let run = ids.next_id().expect("id");
        paragraphs.push(id);
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
    let document =
        Document::new(document_id, blocks, Definitions::default()).expect("a valid document");
    (document, paragraphs)
}

/// One participant: its own copy of the document, its own id allocator, its own log.
struct Replica {
    document: Document,
    ids: IdGenerator,
    log: RevisionLog,
    session: ClientSession,
    next_transaction: u128,
}

impl Replica {
    /// Joins `server` from `document`, minting in the space the relay's participant number
    /// implies — which is the whole point of [`IdSpace`] and is why each replica's allocator
    /// is different here.
    fn join(document: &Document, server: &mut ServerSession, key: &str, who: &str) -> Self {
        let message = server.join(&ClientMessage::Join(Join {
            protocol: PROTOCOL_VERSION,
            identity: Identity::new(who).expect("an identity"),
            resume: Some(Resume {
                key: ResumeKey::new(key).expect("a key"),
                revision: Revision::new(0),
            }),
        }));
        let mut log = RevisionLog::default();
        let session =
            ClientSession::joined(document, &message, &mut log).expect("the join is accepted");
        Self {
            document: document.clone(),
            ids: IdGenerator::new(session.id_space().get()),
            log,
            session,
            next_transaction: 0,
        }
    }

    /// Applies a local edit through the one mutation path.
    fn edit(&mut self, label: &'static str, operations: Vec<Operation>) {
        self.edit_coalescing(label, operations, Coalesce::New);
    }

    fn edit_coalescing(
        &mut self,
        label: &'static str,
        operations: Vec<Operation>,
        coalesce: Coalesce,
    ) {
        self.next_transaction += 1;
        let transaction = Transaction::new(
            TransactionId::new(self.next_transaction),
            self.log.head(),
            label,
            operations,
        )
        .coalescing(coalesce);
        self.log
            .apply(&mut self.document, &mut self.ids, transaction)
            .expect("the edit applies");
    }

    /// Sends everything outstanding and folds the relay's answer back in, the way a transport
    /// would. Returns what was fanned out to the other participants.
    fn exchange(&mut self, server: &mut ServerSession) -> Vec<crate::protocol::Arrival> {
        let mut fanned = Vec::new();
        while let Some(submission) = self.session.flush(&self.log) {
            match server.commit(&submission) {
                Outcome::Ordered { revision } => {
                    self.session
                        .acknowledge(submission.seq, revision, &mut self.log)
                        .expect("the acknowledgement lands");
                    fanned.push(crate::protocol::Arrival {
                        revision,
                        client: submission.client,
                        operations: submission.operations,
                    });
                }
                Outcome::Duplicate { revision } => {
                    self.session
                        .acknowledge(submission.seq, revision, &mut self.log)
                        .expect("the acknowledgement lands");
                }
                Outcome::Refused { reason } => {
                    self.session
                        .refused(Some(submission.seq), reason, &self.log)
                        .expect("a refusal of one chunk is not terminal");
                    break;
                }
            }
        }
        fanned
    }

    /// Merges an arrival through the driver.
    fn receive(
        &mut self,
        arrival: &crate::protocol::Arrival,
    ) -> Result<super::Reception, SessionError> {
        self.session
            .receive(arrival, &mut self.document, &mut self.ids, &mut self.log)
    }
}

/// Replaces every run identity with its document-order rank.
///
/// `apply` mints a run id when an edit splits a run, and no operation in the set addresses a
/// run, so two orders name the same logical run differently and no transform can fix it (doc
/// 150 §9.3). Everything else — block ids, text, properties, order — is compared exactly.
/// Paragraph-only, because these fixtures are.
fn canonicalise(document: &mut Document) {
    let mut next = 1_u128;
    for block in document.body_mut() {
        if let BlockNode::Paragraph(paragraph) = block {
            for inline in &mut paragraph.inlines {
                if let InlineNode::Run(run) = inline {
                    run.id = NodeId::new(next).expect("non-zero");
                    next += 1;
                }
            }
        }
    }
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

fn style(name: &str) -> Style {
    Style {
        kind: StyleKind::Paragraph,
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
    }
}

// ---------------------------------------------------------------------------------------
// The version rule
// ---------------------------------------------------------------------------------------

#[test]
fn a_protocol_mismatch_stops_the_session_and_is_never_retried() {
    let (document, _) = seed();
    let mut log = RevisionLog::default();
    let error = ClientSession::joined(
        &document,
        &ServerMessage::Welcome {
            protocol: PROTOCOL_VERSION + 1,
            client: ClientId::new(0),
            revision: Revision::new(0),
        },
        &mut log,
    )
    .expect_err("a version mismatch is not a join");
    assert_eq!(
        error,
        SessionError::ProtocolVersion {
            server: PROTOCOL_VERSION + 1,
            client: PROTOCOL_VERSION,
        }
    );
    // The distinction that matters more than the detection: a client that treats this as
    // retryable loops for ever, so it is terminal and not retryable, and it says so.
    let refusal = error.refusal();
    assert!(
        refusal.is_terminal(),
        "a version mismatch must end the session"
    );
    assert!(
        !refusal.is_retryable(),
        "a version mismatch must not be retried"
    );
    assert_eq!(refusal.code(), "ODC-7002");
}

#[test]
fn a_relay_answers_a_version_mismatch_with_a_stop_and_not_a_refusal() {
    let mut server = ServerSession::default();
    let answer = server.join(&ClientMessage::Join(Join {
        protocol: PROTOCOL_VERSION + 7,
        identity: Identity::new("ada").expect("an identity"),
        resume: None,
    }));
    assert!(
        matches!(
            answer,
            ServerMessage::Stopped {
                reason: Refusal::ProtocolVersion { .. }
            }
        ),
        "got {answer:?}, which a client would retry"
    );
}

#[test]
fn the_three_refusals_that_ask_for_opposite_things_are_three_codes() {
    // The sibling lost a live debugging session to answering an unparseable message with
    // `CannotMerge`, which named the transform — the one part that was working.
    let codes = [
        Refusal::Malformed.code(),
        Refusal::CannotMerge.code(),
        Refusal::NotSaving.code(),
    ];
    assert_eq!(
        codes.len(),
        codes
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        "two refusals that ask the user for different things share a code: {codes:?}"
    );
    // And the actions they imply differ, which is what the codes are for.
    assert!(!Refusal::Malformed.is_retryable());
    assert!(Refusal::CannotMerge.is_retryable());
    assert!(Refusal::NotSaving.is_terminal());
}

#[test]
fn every_refusal_code_has_a_row_in_the_register() {
    // `docs/20`: "Error codes are never recycled. Message wording may improve without a
    // breaking release, but code meaning may not." A code the register does not list is a
    // string a host cannot look up, and an `ODC-7xxx` row no variant carries is a code a
    // host can look up and never receive. Both directions are checked, because drift is
    // silent in both.
    //
    // The pairing is enum <-> register, and it is deliberately NOT a reachability claim:
    // `NotAuthorised` and `ReadOnlyAccess` have rows and variants but no sender in this
    // crate, because no host-signed grant is built here (`152` §9). They are wire surface a
    // host fills in. "This code is documented" and "this code is emitted" are different
    // claims, and this guard makes only the first.
    let register = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("docs")
            .join("20-ERROR-CODE-REGISTRY.md"),
    )
    .expect("the error code register is readable");

    // Every refusal this crate can send, by construction rather than by a list someone
    // maintains: one of each variant, so adding a variant without a code fails to compile and
    // adding one without a register row fails here.
    let every: [Refusal; 9] = [
        Refusal::ProtocolVersion {
            server: 1,
            client: 2,
        },
        Refusal::NotAuthorised,
        Refusal::ReadOnlyAccess,
        Refusal::NotSaving,
        Refusal::TooFarBehind {
            oldest: Revision::new(0),
            current: Revision::new(1),
        },
        Refusal::StaleBase {
            current: Revision::new(1),
        },
        Refusal::CannotMerge,
        Refusal::IdCollision,
        Refusal::Malformed,
    ];
    let mut codes = std::collections::BTreeSet::new();
    for refusal in every {
        let code = refusal.code();
        assert!(
            codes.insert(code),
            "{refusal:?} shares code {code} with another refusal that asks for something else"
        );
        assert!(
            register.contains(&format!("`{code}`")),
            "{refusal:?} sends {code}, which docs/20 does not list"
        );
    }
    // And the other direction: the register lists no collaboration code nothing sends.
    for line in register.lines() {
        if let Some(rest) = line.split_once("| `ODC-7").map(|(_, rest)| rest) {
            let code = format!("ODC-7{}", &rest[..3]);
            assert!(
                codes.contains(code.as_str()),
                "docs/20 lists {code}, which no refusal in this crate sends"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------
// The identity discipline — doc 152 §4
// ---------------------------------------------------------------------------------------

#[test]
fn two_participants_never_share_an_identity_space() {
    let (document, _) = seed();
    let base = wire::document_space(&document);
    let mut seen = std::collections::BTreeSet::new();
    for number in 0..2048_u64 {
        let space = wire::space_of(base, ClientId::new(number)).expect("a space");
        assert!(
            seen.insert(space.get()),
            "participant {number} was given a space another already had"
        );
        assert_ne!(
            space.get(),
            base.get(),
            "participant {number} was given the document's own space, where imported nodes live"
        );
    }
    // The two participant numbers the function refuses, rather than leaving as a remark:
    // one would alias the document's own space, the other the reserved offline space.
    assert!(wire::space_of(base, ClientId::new(u64::MAX - 1)).is_none());
    assert!(wire::space_of(base, ClientId::new(u64::MAX)).is_none());
}

#[test]
fn an_identity_minted_in_the_document_s_own_space_is_refused() {
    // The document's own space is where the IMPORTER mints, identically on every replica, so
    // nobody may introduce into it. Until the identity partition landed this was also the
    // live editor's space — `(document.id() >> 64) ^ 0xED17_ED17_ED17_ED17`, a document-
    // derived constant — which is why a session could refuse every arrival an editor made
    // and collaboration could not run. The editor now mints through `IdSpace`; this guard
    // keeps the importer's space closed.
    let (document, paragraphs) = seed();
    let mut server = ServerSession::default();
    let mut receiver = Replica::join(&document, &mut server, "receiver", "grace");

    let document_space = wire::document_space(&document);
    let mut as_the_editor_does = IdGenerator::new(document_space.get());
    let colliding = as_the_editor_does.next_id().expect("id");
    let operation = Operation::SplitParagraph {
        at: Pos::new(paragraphs[0], 4),
        new_id: colliding,
        properties: None,
    };

    let arrival = crate::protocol::Arrival {
        revision: Revision::new(1),
        client: ClientId::new(0),
        operations: vec![WireOperation::of(operation)],
    };
    let error = receiver
        .receive(&arrival)
        .expect_err("an id from the shared space cannot be accepted");
    match error {
        SessionError::IdCollision(collision) => {
            assert_eq!(collision.id, colliding);
            assert!(matches!(collision.clash, Clash::ForeignSpace { .. }));
        }
        other => panic!("expected an id collision, got {other:?}"),
    }
    assert_eq!(error.refusal().code(), "ODC-7008");
}

#[test]
fn an_arriving_definition_at_an_id_this_replica_already_holds_is_refused() {
    // The sibling's ADR-025 test shape, transposed: **the receiver already holds a different
    // entry at that id**. An id that lines up by accident proves nothing.
    //
    // It matters here more than there, because `SetStyleDefinition`'s `Some(style)` is
    // "insert OR replace" by design, so without this check the arrival silently overwrites
    // the receiver's own style and the loss is invisible to everything.
    let (mut document, _) = seed();
    let base = wire::document_space(&document);
    let sender_space = wire::space_of(base, ClientId::new(0)).expect("a space");
    let contested = StyleId::new(IdGenerator::new(sender_space.get()).next_id().expect("id"));
    document
        .definitions_mut()
        .styles
        .insert(contested, style("the receiver's own"));

    let mut server = ServerSession::default();
    let mut receiver = Replica::join(&document, &mut server, "receiver", "grace");
    let arrival = crate::protocol::Arrival {
        revision: Revision::new(1),
        client: ClientId::new(0),
        operations: vec![WireOperation::of(Operation::SetStyleDefinition {
            id: contested,
            style: Some(Box::new(style("the sender's"))),
        })],
    };

    let error = receiver
        .receive(&arrival)
        .expect_err("this must be refused");
    match error {
        SessionError::IdCollision(collision) => assert_eq!(
            collision.clash,
            Clash::AlreadyHeld {
                table: crate::wire::Table::Styles
            }
        ),
        other => panic!("expected an id collision, got {other:?}"),
    }
    // The point of refusing: the receiver's own definition is still its own.
    assert_eq!(
        receiver
            .document
            .definitions()
            .styles
            .get(&contested)
            .and_then(|held| held.name.clone())
            .as_deref(),
        Some("the receiver's own"),
        "the arrival overwrote a definition this replica minted"
    );
}

#[test]
fn an_operation_that_under_declares_what_it_introduces_is_refused() {
    // A sender that declares nothing would otherwise skip both identity checks for the id it
    // left out — which is the one that matters. So the receiver recomputes rather than trusts.
    let (document, paragraphs) = seed();
    let mut server = ServerSession::default();
    let mut receiver = Replica::join(&document, &mut server, "receiver", "grace");
    let space = wire::space_of(wire::document_space(&document), ClientId::new(0)).expect("a space");
    let minted = IdGenerator::new(space.get()).next_id().expect("id");

    // An operation prepared honestly, then stripped of its declaration.
    let honest = WireOperation::of(Operation::SplitParagraph {
        at: Pos::new(paragraphs[0], 4),
        new_id: minted,
        properties: None,
    });
    assert_eq!(honest.declared(), [minted]);
    let stripped = WireOperation::of(Operation::DeleteText {
        range: EditRange {
            start: Pos::new(paragraphs[0], 0),
            end: Pos::new(paragraphs[0], 1),
        },
    });
    assert!(stripped.declared().is_empty());
    let liar = WireOperation::forged(honest.operation().clone(), Vec::new());

    let arrival = crate::protocol::Arrival {
        revision: Revision::new(1),
        client: ClientId::new(0),
        operations: vec![liar],
    };
    match receiver.receive(&arrival) {
        Err(SessionError::IdCollision(collision)) => {
            assert_eq!(collision.clash, Clash::Undeclared);
            assert_eq!(collision.id, minted);
        }
        other => panic!("expected an undeclared identity, got {other:?}"),
    }
}

#[test]
fn the_space_a_replica_mints_in_offline_is_no_participant_s_space() {
    // The reserved third space. A replica with no session still has to mint — local-first is
    // not negotiable — so it mints in `IdSpace::local`. If a participant could ever be handed
    // that space, a document edited offline and then shared into a room would have its ids
    // re-minted underneath it by whoever drew that number.
    let (document, _) = seed();
    let base = wire::document_space(&document);
    let offline = IdSpace::local(base);
    assert_ne!(offline, base, "the offline space is the importer's space");

    for number in 0..4096_u64 {
        let space = wire::space_of(base, ClientId::new(number)).expect("a space");
        assert_ne!(
            space, offline,
            "participant {number} was handed the space an offline replica mints in"
        );
    }
    // The property, not the sample: `space(c) == local` needs `K * (c + 2) == K * 1`, and `K`
    // odd makes that `c == u64::MAX` — the number the derivation refuses.
    assert!(wire::space_of(base, ClientId::new(u64::MAX)).is_none());
}

#[test]
fn an_identity_minted_in_the_offline_space_is_refused_from_a_session() {
    // The offline space is private to a replica, so nothing may introduce into it over the
    // wire: two participants sending from it would be the very collision this partition
    // exists to make impossible.
    let (document, paragraphs) = seed();
    let mut server = ServerSession::default();
    let mut receiver = Replica::join(&document, &mut server, "receiver", "grace");

    let offline = IdSpace::local(wire::document_space(&document));
    let minted = IdGenerator::new(offline.get()).next_id().expect("id");
    let arrival = crate::protocol::Arrival {
        revision: Revision::new(1),
        client: ClientId::new(0),
        operations: vec![WireOperation::of(Operation::SplitParagraph {
            at: Pos::new(paragraphs[0], 4),
            new_id: minted,
            properties: None,
        })],
    };
    let error = receiver
        .receive(&arrival)
        .expect_err("an id from the offline space cannot be accepted");
    assert!(matches!(
        error,
        SessionError::IdCollision(Collision {
            clash: Clash::ForeignSpace { .. },
            ..
        })
    ));
    assert_eq!(error.refusal().code(), "ODC-7008");
}

#[test]
fn two_replicas_introducing_a_definition_at_once_do_not_contend_for_one_id() {
    // The decisive shape, and the sibling's standing rule for it: **the receiver already
    // holds a different entry at the id its own mint produced.** Both replicas perform the
    // *same* action — define a style — with allocators in the same state, which is exactly
    // the case a document-derived namespace turned into a silent overwrite.
    //
    // What makes this able to fail: the receiver's own style is already in its table under
    // the id ITS allocator minted, and the assertion at the end reads that entry back by
    // name. A partition that did not partition would put the sender's style at that id and
    // `SetStyleDefinition`'s "insert or replace" would swallow it without an error.
    let (document, _) = seed();
    let mut server = ServerSession::default();
    let mut sender = Replica::join(&document, &mut server, "sender", "ada");
    let mut receiver = Replica::join(&document, &mut server, "receiver", "grace");
    assert_ne!(
        sender.session.id_space(),
        receiver.session.id_space(),
        "the relay handed two participants one space"
    );

    let senders_id = StyleId::new(sender.ids.next_id().expect("id"));
    let receivers_id = StyleId::new(receiver.ids.next_id().expect("id"));
    assert_ne!(
        senders_id, receivers_id,
        "two replicas minted one id for two different styles"
    );

    receiver
        .document
        .definitions_mut()
        .styles
        .insert(receivers_id, style("the receiver's own"));

    sender.edit(
        "Define a style",
        vec![Operation::SetStyleDefinition {
            id: senders_id,
            style: Some(Box::new(style("the sender's"))),
        }],
    );
    let fanned = sender.exchange(&mut server);
    assert_eq!(fanned.len(), 1);
    receiver
        .receive(&fanned[0])
        .expect("a concurrently introduced definition arrives");

    let styles = &receiver.document.definitions().styles;
    assert_eq!(
        styles.get(&receivers_id).and_then(|held| held.name.clone()),
        Some("the receiver's own".to_owned()),
        "the arrival overwrote the definition this replica minted"
    );
    assert_eq!(
        styles.get(&senders_id).and_then(|held| held.name.clone()),
        Some("the sender's".to_owned()),
        "the arrival's own definition is not in the receiver's table"
    );
}

// ---------------------------------------------------------------------------------------
// The uncontended path — doc 107 B6
// ---------------------------------------------------------------------------------------

#[test]
fn a_replica_with_nothing_pending_does_not_roll_back() {
    let (document, paragraphs) = seed();
    let mut server = ServerSession::default();
    let mut sender = Replica::join(&document, &mut server, "sender", "ada");
    let mut receiver = Replica::join(&document, &mut server, "receiver", "grace");

    sender.edit(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[0], 0),
            text: "Z".to_owned(),
        }],
    );
    let fanned = sender.exchange(&mut server);
    assert_eq!(fanned.len(), 1);

    let before = receiver.log.head();
    let reception = receiver.receive(&fanned[0]).expect("the arrival merges");
    assert_eq!(
        reception.replayed, 0,
        "a replica with nothing in flight rolled something back"
    );
    assert!(reception.tombstones.is_empty());
    // It went down the same path a keystroke takes: one commit, one revision, and the horizon
    // moved with it because nothing of this replica's own is outstanding.
    assert_eq!(receiver.log.head().get(), before.get() + 1);
    assert_eq!(receiver.log.horizon(), receiver.log.head());
    assert_eq!(receiver.log.unordered_commits(), 0);
    assert!(plain_text(&receiver.document).starts_with("Zabcdefgh"));
}

#[test]
fn a_replica_never_sends_back_an_operation_it_received() {
    // A remote chunk becomes a commit in this replica's own log — that is the whole point of
    // it going down `RevisionLog::apply`, the same path a keystroke takes. So "everything in
    // the log this client has not sent yet" is **not** the right set to flush: it includes
    // somebody else's edit, which would be echoed back under this client's own sequence
    // number and applied twice by everyone.
    //
    // The floor is therefore the horizon as well as the flush mark. Found by reading `flush`
    // rather than by a failing test, which is exactly the kind of bug that ships green.
    let (document, paragraphs) = seed();
    let mut server = ServerSession::default();
    let mut ada = Replica::join(&document, &mut server, "ada", "ada");
    let mut grace = Replica::join(&document, &mut server, "grace", "grace");

    // Grace has nothing of her own in flight: the uncontended path.
    ada.edit(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[0], 0),
            text: "A".to_owned(),
        }],
    );
    let fanned = ada.exchange(&mut server);
    let reception = grace.receive(&fanned[0]).expect("the arrival merges");
    assert_eq!(reception.replayed, 0);

    assert!(
        grace.session.flush(&grace.log).is_none(),
        "this replica offered somebody else's operation as its own work"
    );
    // And an edit of her own afterwards carries only her own operation.
    grace.edit(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[1], 0),
            text: "G".to_owned(),
        }],
    );
    let submission = grace.session.flush(&grace.log).expect("her own work");
    assert_eq!(submission.operations.len(), 1);
    assert_eq!(
        submission.operations[0].operation(),
        &Operation::InsertText {
            at: Pos::new(paragraphs[1], 0),
            text: "G".to_owned(),
        }
    );
}

// ---------------------------------------------------------------------------------------
// The rebase driver
// ---------------------------------------------------------------------------------------

#[test]
fn a_probe_leaves_the_document_exactly_as_it_found_it() {
    // The one invariant the driver's probe rests on, and the one undo already rests on:
    // applying an operation's inverse restores the state it was applied to.
    let (document, paragraphs) = seed();
    let mut ids = IdGenerator::new(4_000);
    let mut subject = document.clone();
    let operations = vec![
        Operation::InsertText {
            at: Pos::new(paragraphs[1], 3),
            text: "QQ".to_owned(),
        },
        Operation::SplitParagraph {
            at: Pos::new(paragraphs[1], 2),
            new_id: ids.next_id().expect("id"),
            properties: None,
        },
    ];
    let changes = super::probe(&mut subject, &mut ids, operations.clone()).expect("the probe runs");
    assert_eq!(changes.len(), 2);
    let mut expected = document.clone();
    let mut actual = subject.clone();
    canonicalise(&mut expected);
    canonicalise(&mut actual);
    assert_eq!(
        actual, expected,
        "the probe left the document changed, so every inverse it produced is suspect"
    );
}

#[test]
fn two_replicas_editing_one_paragraph_converge() {
    let (document, paragraphs) = seed();
    let mut server = ServerSession::default();
    let mut ada = Replica::join(&document, &mut server, "ada", "ada");
    let mut grace = Replica::join(&document, &mut server, "grace", "grace");

    // Both type into the same paragraph, at the same offset, before either has heard from the
    // other. Grace types twice, so the rebase is of a *sequence* and not of one operation —
    // the case a probe exists for.
    ada.edit(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[0], 4),
            text: "A".to_owned(),
        }],
    );
    grace.edit(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[0], 4),
            text: "G".to_owned(),
        }],
    );
    grace.edit_coalescing(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[0], 5),
            text: "g".to_owned(),
        }],
        Coalesce::Continue,
    );
    assert_eq!(grace.log.unordered_commits(), 2);

    // Ada wins the race for the order.
    let from_ada = ada.exchange(&mut server);
    assert_eq!(from_ada.len(), 1);
    let reception = grace.receive(&from_ada[0]).expect("the arrival merges");
    assert_eq!(
        reception.replayed, 2,
        "both of Grace's steps must survive the rebase"
    );

    // Grace had not yet sent anything, and merging the arrival is what told her where the
    // order had reached — so her first chunk names revision 1 and is accepted at once. That
    // is the whole value of `Base::Revision` naming a position the server just handed out:
    // under accept-at-head, a replica that has caught up before it sends pays no round trip.
    let from_grace = grace.exchange(&mut server);
    assert_eq!(
        from_grace.len(),
        1,
        "a chunk based on the position the client was just told must be ordered"
    );
    assert_eq!(from_grace[0].operations.len(), 2, "both steps must be sent");
    for arrival in &from_grace {
        ada.receive(arrival).expect("the arrival merges");
    }

    let mut left = ada.document.clone();
    let mut right = grace.document.clone();
    canonicalise(&mut left);
    canonicalise(&mut right);
    assert_eq!(
        left,
        right,
        "the two replicas disagree:\n{:?}\n{:?}",
        plain_text(&ada.document),
        plain_text(&grace.document)
    );
    // And the settled order is what decided it, not whoever transformed last.
    assert_eq!(
        plain_text(&ada.document).lines().next(),
        Some("abcdAGgefgh")
    );
}

#[test]
fn a_rebase_keeps_the_user_s_own_undo_steps() {
    // A rebased commit is the same user step expressed against a document that has moved. If
    // a rebase renamed the step, Undo would either skip it or merge it with another.
    let (document, paragraphs) = seed();
    let mut server = ServerSession::default();
    let mut ada = Replica::join(&document, &mut server, "ada", "ada");
    let mut grace = Replica::join(&document, &mut server, "grace", "grace");

    grace.edit(
        "Table structure",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[2], 1),
            text: "K".to_owned(),
        }],
    );
    let before: Vec<_> = grace
        .log
        .commits()
        .map(|commit| (commit.id().get(), commit.group().get(), commit.label()))
        .collect();
    let undo_before = grace.log.undo_target();

    ada.edit(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[0], 0),
            text: "A".to_owned(),
        }],
    );
    let fanned = ada.exchange(&mut server);
    grace.receive(&fanned[0]).expect("the arrival merges");

    let after: Vec<_> = grace
        .log
        .commits()
        .filter(|commit| commit.label() != "Remote change")
        .map(|commit| (commit.id().get(), commit.group().get(), commit.label()))
        .collect();
    assert_eq!(
        before, after,
        "the rebase renamed the user's step, so Undo now targets something else"
    );
    assert_eq!(
        grace.log.undo_target(),
        undo_before,
        "Undo targets a different group after a rebase"
    );
}

#[test]
fn a_rebase_re_anchors_the_marks_that_say_what_is_already_in_flight() {
    // A replay renumbers this replica's own revisions, and every mark that names one has to
    // move with them. If the flush mark is left behind, the next flush offers an operation
    // that is **already in flight under a different sequence number** — so the relay's
    // `(client, seq)` dedupe cannot see it, and the edit is applied twice. Silent, and
    // arrived at by arithmetic.
    let (document, paragraphs) = seed();
    let mut server = ServerSession::default();
    let mut ada = Replica::join(&document, &mut server, "ada", "ada");
    let mut grace = Replica::join(&document, &mut server, "grace", "grace");

    for text in ["G", "g"] {
        grace.edit(
            "Typing",
            vec![Operation::InsertText {
                at: Pos::new(paragraphs[1], 0),
                text: text.to_owned(),
            }],
        );
    }
    let in_flight = grace.session.flush(&grace.log).expect("a chunk");
    assert_eq!(in_flight.operations.len(), 2);

    // Ada wins the order while Grace's chunk is still on the wire, so the arrival reaches her
    // before the answer to her own submission does.
    ada.edit(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[0], 0),
            text: "A".to_owned(),
        }],
    );
    let fanned = ada.exchange(&mut server);
    let reception = grace.receive(&fanned[0]).expect("the arrival merges");
    assert_eq!(reception.replayed, 2);

    assert!(
        grace.session.flush(&grace.log).is_none(),
        "the rebase offered an operation that is already in flight, so the relay's \
         (client, seq) dedupe cannot recognise it and the edit would apply twice"
    );

    // And when the refusal for that chunk does arrive, both steps come back — in their new
    // coordinates, because that is what the log now holds.
    let outcome = server.commit(&in_flight);
    let Outcome::Refused { reason } = outcome else {
        panic!("a chunk written against revision 0 must not be ordered: {outcome:?}");
    };
    grace
        .session
        .refused(Some(in_flight.seq), reason, &grace.log)
        .expect("a stale base is not terminal");
    let resubmit = grace
        .session
        .flush(&grace.log)
        .expect("the refused work must be offered again");
    assert_eq!(
        resubmit.operations.len(),
        2,
        "a step was dropped between the rebase and the resubmit"
    );
}

#[test]
fn an_unordered_commit_that_kept_no_inverse_cannot_be_rolled_back() {
    // Doc 150 §10 Q2 recorded this as an open question. It is not one: a
    // `ContinueKeepingFirstInverse` commit deliberately records no inverse, so it can neither
    // serve as a concurrent change nor be rolled back — which makes suggesting mode
    // **unable to take part in a session** until doc 147's envelope retains inverses and
    // drops them at undo-read time instead. Refused with a stable code rather than diverged.
    let (document, paragraphs) = seed();
    let mut server = ServerSession::default();
    let mut ada = Replica::join(&document, &mut server, "ada", "ada");
    let mut grace = Replica::join(&document, &mut server, "grace", "grace");

    grace.edit(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[1], 0),
            text: "x".to_owned(),
        }],
    );
    grace.edit_coalescing(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[1], 1),
            text: "y".to_owned(),
        }],
        Coalesce::ContinueKeepingFirstInverse,
    );

    ada.edit(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[0], 0),
            text: "A".to_owned(),
        }],
    );
    let fanned = ada.exchange(&mut server);
    let before = grace.document.clone();
    let error = grace
        .receive(&fanned[0])
        .expect_err("a commit with no inverse cannot be rolled back");
    assert_eq!(error, SessionError::NotRollbackable);
    assert_eq!(
        grace.document, before,
        "a refused reception left the document changed"
    );
    assert_eq!(grace.log.unordered_commits(), 2, "the log was left short");
}

// ---------------------------------------------------------------------------------------
// Pipelining and acknowledgement
// ---------------------------------------------------------------------------------------

#[test]
fn pipelining_stops_at_the_bound_and_degrades_to_stop_and_wait() {
    let (document, paragraphs) = seed();
    let mut server = ServerSession::default();
    let mut ada = Replica::join(&document, &mut server, "ada", "ada");

    // One chunk per flush while nothing is acknowledged, up to the bound and then nothing.
    for index in 0..MAX_OUTSTANDING + 8 {
        ada.edit(
            "Typing",
            vec![Operation::InsertText {
                at: Pos::new(paragraphs[0], 0),
                text: "z".to_owned(),
            }],
        );
        let submission = ada.session.flush(&ada.log);
        if index < MAX_OUTSTANDING {
            assert!(submission.is_some(), "chunk {index} was not offered");
        } else {
            assert!(
                submission.is_none(),
                "chunk {index} was sent past the outstanding bound"
            );
        }
    }
    assert_eq!(ada.session.outstanding(), MAX_OUTSTANDING);
    assert!(ada.session.has_unacknowledged());
    // Edits keep accumulating rather than being dropped: the operations are still in the log.
    assert!(ada.log.unordered_commits() >= MAX_OUTSTANDING + 8);
}

#[test]
fn a_chunk_always_carries_at_least_one_commit_however_large_it_is() {
    let (document, paragraphs) = seed();
    let mut server = ServerSession::default();
    let mut ada = Replica::join(&document, &mut server, "ada", "ada");

    // One paste bigger than the whole chunk budget. If the budget could refuse it, `flush`
    // would spin on a submission it can never make.
    let huge = "q".repeat(crate::protocol::CHUNK_BUDGET_BYTES + 1024);
    ada.edit(
        "Paste",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[0], 0),
            text: huge,
        }],
    );
    let submission = ada
        .session
        .flush(&ada.log)
        .expect("an over-budget commit must still be offered");
    assert_eq!(submission.operations.len(), 1);
    assert!(
        submission
            .operations
            .iter()
            .map(WireOperation::carried_bytes)
            .sum::<usize>()
            > crate::protocol::CHUNK_BUDGET_BYTES
    );
}

#[test]
fn the_first_chunk_names_a_revision_and_the_rest_are_chained() {
    let (document, paragraphs) = seed();
    let mut server = ServerSession::default();
    let mut ada = Replica::join(&document, &mut server, "ada", "ada");

    ada.edit(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[0], 0),
            text: "1".to_owned(),
        }],
    );
    let first = ada.session.flush(&ada.log).expect("a first chunk");
    assert_eq!(first.base, Base::Revision(Revision::new(0)));
    assert_eq!(first.seq, Seq::new(1));

    ada.edit(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[0], 0),
            text: "2".to_owned(),
        }],
    );
    let second = ada.session.flush(&ada.log).expect("a second chunk");
    assert_eq!(
        second.base,
        Base::Chained,
        "a sender does not need to know the receiver's position"
    );
    assert_eq!(second.seq, Seq::new(2));
}

#[test]
fn a_chained_chunk_from_a_client_with_nothing_accepted_is_refused_not_guessed_at() {
    let mut server = ServerSession::default();
    let outcome = server.commit(&crate::protocol::Submission {
        client: ClientId::new(3),
        seq: Seq::new(1),
        base: Base::Chained,
        operations: vec![WireOperation::of(Operation::SetEvenAndOddHeaders {
            enabled: true,
        })],
    });
    assert_eq!(
        outcome,
        Outcome::Refused {
            reason: Refusal::Malformed
        },
        "inventing a base is how divergence starts"
    );
    assert_eq!(server.head(), Revision::new(0));
}

#[test]
fn a_resend_of_an_ordered_chunk_answers_where_it_landed_the_first_time() {
    let mut server = ServerSession::default();
    let submission = crate::protocol::Submission {
        client: ClientId::new(1),
        seq: Seq::new(1),
        base: Base::Revision(Revision::new(0)),
        operations: vec![WireOperation::of(Operation::SetEvenAndOddHeaders {
            enabled: true,
        })],
    };
    assert_eq!(
        server.commit(&submission),
        Outcome::Ordered {
            revision: Revision::new(1)
        }
    );
    // Somebody else moves the document on, so "where it landed" and "where we are" differ.
    server.commit(&crate::protocol::Submission {
        client: ClientId::new(2),
        seq: Seq::new(1),
        base: Base::Revision(Revision::new(1)),
        operations: vec![WireOperation::of(Operation::SetEvenAndOddHeaders {
            enabled: false,
        })],
    });
    assert_eq!(
        server.commit(&submission),
        Outcome::Duplicate {
            revision: Revision::new(1)
        },
        "a resend must name where it landed originally, not where the document is now"
    );
    assert_eq!(
        server.head(),
        Revision::new(2),
        "a duplicate was re-ordered"
    );
}

#[test]
fn a_stale_base_is_refused_with_where_the_document_is_and_holds_the_client_until_it_catches_up() {
    let (document, paragraphs) = seed();
    let mut server = ServerSession::default();
    let mut ada = Replica::join(&document, &mut server, "ada", "ada");
    let mut grace = Replica::join(&document, &mut server, "grace", "grace");

    ada.edit(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[0], 0),
            text: "A".to_owned(),
        }],
    );
    ada.exchange(&mut server);

    grace.edit(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[1], 0),
            text: "G".to_owned(),
        }],
    );
    let submission = grace.session.flush(&grace.log).expect("a chunk");
    let outcome = server.commit(&submission);
    assert_eq!(
        outcome,
        Outcome::Refused {
            reason: Refusal::StaleBase {
                current: Revision::new(1)
            }
        }
    );
    assert!(Refusal::CannotMerge.is_retryable());
    grace
        .session
        .refused(
            Some(submission.seq),
            Refusal::StaleBase {
                current: Revision::new(1),
            },
            &grace.log,
        )
        .expect("a stale base is not terminal");
    // Nothing was lost — and nothing is sent again until this replica has received what
    // caused the refusal. That is state, not an assumption about message ordering.
    assert_eq!(grace.session.outstanding(), 0);
    assert!(
        grace.session.flush(&grace.log).is_none(),
        "a client resubmitted before rebasing"
    );
}

#[test]
fn an_acknowledgement_is_cumulative_and_settles_exactly_its_own_commits() {
    let (document, paragraphs) = seed();
    let mut server = ServerSession::default();
    let mut ada = Replica::join(&document, &mut server, "ada", "ada");

    for text in ["1", "2", "3"] {
        ada.edit(
            "Typing",
            vec![Operation::InsertText {
                at: Pos::new(paragraphs[0], 0),
                text: text.to_owned(),
            }],
        );
        ada.session.flush(&ada.log).expect("a chunk");
    }
    assert_eq!(ada.session.outstanding(), 3);
    // One acknowledgement naming the middle chunk drops the prefix, which is what makes a
    // lost acknowledgement self-healing.
    ada.session
        .acknowledge(Seq::new(2), Revision::new(2), &mut ada.log)
        .expect("the acknowledgement lands");
    assert_eq!(ada.session.outstanding(), 1);
    assert_eq!(
        ada.log.unordered_commits(),
        1,
        "the horizon settled the wrong number of commits"
    );
    assert!(
        ada.log.horizon() <= ada.log.head(),
        "the horizon passed the head, which makes the log's own commits unreachable"
    );
}

// ---------------------------------------------------------------------------------------
// Resume
// ---------------------------------------------------------------------------------------

#[test]
fn a_resumed_participant_keeps_its_number_and_its_chunk_counter() {
    let (document, paragraphs) = seed();
    let mut server = ServerSession::default();
    let mut ada = Replica::join(&document, &mut server, "ada", "ada");
    let number = ada.session.client();

    ada.edit(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[0], 0),
            text: "A".to_owned(),
        }],
    );
    ada.exchange(&mut server);

    let answer = server.join(&ClientMessage::Join(Join {
        protocol: PROTOCOL_VERSION,
        identity: Identity::new("ada").expect("an identity"),
        resume: Some(Resume {
            key: ResumeKey::new("ada").expect("a key"),
            revision: Revision::new(1),
        }),
    }));
    match &answer {
        ServerMessage::Resumed { client, .. } => assert_eq!(*client, number),
        other => panic!("expected a resume, got {other:?}"),
    }
    ada.session.resumed(&answer).expect("the resume is adopted");
    // Restarting the counter would let a new chunk collide with an old number and be
    // discarded as a duplicate — silently.
    ada.edit(
        "Typing",
        vec![Operation::InsertText {
            at: Pos::new(paragraphs[0], 0),
            text: "B".to_owned(),
        }],
    );
    let next = ada.session.flush(&ada.log).expect("a chunk");
    assert_eq!(next.seq, Seq::new(2));
}

#[test]
fn a_resume_key_presented_by_a_different_identity_is_not_honoured() {
    // Without the scoping, anyone with a valid session could adopt another participant's
    // number and have that participant's submissions suppressed as duplicates.
    let (document, _) = seed();
    let mut server = ServerSession::default();
    let ada = Replica::join(&document, &mut server, "shared-key", "ada");
    let answer = server.join(&ClientMessage::Join(Join {
        protocol: PROTOCOL_VERSION,
        identity: Identity::new("mallory").expect("an identity"),
        resume: Some(Resume {
            key: ResumeKey::new("shared-key").expect("a key"),
            revision: Revision::new(0),
        }),
    }));
    match answer {
        ServerMessage::Welcome { client, .. } => assert_ne!(
            client,
            ada.session.client(),
            "a key presented by somebody else adopted its owner's participant number"
        ),
        other => panic!("expected a fresh welcome, got {other:?}"),
    }
}

#[test]
fn a_participant_too_far_behind_is_told_before_anything_replaces_its_work() {
    // `history_since` is the one function that decides replay versus snapshot, and the order
    // of the two messages is the whole point: the loss is announced *before* the thing that
    // discards it lands.
    let mut server = ServerSession::new(2);
    for seq in 1..=5_u64 {
        let outcome = server.commit(&crate::protocol::Submission {
            client: ClientId::new(1),
            seq: Seq::new(seq),
            base: if seq == 1 {
                Base::Revision(Revision::new(0))
            } else {
                Base::Chained
            },
            operations: vec![WireOperation::of(Operation::SetEvenAndOddHeaders {
                enabled: seq % 2 == 0,
            })],
        });
        assert!(matches!(outcome, Outcome::Ordered { .. }), "{outcome:?}");
    }
    assert!(server.oldest_rebasable() > Revision::new(0));
    assert!(server.history_since(Revision::new(0)).is_none());
    assert!(server.history_since(server.oldest_rebasable()).is_some());

    server.join(&ClientMessage::Join(Join {
        protocol: PROTOCOL_VERSION,
        identity: Identity::new("ada").expect("an identity"),
        resume: Some(Resume {
            key: ResumeKey::new("ada").expect("a key"),
            revision: Revision::new(0),
        }),
    }));
    let answer = server.join(&ClientMessage::Join(Join {
        protocol: PROTOCOL_VERSION,
        identity: Identity::new("ada").expect("an identity"),
        resume: Some(Resume {
            key: ResumeKey::new("ada").expect("a key"),
            revision: Revision::new(0),
        }),
    }));
    assert!(
        matches!(
            answer,
            ServerMessage::Refused {
                seq: None,
                reason: Refusal::TooFarBehind { .. }
            }
        ),
        "got {answer:?}; the loss has to be announced, not implied by a snapshot"
    );
}

// ---------------------------------------------------------------------------------------
// What a dumb relay costs under contention — doc 152 §10 Q3, ADR-047
// ---------------------------------------------------------------------------------------

/// A document of `paragraphs` one-run paragraphs, so every writer in a contention run has a
/// paragraph of its own.
///
/// The contention this measures is the relay's **ordering** rule — a submission whose base
/// is not the current head is refused — and that rule fires whether or not two edits touch
/// the same text. Giving each writer its own paragraph removes semantic conflict from the
/// measurement so what is left is the cost of the ordering discipline alone, which is the
/// thing ADR-047 chose and the thing §10 Q3 asks about.
fn wide_seed(paragraphs: usize) -> (Document, Vec<NodeId>) {
    let mut ids = IdGenerator::new(7);
    let document_id = ids.next_id().expect("id");
    let mut blocks = Vec::new();
    let mut nodes = Vec::new();
    for _ in 0..paragraphs {
        let id = ids.next_id().expect("id");
        let run = ids.next_id().expect("id");
        nodes.push(id);
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
    let document =
        Document::new(document_id, blocks, Definitions::default()).expect("a valid document");
    (document, nodes)
}

/// What one contention run cost.
#[derive(Debug)]
struct Contention {
    /// Chunks the relay ordered.
    ordered: usize,
    /// Chunks the relay refused with `StaleBase`, each of which is one wasted round trip.
    refused: usize,
    /// The most attempts any single writer needed to get one chunk ordered — the starvation
    /// measure, which an average hides.
    worst_attempts: usize,
}

impl Contention {
    /// Wasted round trips per chunk that actually landed.
    fn refusals_per_ordered(&self) -> f64 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "counts here are in the hundreds; the ratio is a report, not a decision"
        )]
        {
            self.refused as f64 / self.ordered as f64
        }
    }
}

/// Drives `writers` replicas that each make `edits` edits, all against one relay, and
/// returns what the ordering discipline cost.
///
/// Deterministic: the scheduler is a fixed round robin, there is no clock and no randomness,
/// so the numbers are reproducible and a regression is a real change rather than noise.
///
/// The loop models a transport honestly in the one way that matters here: a refused writer
/// cannot retry until it has **received** the arrivals that caused the refusal, which is
/// `ClientSession::flush` holding itself closed on `awaiting` rather than a promise about
/// message order.
fn contend(writers: usize, edits: usize) -> Contention {
    const LABEL: &str = "Typing";

    let (document, paragraphs) = wide_seed(writers);
    let mut server = ServerSession::default();
    let mut replicas: Vec<Replica> = (0..writers)
        .map(|w| Replica::join(&document, &mut server, &format!("writer-{w}"), "ada"))
        .collect();
    let mut inboxes: Vec<Vec<crate::protocol::Arrival>> = vec![Vec::new(); writers];

    let mut ordered = 0_usize;
    let mut refused = 0_usize;
    let mut worst_attempts = 0_usize;

    for round in 0..edits {
        // Everybody types before anybody sends, so every writer is contending from the same
        // instant against the same head. Staggering them would measure the scheduler instead
        // of the protocol, and is exactly how a first draft of this harness measured zero.
        for (w, replica) in replicas.iter_mut().enumerate() {
            replica.edit(
                LABEL,
                vec![Operation::InsertText {
                    at: Pos::new(paragraphs[w], 0),
                    text: char::from(b'A' + u8::try_from(round % 26).expect("a letter"))
                        .to_string(),
                }],
            );
        }

        let mut attempts = vec![0_usize; writers];
        // A bound, so a protocol that cannot make progress fails rather than hanging the
        // suite. One tick per writer is the ping-pong's own shape; the assertions say what
        // was actually needed.
        for _ in 0..=(writers * 2 + 2) {
            // 1. Everyone offers what it has, on what it knows NOW. This is the whole point:
            //    the offers are simultaneous, so all but one are written against a head the
            //    relay is about to move past.
            let mut batch = Vec::new();
            for (w, replica) in replicas.iter_mut().enumerate() {
                let Replica { session, log, .. } = replica;
                if let Some(submission) = session.flush(log) {
                    attempts[w] += 1;
                    batch.push((w, submission));
                }
            }
            let quiet = batch.is_empty() && inboxes.iter().all(Vec::is_empty);
            if quiet {
                break;
            }

            // 2. The relay orders them in the sequence they reached it.
            for (w, submission) in batch {
                let replica = &mut replicas[w];
                match server.commit(&submission) {
                    Outcome::Ordered { revision } | Outcome::Duplicate { revision } => {
                        ordered += 1;
                        worst_attempts = worst_attempts.max(attempts[w]);
                        let Replica { session, log, .. } = replica;
                        session
                            .acknowledge(submission.seq, revision, log)
                            .expect("the acknowledgement lands");
                        let arrival = crate::protocol::Arrival {
                            revision,
                            client: submission.client,
                            operations: submission.operations.clone(),
                        };
                        for (other, inbox) in inboxes.iter_mut().enumerate() {
                            if other != w {
                                inbox.push(arrival.clone());
                            }
                        }
                    }
                    Outcome::Refused { reason } => {
                        refused += 1;
                        let Replica { session, log, .. } = replica;
                        session
                            .refused(Some(submission.seq), reason, log)
                            .expect("a stale base is not terminal");
                    }
                }
            }

            // 3. The fan-out lands. A refused writer cannot retry until this has happened —
            //    `flush` holds itself closed on `awaiting` — which is a state guarantee
            //    rather than a promise about message order.
            for w in 0..writers {
                for arrival in std::mem::take(&mut inboxes[w]) {
                    replicas[w].receive(&arrival).expect("the arrival merges");
                }
            }
        }
    }

    // Everything every writer wrote is ordered and acknowledged. This is the assertion that
    // makes the numbers mean something: a protocol that dropped work would show a *lower*
    // refusal rate.
    for (w, replica) in replicas.iter_mut().enumerate() {
        assert!(
            !replica.session.has_unacknowledged(),
            "writer {w} still has unacknowledged work, so the run did not finish"
        );
        let Replica { session, log, .. } = replica;
        assert!(
            session.flush(log).is_none(),
            "writer {w} still has something to send, so the run did not finish"
        );
    }

    Contention {
        ordered,
        refused,
        worst_attempts,
    }
}

#[test]
fn the_dumb_relay_s_refusal_rate_is_the_ping_pong_and_nothing_worse() {
    // **ADR-047's unproven claim, measured.** The relay holds no document and runs no
    // transform, so a submission written against a head the document has moved past is
    // refused and the client rebases locally and resubmits. The ADR says the cost is "one
    // extra round trip" per refusal and that "sustained many-writer contention can starve a
    // slow client", and it says nothing there is proven until the refusal rate is run.
    //
    // This runs it. Every number below is produced by this test; it is not quoted from
    // anywhere. The shape it asserts is the guarantee — progress for everybody, and a cost
    // that grows no faster than the number of writers — rather than a pinned constant, which
    // would redden on any scheduling change that removed nothing.
    let mut report = String::new();
    let mut previous: Option<f64> = None;
    for writers in [1_usize, 2, 4, 8, 16] {
        let run = contend(writers, 4);
        let rate = run.refusals_per_ordered();
        report.push_str(&format!(
            "{writers:>3} writers: {} ordered, {} refused, {rate:.2} refusals/ordered, \
             worst {} attempts for one chunk\n",
            run.ordered, run.refused, run.worst_attempts
        ));

        // 1. Progress. `contend` already asserts every writer finished; this says the relay
        //    ordered something for each of them rather than one writer taking the lot.
        assert!(
            run.ordered >= writers,
            "{writers} writers produced only {} ordered chunks",
            run.ordered
        );

        // 2. The cost is LINEAR in writers, not worse. Each refused writer pays one round
        //    trip per writer that beat it to the head, so the bound is the writer count with
        //    room for the first round; anything above this is not ping-pong, it is a defect.
        #[expect(
            clippy::cast_precision_loss,
            reason = "a small integer bound compared against a ratio"
        )]
        let bound = writers as f64;
        assert!(
            rate <= bound,
            "{writers} writers cost {rate:.2} refusals per ordered chunk, above the linear \
             ping-pong bound of {bound:.2} — the extra cost is not the ordering discipline"
        );

        // 3. It DOES grow. Recorded as an assertion rather than as prose because it is the
        //    finding: a dumb relay makes the client pay for concurrency, and a reader who
        //    only saw the bound above might conclude the cost was flat.
        if let Some(earlier) = previous {
            assert!(
                rate >= earlier,
                "the refusal rate fell from {earlier:.2} to {rate:.2} as writers rose, which \
                 contradicts the ordering rule — check the harness before believing it"
            );
        }
        previous = Some(rate);

        // 4. No writer starves outright: nobody needs more attempts than there are writers
        //    plus the rounds they are competing over.
        assert!(
            run.worst_attempts <= writers + 4,
            "one writer needed {} attempts with {writers} writers — that is starvation, not \
             ping-pong",
            run.worst_attempts
        );
    }
    // Printed so `cargo test -- --nocapture` is the artifact doc 152 §10 Q3's table is read
    // from, rather than a number typed into prose (a published number is generated from a
    // committed artifact, or it is not published). This is the one place in the crate that
    // may write to stdout, and it is a measurement harness rather than library code.
    #[expect(
        clippy::print_stdout,
        reason = "the measurement's artifact; doc 152 §10 Q3 reads its table from this"
    )]
    {
        println!("{report}");
    }
    assert!(
        previous.is_some_and(|rate| rate > 0.0),
        "no contention was measured at all, so nothing here is evidence"
    );
}

#[test]
fn a_single_writer_is_never_refused_by_the_ordering_rule() {
    // The floor the growth above is measured from, and the A1 line: one person editing pays
    // nothing for the machinery. If this ever costs a round trip, the relay has started
    // charging single-user editing for collaboration.
    let run = contend(1, 8);
    assert_eq!(
        run.refused, 0,
        "a lone writer was refused {} times by a rule about concurrent writers",
        run.refused
    );
    assert_eq!(run.worst_attempts, 1, "a lone writer had to retry");
}
