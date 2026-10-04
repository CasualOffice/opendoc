// SPDX-License-Identifier: Apache-2.0

//! The room's one guarantee: **journal first, answer second.**

use casual_doc_edit::access::Capabilities;
use casual_doc_edit::{Mint, Operation, Pos};
use casual_doc_model::{IdGenerator, NodeId};
use casual_doc_transaction::protocol::{
    Base, ClientId, ClientMessage, Identity, Join, Outcome, PROTOCOL_VERSION, Revision, Seq,
    ServerMessage, Submission,
};

use super::Room;

fn scratch(name: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "opendoc-room-{}-{name}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos())
    ));
    path
}

fn chunk(client: ClientId, seq: u64, base: Base) -> Submission {
    let node = NodeId::new(0x71).expect("id");
    let mint = Mint::reserve(&mut IdGenerator::new(0x80 + seq), 1).expect("a mint");
    Submission {
        client,
        seq: Seq::new(seq),
        base,
        operations: vec![casual_doc_transaction::wire::WireOperation::of(
            Operation::InsertText {
                at: Pos::new(node, 0),
                text: "z".to_owned(),
            },
            mint,
        )],
    }
}

#[test]
fn a_chunk_is_durable_before_the_room_says_it_is_ordered() {
    // The contract, asserted from the outside: by the time `commit` has answered "ordered", a
    // fresh `open` of the same path already sees it. If the room answered first and journalled
    // later there would be a window in which the answer existed and the record did not — and the
    // client drops the chunk from its outstanding set on that answer, so the work would be gone.
    //
    // **What this does NOT prove, said plainly.** It proves the *ordering* of the write relative
    // to the answer. It does not prove the `fsync`: a second `open` in the same process reads the
    // page cache, so removing `sync_data` would leave this green. Durability against power loss
    // is not observable from a unit test in one process, so that line is reviewed rather than
    // tested, and a guard claiming otherwise would be the kind of evidence this repository has
    // been bitten by.
    let path = scratch("durable-before-answer");
    let mut room = Room::create(&path).expect("a new room");
    let ServerMessage::Welcome { client, .. } = room
        .join(
            &ClientMessage::Join(Join {
                protocol: PROTOCOL_VERSION,
                identity: Identity::new("ada").expect("an identity"),
                grant: None,
                resume: None,
            }),
            Capabilities::owner(),
        )
        .expect("the admission is journalled")
    else {
        panic!("expected a welcome");
    };

    let offered = chunk(client, 1, Base::Revision(Revision::new(0)));
    let Outcome::Ordered { revision } = room.commit(&offered).expect("the journal accepts") else {
        panic!("it must order");
    };

    // Read the file as a second process would, while the first room is still open.
    let (_, recovered) = Room::open(&path).expect("the record is already there");
    assert_eq!(
        recovered.replayed, 1,
        "the chunk must be on disk by the time the room has said it is ordered"
    );
    assert_eq!(
        recovered
            .session
            .history_since(Revision::new(0))
            .and_then(|mut entries| entries.next().map(|entry| entry.revision)),
        Some(revision),
        "and at the revision the room answered with"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_room_cannot_be_created_twice_so_a_client_cannot_create_one() {
    // "One doc, one room", and the half of it that is enforceable here: the host creates the
    // room. There is no API that creates one as a side effect of a client arriving, and creating
    // over an existing one is refused rather than silently resetting the order.
    let path = scratch("host-creates");
    Room::create(&path).expect("the host creates it");
    assert!(
        Room::create(&path).is_err(),
        "a second create must be refused: it would destroy the order clients were acknowledged \
         against"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn opening_a_room_that_does_not_exist_is_an_error_and_not_an_empty_room() {
    let path = scratch("absent");
    assert!(
        Room::open(&path).is_err(),
        "an absent room must not be served as an empty one: every client would be told the \
         document is at revision zero and silently lose everything already ordered"
    );
}
