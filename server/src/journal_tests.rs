// SPDX-License-Identifier: Apache-2.0

//! The journal's guards.
//!
//! Each one asserts a *guarantee* rather than a mechanism: that an acknowledged chunk survives a
//! restart, that a torn tail is survivable and a torn middle is not, and that recovery verifies
//! the decision rather than trusting the file.

use casual_doc_edit::access::Capabilities;
use casual_doc_edit::{Mint, Operation, Pos};
use casual_doc_model::{IdGenerator, NodeId};
use casual_doc_transaction::protocol::CHUNK_BUDGET_BYTES;
use casual_doc_transaction::protocol::{
    Base, ClientId, ClientMessage, Identity, Join, Outcome, PROTOCOL_VERSION, Refusal, Resume,
    ResumeKey, Revision, Seq, ServerMessage, Submission,
};
use casual_doc_transaction::session::{DEFAULT_RETAINED_REVISIONS, ServerSession};
use casual_doc_transaction::wire::WireOperation;

use casual_doc_transaction::codec::encode_frame;
use std::fs::OpenOptions;
use std::io::Write as _;

use super::{Journal, JournalError};
use crate::Room;

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
    let (answer, _) = session.join(
        &ClientMessage::Join(Join {
            protocol: PROTOCOL_VERSION,
            identity: Identity::new(who).expect("an identity"),
            grant: None,
            resume: None,
        }),
        Capabilities::owner(),
    );
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
    // Edited as BYTES, not as a `String`. A journal frame is a binary header followed by a JSON
    // payload, so its length field can hold any byte at all — and this guard used to decode the
    // whole file as UTF-8 and happened to pass only because every checkpoint it had ever seen was
    // short enough for the length byte to be ASCII. Adding one field to `ServerSession` pushed the
    // checkpoint to 194 bytes, the length byte became `0xC2`, and the `expect` blew up on a file
    // that was perfectly intact. Searching the bytes has no such coupling.
    let bytes = std::fs::read(&path).expect("read");
    let needle = format!("\"revision\":{logged}").into_bytes();
    let replacement = format!("\"revision\":{forged}").into_bytes();
    assert_eq!(
        needle.len(),
        replacement.len(),
        "the replacement must be the same width, or this tests truncation instead"
    );
    let at = bytes
        .windows(needle.len())
        .rposition(|window| window == needle.as_slice())
        .expect("the appended record names the revision it was ordered at");
    let mut doctored = bytes.clone();
    doctored[at..at + replacement.len()].copy_from_slice(&replacement);
    assert_ne!(
        doctored, bytes,
        "the revision must actually have been rewritten"
    );
    assert_eq!(
        doctored.len(),
        bytes.len(),
        "the file must be the same length, or this tests truncation instead"
    );
    std::fs::write(&path, &doctored).expect("write");

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
fn every_journal_record_fits_the_codec_s_own_frame_bound() {
    // The guard that REPLACES
    // `the_journal_s_bound_covers_the_largest_checkpoint_the_relay_can_hold`, and it asserts the
    // opposite thing on purpose.
    //
    // That guard pinned a derived constant: `MAX_JOURNAL_FRAME_BYTES` had to be at least
    // `DEFAULT_RETAINED_REVISIONS * CHUNK_BUDGET_BYTES` — about 1.2 GB — because a checkpoint
    // embedded the relay's whole retained history in ONE frame. It was correct arithmetic about
    // the wrong shape, and its own doc comment said so. Writing the history as one frame per
    // entry removes the need for a journal-specific bound at all, so the guarantee to hold now
    // is "every record this journal writes fits the bound a socket already enforces".
    //
    // Measured on bytes rather than on arithmetic, which the old shape could not be: a room with
    // a small `retain` is driven past it, so the checkpoint really does carry a full retained
    // window, and every frame in the file is read back and sized.
    let path = scratch("frame-bound");
    let mut session = ServerSession::new(4);
    let client = joined(&mut session, "ada");
    let mut journal = Journal::create(&path, &session).expect("a fresh journal");
    let mut base = Revision::new(0);
    for seq in 1..=8 {
        let offered = chunk(client, seq, Base::Revision(base), "text");
        let Outcome::Ordered { revision } = session.commit(&offered) else {
            panic!("chunk {seq} must order");
        };
        journal.append(&offered, revision).expect("append");
        base = revision;
    }
    journal.compact(&session).expect("a checkpoint");
    drop(journal);

    let bytes = std::fs::read(&path).expect("the journal");
    let mut at = 0_usize;
    let mut frames = 0_usize;
    let mut widest = 0_usize;
    while at < bytes.len() {
        let length = casual_doc_transaction::codec::frame_len(&bytes[at..]).expect("a frame");
        widest = widest.max(length);
        frames += 1;
        at += length;
    }
    assert!(
        frames >= 2,
        "only {frames} frame(s) in the file, so a full retained window was not written as \
         separate frames and this guard is measuring nothing"
    );
    assert!(
        widest <= casual_doc_transaction::codec::MAX_FRAME_BYTES,
        "the widest journal frame is {widest} bytes, above the codec's own MAX_FRAME_BYTES \
         ({}) — so this journal needs a special bound again",
        casual_doc_transaction::codec::MAX_FRAME_BYTES
    );
    let _ = std::fs::remove_file(&path);
}

/// The record of why the deleted constant is not coming back: the wire bound must stay **below** a
/// full retained history, or "a socket is not a file" has quietly stopped being true and raising
/// the codec's own bound would look like a fix.
///
/// A `const` item rather than an `assert!` in a test body, because both sides are constants —
/// clippy's `assertions_on_constants` is right that a runtime assertion over two `const`s is the
/// wrong instrument, and a compile-time one fails the *build* rather than one test.
const _: () = assert!(
    casual_doc_transaction::codec::MAX_FRAME_BYTES
        < DEFAULT_RETAINED_REVISIONS * CHUNK_BUDGET_BYTES,
    "the wire bound now covers a whole retained history, which is the wrong fix: a socket is not \
     a file"
);

#[test]
fn the_steady_state_write_is_one_frame_per_chunk() {
    // `107` §4 B5: snapshots are periodic, never per-operation, and the steady-state write is
    // one appended frame. Asserted as GROWTH rather than as a file size, because the guarantee
    // is that appending is O(the chunk) and not O(the history) — a journal that rewrote its
    // state on every append would satisfy any absolute size check on a short run.
    let path = scratch("steady-state");
    let mut session = ServerSession::default();
    let client = joined(&mut session, "ada");
    let mut journal = Journal::create(&path, &session).expect("a fresh journal");

    let mut base = Revision::new(0);
    let mut growth = Vec::new();
    for seq in 1..=6 {
        let before = std::fs::metadata(&path).expect("stat").len();
        let offered = chunk(client, seq, Base::Revision(base), "text");
        let Outcome::Ordered { revision } = session.commit(&offered) else {
            panic!("chunk {seq} must order");
        };
        journal.append(&offered, revision).expect("append");
        base = revision;
        growth.push(std::fs::metadata(&path).expect("stat").len() - before);
    }
    let first = growth[0];
    assert!(
        growth.iter().all(|&step| step == first),
        "appending identical chunks grew the file by {growth:?} bytes — a step that changes with \
         the history length means the write is O(history), not O(chunk)"
    );
    assert!(
        first > 0 && first < CHUNK_BUDGET_BYTES as u64,
        "one appended chunk grew the file by {first} bytes, which is not one chunk-sized frame"
    );
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------------------------
// The admission record — ADR-058, amended. Each of these CREATES the condition rather than
// relying on a room that happens to be in it: the participants join AFTER the opening
// checkpoint, so their admission is in the appended tail and nowhere else, which is the only
// arrangement a crash can lose.
// ---------------------------------------------------------------------------------------------

/// A room, a journal, and two participants admitted *after* the opening checkpoint.
///
/// Returns the path, so a caller can reopen it as a second process would.
fn room_with_two_admitted_after_the_checkpoint(
    name: &str,
) -> (std::path::PathBuf, ClientId, ClientId) {
    let path = scratch(name);
    let mut room = Room::create(&path).expect("a new room");
    let ada = welcomed(&mut room, "ada", None);
    let grace = welcomed(&mut room, "grace", None);
    let offered = chunk(ada, 1, Base::Revision(Revision::new(0)), "a");
    let Outcome::Ordered { .. } = room.commit(&offered).expect("the journal accepts") else {
        panic!("ada's chunk must order");
    };
    drop(room);
    (path, ada, grace)
}

/// Joins `who` through a [`Room`] — so the admission goes through the journal — and returns the
/// participant number.
fn welcomed(room: &mut Room, who: &str, resume: Option<Resume>) -> ClientId {
    let answer = room
        .join(
            &ClientMessage::Join(Join {
                protocol: PROTOCOL_VERSION,
                identity: Identity::new(who).expect("an identity"),
                grant: None,
                resume,
            }),
            Capabilities::owner(),
        )
        .expect("the admission is journalled");
    match answer {
        ServerMessage::Welcome { client, .. } | ServerMessage::Resumed { client, .. } => client,
        other => panic!("expected an admission, got {other:?}"),
    }
}

#[test]
fn a_participant_number_is_never_handed_out_twice_across_a_crash() {
    // THE defect this record exists for, and it was measured before it was guarded rather than
    // deduced. With `join` not journalled, recovery restored `next_client` from the last
    // checkpoint while the dedupe table was rebuilt by replaying chunks — so the counter went
    // backwards past entries that already existed. The probe printed:
    //
    //   BEFORE  ada=ClientId(0) grace=ClientId(1) head=Revision(1)
    //   AFTER   head=Revision(1) replayed=1 has_assigned(ada)=false has_assigned(grace)=false
    //   SUBMIT  the holder of number 0, seq 1 -> Duplicate { revision: Revision(1) }
    //
    // The last line is the harm: a NEW participant handed number 0 has its first chunk answered
    // `Duplicate` and silently dropped, because ada's `(client, seq)` entry is still there. It is
    // the same harm ADR-060's forged-submission fix closed, reachable through a crash instead of
    // a forgery — and because a participant number IS an `IdSpace` (ADR-051), two live replicas
    // would also be minting colliding `NodeId`s.
    let (path, ada, grace) = room_with_two_admitted_after_the_checkpoint("number-reuse");

    let (mut room, recovered) = Room::open(&path).expect("recovery");
    assert_eq!(
        recovered.readmitted, 2,
        "both admissions must be replayed, or this guard is testing a room that never admitted \
         anybody after its checkpoint"
    );

    // Somebody new joins after the restart. Their number must be above both of the old ones.
    let fresh = welcomed(&mut room, "hopper", None);
    assert!(
        fresh != ada && fresh != grace,
        "the restart handed out {fresh:?} again, which ada or grace already holds — so the new \
         participant inherits a dedupe entry and a minting space that are not theirs"
    );

    // And the harm itself, asserted rather than inferred: the newcomer's own first chunk is
    // ordered and not swallowed as a duplicate of somebody else's work.
    let theirs = chunk(fresh, 1, Base::Revision(room.session().head()), "n");
    assert!(
        matches!(
            room.commit(&theirs).expect("the journal accepts"),
            Outcome::Ordered { .. }
        ),
        "the newcomer's first chunk was not ordered, so it inherited somebody else's dedupe entry"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_client_whose_work_was_acknowledged_can_still_resume_after_a_crash() {
    // The second half of the same defect, and the handover's description of it was WRONG in a way
    // worth recording: it said such a client "is told `TooFarBehind`". Measured, it is told
    // `Welcome`, which is worse. `TooFarBehind` is announced loss — `ODC-7006` exists for exactly
    // that — while a `Welcome` plus a snapshot discards the unacknowledged work that `152` §5.5
    // says a resume exists to preserve, and says nothing at all. Silent loss.
    let path = scratch("resume-after-crash");
    let key = ResumeKey::new("ada-key").expect("a key");
    let mut room = Room::create(&path).expect("a new room");
    let ada = welcomed(
        &mut room,
        "ada",
        Some(Resume {
            key: key.clone(),
            revision: Revision::new(0),
        }),
    );
    let offered = chunk(ada, 1, Base::Revision(Revision::new(0)), "a");
    let Outcome::Ordered { revision } = room.commit(&offered).expect("the journal accepts") else {
        panic!("ada's chunk must order");
    };
    drop(room);

    let (mut room, _) = Room::open(&path).expect("recovery");
    let answer = room
        .join(
            &ClientMessage::Join(Join {
                protocol: PROTOCOL_VERSION,
                identity: Identity::new("ada").expect("an identity"),
                grant: None,
                resume: Some(Resume { key, revision }),
            }),
            Capabilities::owner(),
        )
        .expect("the admission is journalled");
    match answer {
        ServerMessage::Resumed { client, .. } => assert_eq!(
            client, ada,
            "a resume must hand back the SAME participant number, or the dedupe table no longer \
             recognises this client's chunks"
        ),
        other => panic!(
            "ada's resume key survived the crash but she was not resumed: {other:?}. A `Welcome` \
             here replaces her document and discards the unacknowledged work a resume exists to \
             preserve, with no `TooFarBehind` and no announcement — silent loss."
        ),
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_chunk_from_a_participant_this_room_never_admitted_is_refused() {
    // ADR-060 wanted this check in `commit` and could not have it, because a membership table
    // was not durable and recovery refused the relay's own file. The `Admitted` record is what
    // made it possible, so the check is back where it belongs — in the state machine, where a
    // third-party relay driving `ServerSession` gets it without knowing to ask.
    let path = scratch("never-admitted");
    let mut room = Room::create(&path).expect("a new room");
    let nobody = ClientId::new(7);
    let offered = chunk(nobody, 1, Base::Revision(Revision::new(0)), "x");
    assert_eq!(
        room.commit(&offered).expect("the journal is not touched"),
        Outcome::Refused {
            reason: Refusal::NotAuthorised
        },
        "a chunk naming a participant number nobody was ever handed was ordered"
    );
    // And it is refused BEFORE the dedupe table is consulted, so an unadmitted sender cannot
    // learn where somebody else's work landed by probing seq numbers.
    let ada = welcomed(&mut room, "ada", None);
    let theirs = chunk(ada, 4, Base::Revision(Revision::new(0)), "a");
    let Outcome::Ordered { revision } = room.commit(&theirs).expect("the journal accepts") else {
        panic!("ada's chunk must order");
    };
    let probe = chunk(ada, 4, Base::Revision(Revision::new(0)), "a");
    assert_eq!(
        room.commit(&probe).expect("no journal write"),
        Outcome::Duplicate { revision },
        "ada's own resend must still be recognised — the membership check must not break dedupe"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_admission_is_durable_before_the_room_says_who_you_are() {
    // The join half of `Room::commit`'s contract. A participant that has been handed a number
    // must not be able to outlive the record of it, so the journal write happens before the
    // `Welcome` is returned — read here by a second `Room::open` while the first is still alive,
    // which is how a second process would see it.
    //
    // What this does NOT prove, said plainly: the `fsync`. A second open in the same process
    // reads the page cache, so removing `sync_data` leaves this green. Durability against power
    // loss is not observable from one process, so that line is reviewed rather than tested.
    let path = scratch("admission-durable");
    let mut room = Room::create(&path).expect("a new room");
    let ada = welcomed(&mut room, "ada", None);

    let (_, recovered) = Room::open(&path).expect("the record is already there");
    assert_eq!(
        recovered.readmitted, 1,
        "ada's admission was not in the file at the moment she was told her number"
    );
    assert_eq!(
        recovered.session.granted_for(ada),
        Some(Capabilities::owner()),
        "the admission was recorded without the terms it was granted on"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_retained_entry_that_follows_no_checkpoint_is_refused() {
    // A `Retained` frame is only meaningful immediately after the checkpoint it belongs to.
    // Attaching an orphan to an earlier checkpoint would rebuild the WRONG retained window — a
    // relay that then answers `history_since` from it tells a resuming client it is caught up
    // when it is not. So it is refused, with its own error rather than a reused "corrupt frame":
    // the bytes parsed perfectly and telling an operator otherwise sends them looking at a disk.
    let path = scratch("orphan-entry");
    let mut session = ServerSession::default();
    let client = joined(&mut session, "ada");
    let mut journal = Journal::create(&path, &session).expect("a fresh journal");
    let offered = chunk(client, 1, Base::Revision(Revision::new(0)), "a");
    let Outcome::Ordered { revision } = session.commit(&offered) else {
        panic!("it must order");
    };
    journal.append(&offered, revision).expect("append");
    drop(journal);

    // Append a `Retained` frame after an `Ordered` one — the shape `compact` never writes.
    let entry = session
        .history_since(Revision::new(0))
        .expect("a retained window")
        .next()
        .expect("one entry")
        .clone();
    let mut file = OpenOptions::new().append(true).open(&path).expect("reopen");
    file.write_all(&encode_frame(&super::Record::Retained(entry)))
        .expect("write");
    file.sync_data().expect("sync");

    match Journal::open(&path) {
        Err(JournalError::OrphanEntry { at }) => assert!(
            at > 0,
            "the error must name where the misplaced frame was, or an operator cannot act on it"
        ),
        other => panic!("an orphan retained entry was accepted: {other:?}"),
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_checkpoint_round_trips_its_whole_retained_window_as_separate_frames() {
    // The shape change, asserted as a guarantee: splitting the history out of the checkpoint
    // frame must not lose any of it. A `restored` session that dropped entries would answer
    // `history_since` with a shorter window and send a resuming client a snapshot it did not
    // need — or, worse, say `Some` and hand back fewer operations than the client is missing.
    let path = scratch("checkpoint-round-trip");
    let mut session = ServerSession::new(8);
    let client = joined(&mut session, "ada");
    let mut journal = Journal::create(&path, &session).expect("a fresh journal");
    let mut base = Revision::new(0);
    for seq in 1..=5 {
        let offered = chunk(client, seq, Base::Revision(base), "text");
        let Outcome::Ordered { revision } = session.commit(&offered) else {
            panic!("chunk {seq} must order");
        };
        journal.append(&offered, revision).expect("append");
        base = revision;
    }
    journal.compact(&session).expect("a checkpoint");
    drop(journal);

    let (_, recovered) = Journal::open(&path).expect("recovery");
    assert_eq!(
        recovered.replayed, 0,
        "after compaction there is nothing to replay — that is what compaction IS"
    );
    assert_eq!(
        recovered.session.head(),
        session.head(),
        "the order moved across the checkpoint"
    );
    let before: Vec<_> = session
        .history_since(Revision::new(0))
        .expect("a window")
        .cloned()
        .collect();
    let after: Vec<_> = recovered
        .session
        .history_since(Revision::new(0))
        .expect("a window after recovery")
        .cloned()
        .collect();
    assert_eq!(
        after, before,
        "the retained window did not survive being written as one frame per entry"
    );
    assert_eq!(
        before.len(),
        5,
        "the window must be populated, or this proves nothing"
    );
    let _ = std::fs::remove_file(&path);
}

/// Orders `before` chunks, compacts, then orders `after` more — and returns what a restart had
/// to replay.
///
/// The condition is built rather than waited for: `should_compact` fires at `CHECKPOINT_EVERY`
/// (1024), and a guard that ordered 1024 chunks to reach a checkpoint would be measuring the
/// constant instead of the rule.
fn replayed_after_a_checkpoint(name: &str, before: u64, after: u64) -> (usize, usize) {
    let path = scratch(name);
    let mut session = ServerSession::default();
    let client = joined(&mut session, "ada");
    let mut journal = Journal::create(&path, &session).expect("a fresh journal");
    let mut seq = 0_u64;
    let order = |session: &mut ServerSession, journal: &mut Journal, seq: &mut u64| {
        *seq += 1;
        let head = session.head();
        let offered = chunk(client, *seq, Base::Revision(head), "a");
        let Outcome::Ordered { revision } = session.commit(&offered) else {
            panic!("chunk {seq} must order");
        };
        journal.append(&offered, revision).expect("it journals");
    };
    for _ in 0..before {
        order(&mut session, &mut journal, &mut seq);
    }
    journal.compact(&session).expect("it compacts");
    for _ in 0..after {
        order(&mut session, &mut journal, &mut seq);
    }
    drop(journal);

    let (_, recovered) = Journal::open(&path).expect("it reopens");
    let ordered_in_total = usize::try_from(before + after).expect("a small count");
    assert_eq!(
        recovered
            .session
            .history_since(Revision::new(0))
            .map_or(0, |entries| entries.len()),
        ordered_in_total,
        "recovery must have the whole order, whatever it had to replay to get it — a cheap \
         replay that lost ordered work would satisfy a bound and nothing else"
    );
    let _ = std::fs::remove_file(&path);
    (recovered.replayed, recovered.readmitted)
}

/// **B5/B7: cold replay is bounded by the checkpoint, not by the log's history** — `107` §4.
///
/// §4 states it in those words — "the number bounds **recovery time** rather than file size,
/// which is why it counts records: recovery replays that many O(1) `commit` calls" — and the
/// only guard on it was `compaction_leaves_one_checkpoint_and_nothing_to_replay`, which
/// measures the **degenerate** case: compact, reopen, replay zero. Zero is consistent with a
/// bound and equally consistent with no bound at all, because a log with nothing after its
/// checkpoint replays nothing however recovery is written.
///
/// So this asserts the quantity instead, in the two directions that together are the rule:
///
/// 1. **Replay tracks what follows the checkpoint**, and the guard can see it move: *n* and
///    *2n* appends after a checkpoint replay *n* and *2n*. A recovery that replayed the whole
///    file would answer `before + after` for both.
/// 2. **Replay does NOT track the history before it.** Four times the pre-checkpoint history
///    with the same tail replays the same number. This is the half that is actually the bound,
///    and the half the degenerate case cannot express.
///
/// No clock and no baseline: it counts records, which is the unit §4 chose precisely because a
/// millisecond cannot tell a bounded replay from a fast one on a quiet machine. `SKILL` §8's
/// rule — guard complexity, not milliseconds, by comparing *n* against *2n*.
#[test]
fn cold_replay_is_bounded_by_the_checkpoint_and_not_by_the_log_s_history() {
    // One participant joins before the checkpoint, so `readmitted` is 0 throughout and the
    // admission half of recovery is asserted not to be replaying history either.
    let (short_tail, readmitted_short) = replayed_after_a_checkpoint("replay-n", 8, 5);
    let (long_tail, _) = replayed_after_a_checkpoint("replay-2n", 8, 10);
    let (long_history, readmitted_long) = replayed_after_a_checkpoint("replay-history", 32, 5);

    // The bound first, because it is the claim: four times the pre-checkpoint history, the same
    // tail, the same replay. Asserted ahead of the quantities so that a recovery which lost the
    // bound reports losing the bound, rather than reporting whichever count happened to be
    // compared first.
    assert_eq!(
        long_history, short_tail,
        "four times the pre-checkpoint history replayed {long_history} rather than \
         {short_tail}: recovery cost is growing with the log's lifetime, which is exactly what \
         `CHECKPOINT_EVERY` exists to stop and what `107` §4 B7 claims it does"
    );
    assert_eq!(
        short_tail, 5,
        "recovery replayed {short_tail} chunks for a 5-chunk tail, so it is not replaying the \
         tail — it is replaying something else"
    );
    assert_eq!(
        long_tail, 10,
        "doubling the tail must double the replay, or this guard cannot see the quantity it \
         claims to bound"
    );
    assert_eq!(
        (readmitted_short, readmitted_long),
        (0, 0),
        "an admission before the checkpoint is IN the checkpoint, so re-admitting it would be \
         the same unbounded growth in the other half of recovery"
    );
}
