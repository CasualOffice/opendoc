// SPDX-License-Identifier: Apache-2.0

//! The journal's guards.
//!
//! Each one asserts a *guarantee* rather than a mechanism: that an acknowledged chunk survives a
//! restart, that a torn tail is survivable and a torn middle is not, and that recovery verifies
//! the decision rather than trusting the file.

use casual_doc_edit::{Mint, Operation, Pos};
use casual_doc_model::{IdGenerator, NodeId};
use casual_doc_transaction::protocol::CHUNK_BUDGET_BYTES;
use casual_doc_transaction::protocol::{
    Base, ClientId, ClientMessage, Identity, Join, Outcome, PROTOCOL_VERSION, Revision, Seq,
    ServerMessage, Submission,
};
use casual_doc_transaction::session::{DEFAULT_RETAINED_REVISIONS, ServerSession};
use casual_doc_transaction::wire::WireOperation;

use super::{Journal, JournalError};

/// A scratch path unique to this test binary and this name, cleaned up by the caller.
fn scratch(name: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "opendoc-relay-{}-{name}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos())
    ));
    path
}

fn joined(session: &mut ServerSession, who: &str) -> ClientId {
    let answer = session.join(&ClientMessage::Join(Join {
        protocol: PROTOCOL_VERSION,
        identity: Identity::new(who).expect("an identity"),
        resume: None,
    }));
    match answer {
        ServerMessage::Welcome { client, .. } | ServerMessage::Resumed { client, .. } => client,
        other => panic!("expected a welcome, got {other:?}"),
    }
}

fn chunk(client: ClientId, seq: u64, base: Base, text: &str) -> Submission {
    let node = NodeId::new(0x51).expect("id");
    let mint = Mint::reserve(&mut IdGenerator::new(0x60 + seq), 1).expect("a mint");
    Submission {
        client,
        seq: Seq::new(seq),
        base,
        operations: vec![WireOperation::of(
            Operation::InsertText {
                at: Pos::new(node, 0),
                text: text.to_owned(),
            },
            mint,
        )],
    }
}

#[test]
fn a_chunk_the_relay_ordered_survives_a_restart() {
    // The whole point of 6.1, asserted as the guarantee and not as "a file was written": the
    // order a client was acknowledged against is still the order after a restart.
    let path = scratch("restart");
    let mut session = ServerSession::default();
    let client = joined(&mut session, "ada");

    let mut journal = Journal::create(&path, &session).expect("a fresh journal");
    let first = chunk(client, 1, Base::Revision(Revision::new(0)), "a");
    let Outcome::Ordered { revision: at_one } = session.commit(&first) else {
        panic!("the first chunk must order");
    };
    journal.append(&first, at_one).expect("it journals");
    let second = chunk(client, 2, Base::Chained, "b");
    let Outcome::Ordered { revision: at_two } = session.commit(&second) else {
        panic!("the second chunk must order");
    };
    journal.append(&second, at_two).expect("it journals");
    drop(journal);

    let (_, recovered) = Journal::open(&path).expect("it reopens");
    assert_eq!(recovered.replayed, 2, "both chunks must be replayed");
    assert_eq!(
        recovered.discarded_tail_bytes, 0,
        "a clean file has no partial tail"
    );
    assert_eq!(
        recovered
            .session
            .history_since(Revision::new(0))
            .map_or(0, |entries| entries.len()),
        2,
        "the recovered relay must hold the order it imposed, not an empty one"
    );
    // And it must hold it at the SAME revisions, or every client's acknowledgement now names a
    // position that means something else.
    let revisions: Vec<u64> = recovered
        .session
        .history_since(Revision::new(0))
        .expect("the window covers it")
        .map(|entry| entry.revision.get())
        .collect();
    assert_eq!(revisions, vec![at_one.get(), at_two.get()]);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_partial_final_frame_is_discarded_and_counted_rather_than_failing_the_whole_file() {
    // A crash between appends. The condition is CREATED by truncating a real journal mid-frame,
    // not simulated by a flag, because the shape that matters is a frame whose header promises
    // more bytes than the file holds.
    let path = scratch("torn-tail");
    let mut session = ServerSession::default();
    let client = joined(&mut session, "ada");
    let mut journal = Journal::create(&path, &session).expect("a fresh journal");
    let first = chunk(client, 1, Base::Revision(Revision::new(0)), "a");
    let Outcome::Ordered { revision } = session.commit(&first) else {
        panic!("it must order");
    };
    journal.append(&first, revision).expect("it journals");
    drop(journal);

    let whole = std::fs::read(&path).expect("read");
    let torn = &whole[..whole.len() - 7];
    std::fs::write(&path, torn).expect("write");

    let (_, recovered) = Journal::open(&path).expect("a torn tail is survivable");
    assert_eq!(
        recovered.replayed, 0,
        "the torn chunk must not be replayed — it was never fully recorded"
    );
    assert!(
        recovered.discarded_tail_bytes > 0,
        "the discarded tail must be COUNTED: it is the only evidence the last run crashed"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_corrupt_frame_in_the_middle_is_refused_rather_than_skipped() {
    // The other half of the same rule, and the one a careless implementation gets wrong by
    // being forgiving: records after a corrupt one belong to an order this file can no longer
    // describe, so continuing past it would drop acknowledged work silently.
    let path = scratch("torn-middle");
    let mut session = ServerSession::default();
    let client = joined(&mut session, "ada");
    let mut journal = Journal::create(&path, &session).expect("a fresh journal");
    for seq in 1..=2_u64 {
        let base = if seq == 1 {
            Base::Revision(Revision::new(0))
        } else {
            Base::Chained
        };
        let offered = chunk(client, seq, base, "x");
        let Outcome::Ordered { revision } = session.commit(&offered) else {
            panic!("it must order");
        };
        journal.append(&offered, revision).expect("it journals");
    }
    drop(journal);

    let mut whole = std::fs::read(&path).expect("read");
    // Break the magic of a frame that is NOT the last one. The checkpoint's own length tells us
    // where the first appended frame starts.
    let checkpoint_payload =
        u32::from_le_bytes([whole[8], whole[9], whole[10], whole[11]]) as usize;
    let second_frame = 12 + checkpoint_payload;
    whole[second_frame] = b'Z';
    std::fs::write(&path, &whole).expect("write");

    match Journal::open(&path) {
        Err(JournalError::Corrupt { at, .. }) => assert_eq!(
            at as usize, second_frame,
            "the refusal must name where the damage is"
        ),
        other => panic!("a corrupt middle must be refused, got {other:?}"),
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn recovery_checks_the_decision_and_refuses_a_journal_that_disagrees() {
    // The property that makes this recovery verifiable rather than trusting (ADR-051's rule,
    // applied to the relay's own state). The condition is created by rewriting a logged
    // revision to a value the replay cannot reach — which is what a changed ordering rule would
    // look like from the file's side.
    let path = scratch("disagrees");
    let mut session = ServerSession::default();
    let client = joined(&mut session, "ada");
    let mut journal = Journal::create(&path, &session).expect("a fresh journal");
    let offered = chunk(client, 1, Base::Revision(Revision::new(0)), "a");
    let Outcome::Ordered { revision } = session.commit(&offered) else {
        panic!("it must order");
    };
    journal.append(&offered, revision).expect("it journals");
    drop(journal);

    // The logged revision is a JSON number in the appended frame's payload. Rewriting it is the
    // smallest possible disagreement: everything else about the file is intact.
    //
    // Replaced with a value of the SAME decimal width, so the frame's declared length still
    // matches its payload. A longer replacement would truncate the frame and the journal would
    // refuse it as corrupt instead — which is a different guard, and would have made this one
    // pass for the wrong reason. (It did, first time round.)
    let logged = revision.get();
    let forged = if logged % 10 == 9 {
        logged - 1
    } else {
        logged + 1
    };
    let text = String::from_utf8(std::fs::read(&path).expect("read")).expect("the payload is text");
    let doctored = text.replace(
        &format!("\"revision\":{logged}"),
        &format!("\"revision\":{forged}"),
    );
    assert_ne!(
        doctored, text,
        "the revision must actually have been rewritten"
    );
    assert_eq!(
        doctored.len(),
        text.len(),
        "the replacement must be the same width, or this tests truncation instead"
    );
    std::fs::write(&path, doctored.as_bytes()).expect("write");

    match Journal::open(&path) {
        Err(JournalError::DecisionDiffers {
            logged: said,
            replayed,
        }) => {
            assert_eq!(said.get(), forged);
            assert_eq!(replayed, Some(revision));
        }
        other => panic!("a journal whose decision differs must be refused, got {other:?}"),
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn creating_over_an_existing_room_is_refused() {
    // "The host creates the room, not the first client" is only enforceable if creating twice
    // cannot quietly succeed: the second create would destroy the only copy of an order clients
    // were already acknowledged against.
    let path = scratch("exists");
    let session = ServerSession::default();
    Journal::create(&path, &session).expect("the first create");
    match Journal::create(&path, &session) {
        Err(JournalError::Io(error)) => assert_eq!(
            error.kind(),
            std::io::ErrorKind::AlreadyExists,
            "the refusal must say the room is already there"
        ),
        other => panic!("creating over a room must be refused, got {other:?}"),
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn compaction_leaves_one_checkpoint_and_nothing_to_replay() {
    let path = scratch("compact");
    let mut session = ServerSession::default();
    let client = joined(&mut session, "ada");
    let mut journal = Journal::create(&path, &session).expect("a fresh journal");
    let offered = chunk(client, 1, Base::Revision(Revision::new(0)), "a");
    let Outcome::Ordered { revision } = session.commit(&offered) else {
        panic!("it must order");
    };
    journal.append(&offered, revision).expect("it journals");
    let before = std::fs::metadata(&path).expect("stat").len();

    journal.compact(&session).expect("it compacts");
    drop(journal);

    let (_, recovered) = Journal::open(&path).expect("it reopens");
    assert_eq!(
        recovered.replayed, 0,
        "after compaction there is nothing left to replay — that is what compaction IS"
    );
    assert_eq!(
        recovered
            .session
            .history_since(Revision::new(0))
            .map_or(0, |entries| entries.len()),
        1,
        "and the order itself must still be there, inside the checkpoint"
    );
    assert!(
        std::fs::metadata(&path).expect("stat").len() <= before,
        "a compaction that grows the file is not a compaction"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_journal_s_bound_covers_the_largest_checkpoint_the_relay_can_hold() {
    // A guard on the ARITHMETIC, because the bytes cannot be guarded: the condition is a 1.2 GB
    // checkpoint, and a test that allocated one would be a test nobody runs.
    //
    // The defect this exists for was real and was found by reading the two constants against each
    // other rather than by any test. A checkpoint embeds the relay's whole retained history —
    // `DEFAULT_RETAINED_REVISIONS` entries of up to `CHUNK_BUDGET_BYTES` — and the codec's wire
    // bound is 30 MB, so reading the journal with the wire bound meant a busy relay writing a
    // checkpoint it could never read back: `compact` succeeds, the next `open` refuses its own
    // file, and the order is gone.
    let worst_checkpoint = DEFAULT_RETAINED_REVISIONS * CHUNK_BUDGET_BYTES;
    assert!(
        super::MAX_JOURNAL_FRAME_BYTES >= worst_checkpoint,
        "the journal's frame bound ({}) is below the largest checkpoint the relay can hold ({}), \
         so a busy relay would write a file it cannot read back",
        super::MAX_JOURNAL_FRAME_BYTES,
        worst_checkpoint
    );
    // And the wire bound must stay where it is: raising it to cover a checkpoint would let a
    // hostile socket buffer what only a local file is allowed to.
    assert!(
        casual_doc_transaction::codec::MAX_FRAME_BYTES < worst_checkpoint,
        "the wire bound has been raised to cover a checkpoint, which is the wrong fix: a socket \
         is not a file"
    );
}
