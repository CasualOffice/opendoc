// SPDX-License-Identifier: Apache-2.0

//! The relay's decisions, every one of them read back out of what each participant received.
//!
//! These were unreachable while this logic lived in `main.rs` behind a `TcpStream`, which is why
//! it does not live there any more.

use casual_doc_edit::access::Capabilities;
use casual_doc_edit::{Mint, Operation, Pos};

use crate::Access;
use casual_doc_model::{IdGenerator, NodeId};
use casual_doc_transaction::codec::decode_frame;
use casual_doc_transaction::presence::{PresenceClock, PresenceUpdate};
use casual_doc_transaction::protocol::{
    Base, ClientId, ClientMessage, GrantToken, Identity, Join, PROTOCOL_VERSION, Refusal, Revision,
    Seq, ServerMessage, Submission,
};
use casual_doc_transaction::wire::WireOperation;

use super::Relay;
use crate::Room;

/// A writer that keeps every byte, so a test can read what a participant received.
#[derive(Debug, Default)]
struct Sink {
    written: Vec<u8>,
}

impl std::io::Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.written.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "opendoc-relay-handle-{}-{name}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos())
    ));
    path
}

/// A relay with `count` participants connected, and their ids in join order.
///
/// Opened at [`Capabilities::owner`] because these guards are about fan-out, ordering, presence
/// and disconnection — never about access — so full authority is what preserves what each of them
/// was written to measure. The access guards name their own narrower policy.
fn with_participants(name: &str, count: u64) -> (std::path::PathBuf, Relay<Sink>, Vec<ClientId>) {
    with_access(name, count, Access::Open(Capabilities::owner()))
}

/// As [`with_participants`], under an explicit access policy.
fn with_access(
    name: &str,
    count: u64,
    access: Access,
) -> (std::path::PathBuf, Relay<Sink>, Vec<ClientId>) {
    let path = scratch(name);
    let mut relay = Relay::new(Room::create(&path).expect("a new room"), access);
    let mut clients = Vec::new();
    for which in 0..count {
        let mut writer = Some(Sink::default());
        let handled = relay
            .handle(
                None,
                &mut writer,
                &ClientMessage::Join(Join {
                    protocol: PROTOCOL_VERSION,
                    identity: Identity::new(format!("who-{which}")).expect("an identity"),
                    grant: None,
                    resume: None,
                }),
            )
            .expect("the join is handled");
        assert!(
            writer.is_none(),
            "the join must take the writer, or the participant is not in the set"
        );
        clients.push(handled.joined_as.expect("the join assigned an identity"));
    }
    (path, relay, clients)
}

fn chunk(client: ClientId, seq: u64, base: Base) -> Submission {
    let node = NodeId::new(0x91).expect("id");
    let mint = Mint::reserve(&mut IdGenerator::new(0xA0 + seq), 1).expect("a mint");
    Submission {
        client,
        seq: Seq::new(seq),
        base,
        operations: vec![WireOperation::of(
            Operation::InsertText {
                at: Pos::new(node, 0),
                text: "q".to_owned(),
            },
            mint,
        )],
    }
}

/// Every `ServerMessage` a participant received, decoded.
fn received(relay: &mut Relay<Sink>, client: ClientId) -> Vec<ServerMessage> {
    let sink = relay
        .participants_mut()
        .take(client)
        .expect("the participant is connected");
    let mut out = Vec::new();
    let mut at = 0_usize;
    while at < sink.written.len() {
        let length = casual_doc_transaction::codec::frame_len(&sink.written[at..])
            .expect("a complete frame");
        out.push(decode_frame(&sink.written[at..at + length]).expect("it decodes"));
        at += length;
    }
    // Put it back, so a test can keep going.
    relay.participants_mut().joined(client, sink);
    out
}

#[test]
fn an_ordered_chunk_is_applied_by_everyone_except_its_author() {
    // Three participants, so "everyone but the author" cannot be satisfied by a fan-out that
    // writes to nobody or to exactly one.
    let (path, mut relay, clients) = with_participants("ordered", 3);
    let author = clients[1];

    let handled = relay
        .handle(
            Some(author),
            &mut None,
            &ClientMessage::Submit(chunk(author, 1, Base::Revision(Revision::new(0)))),
        )
        .expect("the chunk is journalled and ordered");

    assert!(
        matches!(handled.answer, Some(ServerMessage::Ack { .. })),
        "the author gets an acknowledgement, which is what it is waiting for: {:?}",
        handled.answer
    );
    assert!(handled.behind.is_empty());

    for client in &clients {
        let messages = received(&mut relay, *client);
        if *client == author {
            assert!(
                messages.is_empty(),
                "the author must receive NOTHING on the fan-out path — it already has its own \
                 edit, and applying it twice is what this prevents"
            );
        } else {
            assert_eq!(messages.len(), 1, "exactly one message for {client:?}");
            assert!(
                matches!(messages[0], ServerMessage::Apply(_)),
                "the others must be told to apply it: {:?}",
                messages[0]
            );
        }
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_duplicate_chunk_is_acknowledged_and_not_fanned_out_a_second_time() {
    // The case a careless relay gets wrong by treating "acknowledge" and "fan out" as one step.
    // Everybody already has it; a second copy would be applied twice.
    let (path, mut relay, clients) = with_participants("duplicate", 2);
    let author = clients[0];
    let offered = chunk(author, 1, Base::Revision(Revision::new(0)));

    relay
        .handle(
            Some(author),
            &mut None,
            &ClientMessage::Submit(offered.clone()),
        )
        .expect("the first lands");
    let first = received(&mut relay, clients[1]).len();
    assert_eq!(first, 1, "the precondition: it was fanned out once");

    let handled = relay
        .handle(Some(author), &mut None, &ClientMessage::Submit(offered))
        .expect("the resend is handled");
    assert!(
        matches!(handled.answer, Some(ServerMessage::Ack { .. })),
        "a resend is still acknowledged, so a resumed client's retry is idempotent"
    );
    assert_eq!(
        received(&mut relay, clients[1]).len(),
        first,
        "and it must NOT be fanned out again"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn presence_is_fanned_with_the_identity_the_relay_attaches_and_never_answered() {
    let (path, mut relay, clients) = with_participants("presence", 2);
    let who = clients[0];
    let update = PresenceUpdate {
        clock: PresenceClock::new(5),
        payload: b"somewhere".to_vec(),
    };

    let handled = relay
        .handle(
            Some(who),
            &mut None,
            &ClientMessage::Presence(update.clone()),
        )
        .expect("presence is handled");
    assert!(
        handled.answer.is_none(),
        "presence is not ordered and never acknowledged, so there is nothing truthful to answer"
    );
    assert!(
        handled.behind.is_empty(),
        "a participant that missed a presence update is not behind anything — the next one \
         corrects it — so failures are deliberately not reported here"
    );

    let messages = received(&mut relay, clients[1]);
    assert_eq!(messages.len(), 1);
    match &messages[0] {
        ServerMessage::Awareness {
            client,
            update: sent,
        } => {
            assert_eq!(
                *client, who,
                "the identity on the wire is the one the RELAY attached, which is what makes a \
                 forged identity unexpressible rather than merely rejected"
            );
            assert_eq!(sent.payload, update.payload);
        }
        other => panic!("expected awareness, got {other:?}"),
    }
    assert!(
        received(&mut relay, who).is_empty(),
        "and the sender is not sent its own caret back"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_presence_update_that_says_nothing_new_is_not_fanned_out() {
    // A replayed or out-of-order update. Fanning it would move a caret BACKWARDS on every other
    // screen, which is worse than dropping it — and `Roster::accept` already decides it, so this
    // asserts the relay honours the decision rather than re-deciding it.
    let (path, mut relay, clients) = with_participants("stale", 2);
    let who = clients[0];
    let at = |clock: u64| PresenceUpdate {
        clock: PresenceClock::new(clock),
        payload: b"x".to_vec(),
    };

    relay
        .handle(Some(who), &mut None, &ClientMessage::Presence(at(9)))
        .expect("the first lands");
    assert_eq!(received(&mut relay, clients[1]).len(), 1);

    relay
        .handle(Some(who), &mut None, &ClientMessage::Presence(at(4)))
        .expect("the stale one is handled");
    assert_eq!(
        received(&mut relay, clients[1]).len(),
        1,
        "an older clock must not reach anybody"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn presence_before_a_join_is_dropped_because_there_is_nobody_to_attribute_it_to() {
    // The shape that would otherwise need a guess. A connection that has not joined has no
    // participant number, and inventing one is exactly what the no-identity-field design exists
    // to prevent.
    let (path, mut relay, clients) = with_participants("unjoined", 1);
    let handled = relay
        .handle(
            None,
            &mut None,
            &ClientMessage::Presence(PresenceUpdate {
                clock: PresenceClock::new(1),
                payload: b"nowhere".to_vec(),
            }),
        )
        .expect("it is handled");
    assert!(handled.answer.is_none());
    assert!(
        relay.roster().is_empty(),
        "nothing may enter the roster without an identity the relay itself assigned"
    );
    assert!(received(&mut relay, clients[0]).is_empty());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_departure_is_announced_so_no_caret_outlives_its_owner() {
    let (path, mut relay, clients) = with_participants("departure", 2);
    let leaving = clients[0];
    relay
        .handle(
            Some(leaving),
            &mut None,
            &ClientMessage::Presence(PresenceUpdate {
                clock: PresenceClock::new(1),
                payload: b"here".to_vec(),
            }),
        )
        .expect("presence lands");
    let before = received(&mut relay, clients[1]).len();

    let behind = relay.disconnected(leaving);
    assert!(behind.is_empty());

    let messages = received(&mut relay, clients[1]);
    assert_eq!(messages.len(), before + 1);
    assert!(
        matches!(messages[before], ServerMessage::Departed { client } if client == leaving),
        "the others must be TOLD: a caret that outlives its owner has the reader believing \
         somebody is there. Got {:?}",
        messages[before]
    );
    assert!(
        relay.roster().get(leaving).is_none(),
        "and the presence itself must be gone"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_connection_that_never_joined_can_disconnect_without_announcing_anything() {
    // The other half of the same rule: `disconnected` is called on EVERY exit path, including one
    // that failed before its `Join`, so it must be a no-op there rather than announcing the
    // departure of somebody who never arrived.
    let (path, mut relay, clients) = with_participants("never-joined", 1);
    let ghost = ClientId::new(999);
    assert!(relay.disconnected(ghost).is_empty());
    assert!(
        received(&mut relay, clients[0]).is_empty(),
        "nobody may be told that a participant who never arrived has left"
    );
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------------------
// Access — ADR-060. Each of these CREATES the condition: a guard that passed because the
// fixture happened to be read-only would tell us nothing.
// ---------------------------------------------------------------------------------------

/// A verifier that honours exactly one token and refuses everything else.
///
/// Deliberately not a "parse a JWT" stand-in. What these guards need to establish is that the
/// relay consults the seam and acts on both answers; a real signature profile is a deployment's
/// choice (`143` §16 Q5) and faking one here would test the fake.
#[derive(Debug)]
struct OneToken {
    expected: Vec<u8>,
    grants: Capabilities,
}

impl crate::GrantVerifier for OneToken {
    fn verify(&self, token: &GrantToken) -> Result<Capabilities, crate::GrantRefusal> {
        if token.as_bytes() == self.expected.as_slice() {
            Ok(self.grants)
        } else {
            Err(crate::GrantRefusal::Invalid)
        }
    }
}

/// Joins `relay` presenting `grant`, returning the answer and the assigned id.
fn join_with(
    relay: &mut Relay<Sink>,
    who: &str,
    grant: Option<GrantToken>,
) -> (ServerMessage, Option<ClientId>) {
    let mut writer = Some(Sink::default());
    let handled = relay
        .handle(
            None,
            &mut writer,
            &ClientMessage::Join(Join {
                protocol: PROTOCOL_VERSION,
                identity: Identity::new(who).expect("an identity"),
                grant,
                resume: None,
            }),
        )
        .expect("the join is handled");
    (
        handled.answer.expect("a join is always answered"),
        handled.joined_as,
    )
}

#[test]
fn a_participant_may_not_submit_as_somebody_else() {
    // The forgeable-identity hole in the EDIT path. `152` §2b closed it for presence by giving
    // the message no field to put an identity in; `Submission` has one, because the dedupe table
    // is keyed on it, so the claim has to be checked instead.
    //
    // The harm is not only misattribution: writing somebody else's `(client, seq)` entry makes
    // THEIR next chunk at that seq come back `Duplicate` and vanish. So this asserts both halves.
    let (path, mut relay, clients) = with_participants("forged-submitter", 2);
    let (ada, grace) = (clients[0], clients[1]);

    // Ada's own chunk is ordered, which establishes that the relay is working at all.
    let handled = relay
        .handle(
            Some(ada),
            &mut None,
            &ClientMessage::Submit(chunk(ada, 1, Base::Revision(Revision::new(0)))),
        )
        .expect("handled");
    assert!(
        matches!(handled.answer, Some(ServerMessage::Ack { .. })),
        "ada's own chunk must be ordered, or this guard is measuring a broken relay: {:?}",
        handled.answer
    );

    // Now ada claims to be grace, on ada's connection.
    //
    // `Base::Revision(head)` and NOT `Base::Chained`, deliberately. A chained chunk from a client
    // with nothing accepted is refused as `Malformed` for an unrelated reason, so writing the
    // forgery that way makes this guard pass on a relay with no identity check at all — measured:
    // it does, and the refusal comes back `Malformed` instead of `NotAuthorised`. An absolute
    // base against the current head is a chunk the relay would otherwise ORDER.
    let handled = relay
        .handle(
            Some(ada),
            &mut None,
            &ClientMessage::Submit(chunk(grace, 1, Base::Revision(Revision::new(1)))),
        )
        .expect("handled");
    assert_eq!(
        handled.answer,
        Some(ServerMessage::Refused {
            seq: Some(Seq::new(1)),
            reason: Refusal::NotAuthorised,
        }),
        "a participant wrote as somebody else"
    );

    // And grace's own first chunk still lands at seq 1 — which is the half that proves the
    // forgery would have DESTROYED work rather than merely mislabelled it.
    let handled = relay
        .handle(
            Some(grace),
            &mut None,
            &ClientMessage::Submit(chunk(grace, 1, Base::Revision(Revision::new(1)))),
        )
        .expect("handled");
    assert!(
        matches!(
            handled.answer,
            Some(ServerMessage::Ack {
                through,
                revision
            }) if through == Seq::new(1) && revision == Revision::new(2)
        ),
        "grace's own seq 1 must still be orderable; if ada's forgery had taken it, this comes \
         back as a duplicate and grace's work is silently gone. Got {:?}",
        handled.answer
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_submission_before_a_join_is_refused() {
    // The same rule with `me == None`: nothing has been admitted on this connection, so there is
    // nobody for the claim to belong to.
    let (path, mut relay, _) = with_participants("submit-before-join", 1);
    let handled = relay
        .handle(
            None,
            &mut None,
            &ClientMessage::Submit(chunk(ClientId::new(0), 1, Base::Revision(Revision::new(0)))),
        )
        .expect("handled");
    assert!(
        matches!(
            handled.answer,
            Some(ServerMessage::Refused {
                reason: Refusal::NotAuthorised,
                ..
            })
        ),
        "got {:?}",
        handled.answer
    );
    assert_eq!(
        relay.room().session().head(),
        Revision::new(0),
        "and nothing may have been ordered"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_read_only_room_refuses_every_edit_at_the_relay() {
    // `Access::Open(viewer())` is a real, reachable capability and not a shape: the relay refuses
    // the operation, with `ODC-7004`, whatever the client's chrome offered. The ordering state is
    // asserted too, because an answer of "refused" over a document that moved anyway would be
    // worse than no check.
    let (path, mut relay, clients) =
        with_access("read-only-room", 1, Access::Open(Capabilities::viewer()));
    let ada = clients[0];
    let handled = relay
        .handle(
            Some(ada),
            &mut None,
            &ClientMessage::Submit(chunk(ada, 1, Base::Revision(Revision::new(0)))),
        )
        .expect("handled");
    assert_eq!(
        handled.answer,
        Some(ServerMessage::Refused {
            seq: Some(Seq::new(1)),
            reason: Refusal::ReadOnlyAccess,
        }),
        "a read-only room ordered an edit"
    );
    assert_eq!(
        relay.room().session().head(),
        Revision::new(0),
        "and the order must not have moved"
    );
    assert_eq!(Refusal::ReadOnlyAccess.code(), "ODC-7004");

    // The same relay, the same participant, an editor room: the SAME chunk is ordered. Without
    // this the guard could pass on a relay that refuses everything.
    let (other, mut editable, clients) =
        with_access("editable-room", 1, Access::Open(Capabilities::editor()));
    let ada = clients[0];
    let handled = editable
        .handle(
            Some(ada),
            &mut None,
            &ClientMessage::Submit(chunk(ada, 1, Base::Revision(Revision::new(0)))),
        )
        .expect("handled");
    assert!(
        matches!(handled.answer, Some(ServerMessage::Ack { .. })),
        "the identical chunk must be ordered in an editable room, or the refusal above is not \
         about access: {:?}",
        handled.answer
    );
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&other);
}

#[test]
fn a_room_that_requires_a_grant_admits_nobody_without_one() {
    // `Access::Granted`: a join with no grant, or with one that does not verify, is not admitted
    // at all — and the refusal is `Stopped`, because retrying an unauthorised join loops for ever
    // (`Refusal::is_terminal` already says so about this code).
    let path = scratch("granted-room");
    let token = GrantToken::new(b"the-one-true-token".to_vec()).expect("a token");
    let mut relay = Relay::new(
        Room::create(&path).expect("a new room"),
        Access::Granted(Box::new(OneToken {
            expected: token.as_bytes().to_vec(),
            grants: Capabilities::commenter(),
        })),
    );

    for (what, offered) in [
        ("no grant at all", None),
        (
            "a grant that does not verify",
            Some(GrantToken::new(b"forged".to_vec()).expect("a token")),
        ),
    ] {
        let (answer, joined) = join_with(&mut relay, "ada", offered);
        assert_eq!(
            answer,
            ServerMessage::Stopped {
                reason: Refusal::NotAuthorised,
            },
            "{what} was admitted"
        );
        assert!(joined.is_none(), "{what} joined the participant set");
        assert!(
            Refusal::NotAuthorised.is_terminal(),
            "an unauthorised join must be terminal, or a client retries it for ever"
        );
    }

    // The real token is admitted, with exactly the capabilities the verifier returned — and NOT
    // more. A guard that only checked the refusals would pass on a relay that refuses everyone.
    let (answer, joined) = join_with(&mut relay, "ada", Some(token));
    let ServerMessage::Welcome { capabilities, .. } = answer else {
        panic!("the real token must be admitted, got {answer:?}");
    };
    assert_eq!(capabilities, Capabilities::commenter());
    assert!(!capabilities.may_edit(), "a commenter must not hold `edit`");
    let ada = joined.expect("the join assigned an identity");

    // And the capabilities are enforced, not merely announced.
    let handled = relay
        .handle(
            Some(ada),
            &mut None,
            &ClientMessage::Submit(chunk(ada, 1, Base::Revision(Revision::new(0)))),
        )
        .expect("handled");
    assert_eq!(
        handled.answer,
        Some(ServerMessage::Refused {
            seq: Some(Seq::new(1)),
            reason: Refusal::ReadOnlyAccess,
        }),
        "a commenter's keystroke was ordered"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_grant_dies_with_the_connection() {
    // A grant left behind is a capability nobody is holding, and the next participant handed that
    // number would inherit it before its own join had been verified. Presence already dies with
    // the connection (`152` §2b); so does this.
    let (path, mut relay, clients) =
        with_access("grant-lifetime", 1, Access::Open(Capabilities::editor()));
    let ada = clients[0];
    let handled = relay
        .handle(
            Some(ada),
            &mut None,
            &ClientMessage::Submit(chunk(ada, 1, Base::Revision(Revision::new(0)))),
        )
        .expect("handled");
    assert!(
        matches!(handled.answer, Some(ServerMessage::Ack { .. })),
        "ada must be able to edit while connected: {:?}",
        handled.answer
    );

    relay.disconnected(ada);

    // The same id, submitting again after the disconnect. The connection check would catch a
    // reused `me`, so this passes `Some(ada)` deliberately: the question is whether the GRANT is
    // still there, and the answer must be no.
    let handled = relay
        .handle(
            Some(ada),
            &mut None,
            &ClientMessage::Submit(chunk(ada, 2, Base::Chained)),
        )
        .expect("handled");
    assert_eq!(
        handled.answer,
        Some(ServerMessage::Refused {
            seq: Some(Seq::new(2)),
            reason: Refusal::ReadOnlyAccess,
        }),
        "a disconnected participant's grant outlived its connection"
    );
    let _ = std::fs::remove_file(&path);
}
